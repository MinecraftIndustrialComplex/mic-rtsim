package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Machine cycle evaluators for the glass box (GLASSBOX.md section 2, G-B5): each method re-enacts one
 * Create 6.0.10 block entity's own tick arithmetic on the JVM, step by step, in the types Create uses
 * (float where Create uses float, int where int), and counts ticks. Nothing here is a closed form
 * except where the source itself is integer arithmetic with no loop (fan, funnel, boiler).
 *
 * <p>Pure Java: no Create or Minecraft import. {@link #clamp}, {@link #lerp}, {@link #ceil} and
 * {@link #log2} copy {@code net.minecraft.util.Mth} exactly. Config values Create reads at runtime are
 * parameters, so the caller passes the live config. {@code rpm} is always the magnitude of
 * {@code KineticBlockEntity.getSpeed()} and must be positive and finite.
 *
 * <p>"Tick" means one call of that block entity's {@code tick()} on the server. Where a period depends
 * on another block entity (a depot, a belt), it is stated whether the level's ticker order matters; the
 * depot and belt co-simulations below run both orders and agree unless a method says otherwise.
 *
 * <p>Results are cached per key in a plain HashMap: call from the server thread only.
 *
 * <p>Calibration ({@code gametest/GlassCalibrationGameTests}, each evaluator beside the real block entity,
 * 16 to 256 RPM, fed from the test's tick): exact, to the tick, for {@link #millstone}, {@link #crusher}
 * (n 1 and 16), {@link #pressDepot}, {@link #pressBelt} (single items and stacks of 4, belt at the press's speed),
 * {@link #mixerSteady} (and {@link #NEVER} at 16 RPM), {@link #saw}, {@link #deployerDepot},
 * {@link #crafter(int, float, int)} (d 1 and 2, targets placed after their sources), {@link #fanTicks}
 * (n 1, 16, 17, 64), {@link #beltInsertMoves}, {@link #funnelPeriod} (andesite 1 item, brass 64),
 * {@link #chuteGrabPeriod} (16 per grab), {@link #pumpTransfer}, and {@link #armCycle} (to 0.03 ticks).
 * Everything else is traced from the decompile only.
 */
public final class Cycles {
    private Cycles() {}

    /** Returned when the machine never completes an operation at these inputs: omit the rate entry. */
    public static final int NEVER = -1;

    /** A plain chute's extraction amount ({@code ChuteBlockEntity.getExtractionAmount}). */
    public static final int CHUTE_EXTRACTION = 16;

    /** Ticks per operation as a rational, for Rate24: {@code num / den} ticks per operation. */
    public record Ratio(long num, long den) {
        public double ticks() {
            return (double) num / (double) den;
        }
    }

    private record Key(int kind, int a, int b, int c, int d) {}

    private static final Map<Key, Long> CACHE = new HashMap<>();
    private static final int STEP_LIMIT = 10_000_000;

    private static final int K_MILL = 1, K_CRUSH = 2, K_CRUSH_APPLY = 3, K_PRESS = 4, K_PRESS_DEPOT = 5,
            K_MIX_FIRST = 6, K_MIX_STEADY = 7, K_MIX_RESTART = 8, K_SAW = 9, K_SAW_APPLY = 10, K_DEPLOY = 11,
            K_DEPLOY_DEPOT = 12, K_CRAFT = 13, K_ARM = 14, K_ARM_MAX = 15, K_BELT_INS = 16, K_BELT_FOLLOW = 17,
            K_CHUTE = 18, K_SPOUT = 19, K_DRAIN = 20, K_PACKAGER = 21;

    // ---------------------------------------------------------------- Mth, copied exactly

    /** {@code Mth.clamp(float, float, float)}: {@code value < min ? min : Math.min(value, max)}. */
    static float clamp(float value, float min, float max) {
        return value < min ? min : Math.min(value, max);
    }

    /** {@code Mth.clamp(int, int, int)}. */
    static int clamp(int value, int min, int max) {
        return Math.min(Math.max(value, min), max);
    }

    /** {@code Mth.lerp(float, float, float)}: {@code start + delta * (end - start)}. */
    static float lerp(float delta, float start, float end) {
        return start + delta * (end - start);
    }

    /** {@code Mth.ceil(float)}. */
    static int ceil(float value) {
        int i = (int) value;
        return value > (float) i ? i + 1 : i;
    }

    private static final int[] DE_BRUIJN = {
        0, 1, 28, 2, 29, 14, 24, 3, 30, 22, 20, 15, 25, 17, 4, 8, 31, 27, 13, 23, 21, 19, 16, 7, 26, 12, 18, 6, 11, 5,
        10, 9
    };

    /** {@code Mth.log2(int)}: floor of log2, and -1 for 0 (the De Bruijn table quirk). */
    static int log2(int value) {
        boolean pow2 = value != 0 && (value & value - 1) == 0;
        int v = value;
        if (!pow2) {
            int i = value - 1;
            i |= i >> 1;
            i |= i >> 2;
            i |= i >> 4;
            i |= i >> 8;
            i |= i >> 16;
            v = i + 1;
        }
        int ceil = DE_BRUIJN[(int) ((long) v * 125613361L >> 27) & 31];
        return ceil - (pow2 ? 0 : 1);
    }

    private static int bits(float rpm) {
        if (!(rpm > 0.0F) || Float.isInfinite(rpm)) {
            throw new IllegalArgumentException("rpm must be positive and finite: " + rpm);
        }
        return Float.floatToIntBits(rpm);
    }

    private static void guard(int steps) {
        if (steps > STEP_LIMIT) {
            throw new IllegalStateException("cycle simulation did not terminate");
        }
    }

    private interface Sim {
        long run();
    }

    private static long cached(Key key, Sim sim) {
        Long hit = CACHE.get(key);
        if (hit != null) {
            return hit;
        }
        long value = sim.run();
        CACHE.put(key, value);
        return value;
    }

    // ---------------------------------------------------------------- Millstone

    /**
     * Millstone ticks per input item: re-enacts {@code MillstoneBlockEntity.tick}, {@code getProcessingSpeed}
     * ({@code clamp((int) |speed / 16|, 1, 512)}) and {@code process}.
     *
     * <p>Counts the tick that sets {@code timer = duration} (timer at or below 0, input present: no decrement
     * that tick) through the tick whose decrement takes the timer to 0 or below and calls {@code process}
     * (one input item). The next tick sets the timer again, so this is the steady period while input is
     * always present and no output slot is full (a full slot returns before any countdown). Equals
     * {@code ceil(D / s) + 1}.
     *
     * @param duration the milling recipe's processing time, or 100 if no recipe matches (then nothing is
     *     produced and the item stays)
     * @return ticks per item, or {@link #NEVER} when {@code duration <= 0}: the timer is set to 0, the
     *     countdown branch needs {@code timer > 0}, so a 0-time milling recipe never processes
     */
    public static int millstone(int duration, float rpm) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_MILL, duration, 0, 0, rb), () -> {
            if (duration <= 0) {
                return NEVER;
            }
            int speed = clamp((int) Math.abs(rpm / 16.0F), 1, 512);
            int timer = 0;
            int ticks = 0;
            while (true) {
                guard(++ticks);
                if (timer > 0) {
                    timer = timer - speed;
                    if (timer <= 0) {
                        return ticks;
                    }
                } else {
                    timer = duration;
                }
            }
        });
    }

    // ---------------------------------------------------------------- Crushing wheels

    /**
     * Crushing wheel controller ticks per batch: re-enacts {@code CrushingWheelControllerBlockEntity.tick},
     * {@code itemInserted}, {@code applyRecipe} with {@code crushingspeed = |wheelSpeed / 50|}
     * ({@code CrushingWheelControllerBlock.updateSpeed}) and {@code speed = crushingspeed * 4}.
     *
     * <p>Each tick: {@code remainingTime -= clamp(speed / d, 0.25, 20)} with
     * {@code d = (float) log(n) / (float) log(2)} before the recipe is applied and 1 after (n = 1 gives
     * d = 0, speed / 0 = infinity, clamped to 20). The first tick with {@code remainingTime < 20} applies
     * the recipe to all n items and does not output. {@code applyRecipe} empties the inventory with
     * {@code ProcessingInventory.clear()}, which also zeroes {@code remainingTime}, so the next tick (its
     * decrement leaves it below 0) outputs: there is no countdown from 20, and the period is the apply tick
     * plus 1 at every speed. Calibrated in game (GlassCalibrationGameTests, raw iron, 400 ticks): n 1 is 21
     * at 16 to 256 RPM; n 16 is 1189, 595, 298, 150, 76 at 16, 32, 64, 128, 256 RPM, exact.
     *
     * <p>Counts controller ticks from the first tick after the stack enters slot 0 (same game tick if the
     * inserter ticks first) through the tick the output leaves and the inventory clears. Slot 0 accepts
     * only into an empty inventory ({@code ProcessingInventory.isItemValid}), so with a feeder that is
     * always ready this is also the steady period per batch, in either ticker order. Assumes the output
     * is accepted (a belt or depot below that refuses keeps the controller waiting).
     *
     * @param duration the crushing recipe's processing time (falls back to milling, then 100)
     * @param batch items in the inserted stack, 1 to 64
     */
    public static int crusher(int duration, int batch, float wheelRpm) {
        int rb = bits(wheelRpm);
        return (int) cached(new Key(K_CRUSH, duration, batch, 0, rb), () -> crushSim(duration, batch, wheelRpm, false));
    }

    /** The controller tick (same counting as {@link #crusher}) on which {@code applyRecipe} runs. */
    public static int crusherApplyTick(int duration, int batch, float wheelRpm) {
        int rb = bits(wheelRpm);
        return (int) cached(new Key(K_CRUSH_APPLY, duration, batch, 0, rb), () -> crushSim(duration, batch, wheelRpm, true));
    }

    private static long crushSim(int duration, int batch, float wheelRpm, boolean applyTick) {
        if (batch < 1) {
            throw new IllegalArgumentException("batch must be at least 1: " + batch);
        }
        float crushingspeed = Math.abs(wheelRpm / 50.0F);
        float speed = crushingspeed * 4.0F;
        float remaining = (float) duration;
        boolean applied = false;
        int ticks = 0;
        while (true) {
            guard(++ticks);
            float processingSpeed = clamp(
                speed / (!applied ? (float) Math.log((double) batch) / (float) Math.log(2.0) : 1.0F), 0.25F, 20.0F);
            remaining -= processingSpeed;
            if (remaining < 20.0F && !applied) {
                applied = true;
                // applyRecipe empties the inventory through ProcessingInventory.clear(), which zeroes
                // remainingTime: the next tick's decrement takes it below 0 and outputs.
                remaining = 0.0F;
                if (applyTick) {
                    return ticks;
                }
            } else if (!(remaining > 0.0F)) {
                return ticks;
            }
        }
    }

    // ---------------------------------------------------------------- Mechanical press

    private static int pressStep(float rpm) {
        return (int) lerp(clamp(Math.abs(rpm) / 512.0F, 0.0F, 1.0F), 1.0F, 60.0F);
    }

    /**
     * The press's own running time: re-enacts {@code PressingBehaviour.tick} and {@code getRunningTickSpeed}
     * ({@code s = (int) lerp(clamp(|speed| / 512, 0, 1), 1, 60)}). After {@code start()} sets
     * {@code runningTicks = 0}, each press tick adds s; the step that crosses 120 snaps to 120; the tick
     * that starts at 120 applies (world and basin modes; in belt mode the belt or depot applies while it
     * reads 120) and still adds s; the first tick that starts above 240 ({@code CYCLE}) completes and does
     * not add.
     *
     * <p>Counts press ticks from the first one after {@code start()} through the completion tick:
     * {@code ceil(120 / s) + floor(120 / s) + 2}.
     */
    public static int pressRun(float rpm) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_PRESS, 0, 0, 0, rb), () -> {
            int s = pressStep(rpm);
            int runningTicks = 0;
            int ticks = 0;
            while (true) {
                guard(++ticks);
                if (runningTicks > 240) {
                    return ticks;
                }
                int prev = runningTicks;
                runningTicks = runningTicks + s;
                if (prev < 120 && runningTicks >= 120) {
                    runningTicks = 120;
                }
            }
        });
    }

    /**
     * Press ticks from the first one after {@code start()} (counted 1) to the tick that begins at
     * {@code runningTicks == 120}: the basin or world application tick, {@code ceil(120 / s) + 1}.
     */
    public static int pressApplyTick(float rpm) {
        bits(rpm);
        int s = pressStep(rpm);
        int runningTicks = 0;
        int ticks = 0;
        while (true) {
            guard(++ticks);
            if (runningTicks == 120) {
                return ticks;
            }
            int prev = runningTicks;
            runningTicks = runningTicks + s;
            if (prev < 120 && runningTicks >= 120) {
                runningTicks = 120;
            }
        }
    }

    /**
     * Basin compacting or pressing, steady period per craft while the recipe still matches and the basin's
     * spoutput buffers are empty: {@code MechanicalPressBlockEntity.onPressingCompleted} calls
     * {@code startProcessingBasin} inside the completion tick, which resets {@code runningTicks} to 0, and
     * the next press tick adds s. So the period equals {@link #pressRun}. When the recipe stops matching,
     * {@code basinChecker.scheduleUpdate()} defers the restart to the next press tick's
     * {@code DeferralBehaviour}, which starts and adds s in that same tick: that cycle is
     * {@code pressRun + 1}.
     */
    public static int pressBasin(float rpm) {
        return pressRun(rpm);
    }

    /**
     * Press on a depot holding a stack, one item per cycle ({@code bulkPressing} false): steady period
     * between {@code start()} calls. Co-simulates {@code DepotBehaviour.tick} (receive: start and HOLD;
     * held: HOLD until {@code runningTicks == 120}, process one item, HOLD; PASS once not running) with
     * {@code PressingBehaviour.tick}, in both ticker orders. The depot needs one tick to see the press
     * stopped (PASS unlocks) and one more to receive the stack again and start. Equals
     * {@code pressRun + 1} in both orders. Assumes the 8-slot output buffer is emptied.
     */
    public static int pressDepot(float rpm) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_PRESS_DEPOT, 0, 0, 0, rb), () -> {
            int a = pressDepotSim(pressStep(rpm), true);
            int b = pressDepotSim(pressStep(rpm), false);
            return Math.max(a, b);
        });
    }

    private static int pressDepotSim(int s, boolean depotFirst) {
        boolean running = false;
        boolean locked = false;
        int runningTicks = 0;
        int lastStart = -1;
        int period = -1;
        int starts = 0;
        for (int tick = 0; starts < 4; tick++) {
            guard(tick);
            for (int part = 0; part < 2; part++) {
                boolean depot = (part == 0) == depotFirst;
                if (depot) {
                    if (!locked) {
                        if (!running) {
                            running = true;
                            runningTicks = 0;
                            if (lastStart >= 0) {
                                period = tick - lastStart;
                            }
                            lastStart = tick;
                            starts++;
                        }
                        locked = true;
                    } else if (!running) {
                        locked = false;
                    }
                } else if (running) {
                    if (runningTicks > 240) {
                        running = false;
                    } else {
                        int prev = runningTicks;
                        runningTicks += s;
                        if (prev < 120 && runningTicks >= 120) {
                            runningTicks = 120;
                        }
                    }
                }
            }
        }
        return period;
    }

    /**
     * Press over a belt that is never short of items (single items queued 1 block apart, or a stack whose
     * remainder is re-inserted just before the centre), steady period per pressed item. The processed
     * output stays locked under the press until it stops running; the belt releases it on the tick it sees
     * the press stopped (tick N + start, both orders), and the next item reaches the centre and calls
     * {@code start()} on the k-th belt move after that, k = {@link #beltFollowMoves}. Equals
     * {@code pressRun + beltFollowMoves - 1}.
     *
     * <p>Calibrated in game for single items queued at the press's own speed (GlassCalibrationGameTests,
     * belt and press at the same RPM): 151, 76, 39, 21, 11 at 16 to 256 RPM, exact. Stacks too: the press
     * takes one item, the belt puts the remainder back at {@code centre - 1/512} locked
     * ({@code BeltInventory.applyToEachWithin}); once the press stops it is unlocked, waits for the sheet to
     * get 1 block ahead, crosses the centre again and is pressed. Stacks of 4 at 64 and 256 RPM: every
     * ingot pressed, 39 and 11 ticks per sheet, exact. Belt speeds other than the press's are UNVERIFIED.
     */
    public static int pressBelt(float pressRpm, float beltRpm) {
        return pressRun(pressRpm) + beltFollowMoves(beltRpm) - 1;
    }

    // ---------------------------------------------------------------- Mechanical mixer

    private static int mixerProcessingTicks(int duration, float rpm) {
        float speed = Math.abs(rpm);
        float recipeSpeed = 1.0F;
        if (duration != 0) {
            recipeSpeed = (float) duration / 100.0F;
        }
        return Math.max(log2((int) (512.0F / speed)) * ceil(recipeSpeed * 15.0F) + 1, 1);
    }

    /**
     * Mixer, first craft: re-enacts {@code MechanicalMixerBlockEntity.tick}. The basin checker
     * ({@code DeferralBehaviour}, inside {@code super.tick()}) calls {@code startProcessingBasin}
     * ({@code runningTicks = 0}) and the same tick's own code adds 1; ticks 1 to 20 wind up to 20; the next
     * tick, seeing {@code processingTicks < 0}, sets
     * {@code P = max(log2((int) (512 / speed)) * ceil(recipeSpeed * 15) + 1, 1)} without counting down
     * ({@code recipeSpeed = duration / 100}, or 1 when the duration is 0 or the recipe is not a
     * {@code StandardProcessingRecipe}); P more ticks count down and the last applies the recipe.
     *
     * <p>Counts the start tick through the apply tick: {@code 21 + P}. Add 1 for the first craft after the
     * mixer is placed or its chunk loads: {@code processingTicks} is not saved and starts at 0, so the first
     * tick at 20 decrements it to -1 instead of setting P.
     *
     * @param duration the recipe's processing time (0: factor 1, same as 100)
     * @param mediumSpeed {@code CKinetics.mediumSpeed} (30): the mixer's {@code SpeedLevel.MEDIUM}
     *     requirement, {@code |speed| >= mediumSpeed}
     * @return ticks, or {@link #NEVER} below mediumSpeed (the mixer winds down and never applies)
     */
    public static int mixerFirst(int duration, float rpm, float mediumSpeed) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_MIX_FIRST, duration, Float.floatToIntBits(mediumSpeed), 0, rb),
                () -> Math.abs(rpm) >= mediumSpeed ? mixerSim(mixerProcessingTicks(duration, rpm), 0) : NEVER);
    }

    /**
     * Mixer steady period between crafts while the recipe still matches after each craft and the basin
     * could continue before it ({@code BasinOperatingBlockEntity.applyBasinRecipe} then calls
     * {@code continueWithPreviousRecipe}, {@code runningTicks = 20}): the apply tick leaves
     * {@code runningTicks} at 20, the next tick sets P, P ticks count down. Equals {@code P + 1}. Note that
     * the next craft's inputs must already be in the basin at the apply tick; if they arrive later the mixer
     * winds down and {@link #mixerRestart} applies.
     *
     * @return ticks per craft, or {@link #NEVER} below mediumSpeed
     */
    public static int mixerSteady(int duration, float rpm, float mediumSpeed) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_MIX_STEADY, duration, Float.floatToIntBits(mediumSpeed), 0, rb),
                () -> Math.abs(rpm) >= mediumSpeed ? mixerSim(mixerProcessingTicks(duration, rpm), 1) : NEVER);
    }

    /**
     * Mixer period when every craft winds down (the recipe does not match right after a craft but does by
     * the time the mixer stops): start tick, P countdown, apply at {@code 21 + P}, wind down to 40, one tick
     * that stops and schedules the basin checker, and the checker restarts on the next tick. Counts start
     * tick to the stop tick inclusive, which is the period: {@code 40 + P}.
     */
    public static int mixerRestart(int duration, float rpm, float mediumSpeed) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_MIX_RESTART, duration, Float.floatToIntBits(mediumSpeed), 0, rb),
                () -> Math.abs(rpm) >= mediumSpeed ? mixerSim(mixerProcessingTicks(duration, rpm), 2) : NEVER);
    }

    /**
     * The mixer's own code after {@code super.tick()}, from the start tick ({@code runningTicks = 0}, running).
     * mode 0: start to first apply; 1: apply to next apply, continuing; 2: start to the stop tick.
     */
    private static int mixerSim(int p, int mode) {
        int runningTicks = 0;
        int processingTicks = -1;
        int ticks = 0;
        int firstApply = -1;
        while (true) {
            guard(++ticks);
            if (runningTicks >= 40) {
                return ticks;
            }
            if (runningTicks == 20) {
                if (processingTicks < 0) {
                    processingTicks = p;
                } else {
                    processingTicks--;
                    if (processingTicks == 0) {
                        runningTicks++;
                        processingTicks = -1;
                        if (mode == 0) {
                            return ticks;
                        }
                        if (mode == 1) {
                            if (firstApply >= 0) {
                                return ticks - firstApply;
                            }
                            firstApply = ticks;
                            runningTicks = 20;
                        }
                    }
                }
            }
            if (runningTicks != 20) {
                runningTicks++;
            }
        }
    }

    // ---------------------------------------------------------------- Mechanical saw

    /**
     * Saw ticks per inserted item: re-enacts {@code SawBlockEntity.start}, {@code tick}, {@code applyRecipe}.
     * Insertion calls {@code start}: {@code remainingTime = duration * max(1, n / 5)} (int division).
     * Each tick subtracts {@code clamp(|speed| / 24, 1, 128)}; the first tick below 5 applies the recipe and
     * sets {@code remainingTime = 20}; later ticks count down again and the first at or below 0 outputs.
     *
     * <p>Counts saw ticks from the first after insertion through the output tick. Slot 0 accepts only into
     * an empty inventory, so with a feeder that is always ready and an output that accepts, this is the
     * steady period, in either ticker order. With {@code bulkCutting} false (default) the slot limit is 1,
     * so n is 1.
     *
     * @param duration the cutting recipe's processing time, 50 for stonecutting or woodcutting; 10 with
     *     stackCount 1 for an item no recipe matches (start sets 10 without the n / 5 factor; it passes
     *     through unchanged)
     */
    public static int saw(int duration, int stackCount, float rpm) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_SAW, duration, stackCount, 0, rb), () -> sawSim(duration, stackCount, rpm, false));
    }

    /** The saw tick (same counting as {@link #saw}) on which {@code applyRecipe} runs. */
    public static int sawApplyTick(int duration, int stackCount, float rpm) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_SAW_APPLY, duration, stackCount, 0, rb), () -> sawSim(duration, stackCount, rpm, true));
    }

    private static long sawSim(int duration, int stackCount, float rpm, boolean applyTick) {
        float processingSpeed = clamp(Math.abs(rpm) / 24.0F, 1.0F, 128.0F);
        float remaining = (float) (duration * Math.max(1, stackCount / 5));
        boolean applied = false;
        int ticks = 0;
        while (true) {
            guard(++ticks);
            remaining -= processingSpeed;
            if (!(remaining < 5.0F) || applied) {
                if (!(remaining > 0.0F)) {
                    return ticks;
                }
            } else {
                applied = true;
                remaining = 20.0F;
                if (applyTick) {
                    return ticks;
                }
            }
        }
    }

    // ---------------------------------------------------------------- Deployer

    private static int deployerSpeed(float rpm) {
        return (int) clamp(Math.abs(rpm * 2.0F), 8.0F, 512.0F);
    }

    /**
     * Free-running deployer (a target in the world, not a belt or depot below it): re-enacts
     * {@code DeployerBlockEntity.tick}, {@code getTimerSpeed} ({@code u = (int) clamp(|speed * 2|, 8, 512)}),
     * {@code start}, {@code activate}. While {@code timer > 0} a tick only subtracts u; a tick at or below 0
     * changes state: WAITING starts (EXPANDING, 1000), EXPANDING activates (RETRACTING, 1000), RETRACTING
     * waits (WAITING, 500). Counts start tick to the next start tick: {@code 2 ceil(1000/u) + ceil(500/u) + 3},
     * one activation each. Assumes {@code shouldActivate} stays true and no overflow items (either sets a
     * {@code u * 10} wait).
     */
    public static int deployer(float rpm) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_DEPLOY, 0, 0, 0, rb), () -> {
            int u = deployerSpeed(rpm);
            int state = 0; // 0 WAITING, 1 EXPANDING, 2 RETRACTING
            int timer = 0;
            int ticks = 0;
            int startTick = -1;
            while (true) {
                guard(++ticks);
                if (timer > 0) {
                    timer = timer - u;
                } else if (state == 0) {
                    if (startTick >= 0) {
                        return ticks - startTick;
                    }
                    startTick = ticks;
                    state = 1;
                    timer = 1000;
                } else if (state == 1) {
                    state = 2;
                    timer = 1000;
                } else {
                    state = 0;
                    timer = 500;
                }
            }
        });
    }

    /**
     * Ticks a WAITING deployer loses each time it finds items in its fake player's inventory, or overflow
     * not yet taken out ({@code DeployerBlockEntity.tick}): the check sets {@code timer = 10 u}, ten
     * decrements of u run it out, and the next tick checks again. 11 at every speed.
     */
    public static final int DEPLOYER_OVERFLOW_WAIT = 11;

    /**
     * Shearing deployer (USE mode, shears, a sheep in front), one shear per cycle: {@link #deployer} plus
     * one {@link #DEPLOYER_OVERFLOW_WAIT}. Shears are not a BlockItem or bucket and the target is not a
     * belt, so {@code shouldActivate} is true; but the entity interaction captures the wool
     * ({@code DeployerHandler}: {@code entity.captureDrops}, then {@code placeItemBackInInventory} into the
     * fake player), so the next WAITING check moves it to the overflow list and waits 11 ticks. If the
     * overflow is still there at the following check (a hopper pulling it one item per 8 ticks), another 11.
     * So {@code deployer + 11}, or {@code + 22} and more while the overflow drains slowly: 19 or 30 at 256
     * RPM. The corpus measured 19 to 30 (mean 22.67) at 256 RPM; not calibrated by the calibration
     * GameTests. The yield is not the cycle: a sheared sheep regrows only after eating grass
     * ({@code EatBlockGoal}: 1 in 1000 per tick for adults, then the eat animation), a shear on a sheep
     * without wool yields nothing (no overflow wait), and the shears lose durability each use.
     */
    public static int shearingDeployer(float rpm) {
        return deployer(rpm) + DEPLOYER_OVERFLOW_WAIT;
    }

    /**
     * Deployer over a depot holding a stack (USE mode, a held item, an item-application recipe): steady
     * period per application. Co-simulates {@code BeltDeployerCallbacks.onItemReceived}/{@code whenItemHeld}
     * (from the depot's tick: start when WAITING; apply when {@code RETRACTING && timer == 1000}) with the
     * deployer's tick, in both orders. The depot restarts the deployer as soon as it reads WAITING, so the
     * 500 wait is cut: {@code 2 ceil(1000/u) + 2}.
     */
    public static int deployerDepot(float rpm) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_DEPLOY_DEPOT, 0, 0, 0, rb), () -> {
            int u = deployerSpeed(rpm);
            return Math.max(deployerDepotSim(u, true), deployerDepotSim(u, false));
        });
    }

    private static int deployerDepotSim(int u, boolean depotFirst) {
        int state = 0;
        int timer = 0;
        boolean locked = false;
        int lastStart = -1;
        int period = -1;
        int starts = 0;
        for (int tick = 0; starts < 4; tick++) {
            guard(tick);
            for (int part = 0; part < 2; part++) {
                boolean depot = (part == 0) == depotFirst;
                if (depot) {
                    boolean start = false;
                    if (!locked) {
                        locked = true;
                        start = state == 0;
                    } else if (!(state == 2 && timer == 1000)) {
                        start = state == 0;
                    }
                    if (start) {
                        state = 1;
                        timer = 1000;
                        if (lastStart >= 0) {
                            period = tick - lastStart;
                        }
                        lastStart = tick;
                        starts++;
                    }
                } else if (timer > 0) {
                    timer -= u;
                } else if (state == 1) {
                    state = 2;
                    timer = 1000;
                } else if (state == 2) {
                    state = 0;
                    timer = 500;
                } else {
                    timer = u * 10;
                }
            }
        }
        return period;
    }

    /**
     * Deployer over a belt that is never short of items: the outputs move off from the belt tick after the
     * application (outputs are re-inserted unlocked), the next item reaches the centre k belt moves later
     * ({@link #beltFollowMoves}) and is held until the deployer reads WAITING. Period
     * {@code max(2 ceil(1000/u) + 2, ceil(1000/u) + 1 + k)}. UNVERIFIED in game (same belt caveats as
     * {@link #pressBelt}).
     */
    public static int deployerBelt(float deployerRpm, float beltRpm) {
        int u = deployerSpeed(deployerRpm);
        int c = (1000 + u - 1) / u;
        int k = beltFollowMoves(beltRpm);
        return Math.max(deployerDepot(deployerRpm), c + 1 + k);
    }

    // ---------------------------------------------------------------- Mechanical crafter

    /**
     * Mechanical crafters, the critical path from {@code begin()} to the final crafter's output: re-enacts
     * {@code MechanicalCrafterBlockEntity.tick}, {@code begin}, {@code continueIfAllPrecedingFinished},
     * {@code getCountDownSpeed} ({@code c = clamp((int) |speed|, 4, 250)}). Every crafter on the path spends
     * one tick in ASSEMBLING ({@code countDown = 1}, minus c, below 0); a crafter with a target then
     * EXPORTS ({@code countDown 1000}, {@code floor(1000/c) + 1} ticks, the last merges into the target and
     * sets it ASSEMBLING); the final crafter CRAFTS ({@code countDown 2000}, {@code floor(2000/c) + 1} ticks,
     * the last inserts the result and, if fully inserted, ejects the grid to IDLE).
     *
     * <p>Counts crafter ticks along the longest chain, starting with the leaf's first tick after the grid
     * completed (the insertion that completes the grid calls {@code begin} for all), ending with the
     * output tick: {@code d (floor(1000/c) + 2) + floor(2000/c) + 2}. Each hand-off whose target ticks after
     * its source in the level's ticker list runs the target's ASSEMBLING tick in the same game tick: see
     * {@link #crafter(int, float, int)}. Empty grid cells export with {@code countDown 0}, one tick, and can
     * shorten a chain that only carries empties: not modelled here.
     *
     * @param handOffs d, the number of hand-offs on the longest chain to the final crafter (0 for one crafter)
     */
    public static int crafter(int handOffs, float rpm) {
        return crafter(handOffs, rpm, 0);
    }

    /**
     * As {@link #crafter(int, float)} in game ticks, minus one per hand-off whose target ticks after its
     * source in the same game tick ({@code sameTickHandOffs}, 0 to handOffs). With all of them,
     * {@code d (floor(1000/c) + 1) + floor(2000/c) + 2}.
     */
    public static int crafter(int handOffs, float rpm, int sameTickHandOffs) {
        if (sameTickHandOffs < 0 || sameTickHandOffs > handOffs) {
            throw new IllegalArgumentException("sameTickHandOffs out of range: " + sameTickHandOffs);
        }
        int rb = bits(rpm);
        int sequential = (int) cached(new Key(K_CRAFT, handOffs, 0, 0, rb), () -> {
            int c = clamp((int) Math.abs(rpm), 4, 250);
            int ticks = 0;
            for (int crafter = 0; crafter <= handOffs; crafter++) {
                int countDown = 1;
                do {
                    guard(++ticks);
                    countDown = countDown - c;
                } while (!(countDown < 0));
                countDown = crafter < handOffs ? 1000 : 2000;
                do {
                    guard(++ticks);
                    countDown = countDown - c;
                } while (!(countDown < 0));
            }
            return ticks;
        });
        return sequential - sameTickHandOffs;
    }

    // ---------------------------------------------------------------- Fans

    /**
     * Ticks a stack on a belt or depot must stay in one air current type to be processed: re-enacts
     * {@code FanProcessing.applyProcessing(TransportedItemStack, ...)}, called once per tick by
     * {@code AirCurrent.tickAffectedHandlers}. The first call tags the stack and sets
     * {@code processingTime = T * ((n - 1) / 16 + 1) + 1}; each later call tests {@code processingTime-- > 0}
     * and processes on the first call that reads 0. Counts the tagging call through the processing call:
     * {@code T * ((n - 1) / 16 + 1) + 3}. Independent of RPM. Outputs inherit {@code processedBy} and
     * {@code processingTime -1}, so they are not processed again by the same type.
     *
     * @param fanProcessingTime {@code CKinetics.fanProcessingTime} (150)
     */
    public static int fanTicks(int stackCount, int fanProcessingTime) {
        int timeModifierForStackSize = (stackCount - 1) / 16 + 1;
        int processingTime = fanProcessingTime * timeModifierForStackSize + 1;
        return processingTime + 2;
    }

    /**
     * As {@link #fanTicks} for an item entity in the current ({@code FanProcessing.applyProcessing(ItemEntity,
     * ...)} via {@code decrementProcessingTime}): the first call stores {@code T * m + 1} and returns it minus
     * 1, processing when the returned value is 0, so {@code T * m + 1} calls. An entity is only caught by
     * {@code AirCurrent.findEntities}, run every 6 ticks ({@code entitySearchCooldown 5}), so up to 5 more
     * ticks before the first call.
     */
    public static int fanTicksEntity(int stackCount, int fanProcessingTime) {
        return fanProcessingTime * ((stackCount - 1) / 16 + 1) + 1;
    }

    /**
     * Fan reach before obstacles: {@code IAirCurrentSource.getMaxDistance},
     * {@code lerp(min(|speed| / argmax, 1), 3, distance)} in float. {@code AirCurrent.getFlowLimit} then cuts
     * it at the first blocking shape.
     *
     * @param rotationArgmax {@code CKinetics.fanRotationArgmax} (256)
     * @param distanceConfig {@code fanPushDistance} (20) when pushing, {@code fanPullDistance} (20) when pulling
     */
    public static float fanRange(float rpm, int rotationArgmax, int distanceConfig) {
        float speed = Math.abs(rpm);
        float distanceFactor = Math.min(speed / (float) rotationArgmax, 1.0F);
        return lerp(distanceFactor, 3.0F, (float) distanceConfig);
    }

    // ---------------------------------------------------------------- Mechanical arm

    /** Arm ticks per movement phase without lazy-tick help: progress {@code += min(256, |speed|) / 1024} to 1. */
    public static int armMoveTicks(float rpm) {
        bits(rpm);
        float step = Math.min(256.0F, Math.abs(rpm)) / 1024.0F;
        float progress = 0.0F;
        int ticks = 0;
        while (progress < 1.0F) {
            guard(++ticks);
            progress = progress + step;
            if (progress > 1.0F) {
                progress = 1.0F;
            }
        }
        return ticks;
    }

    /**
     * Arm, one full cycle (search inputs, move to input, search outputs, move to output) when the input
     * always has items and the output always accepts, without the periodic lazy tick: {@code 4 k},
     * k = {@link #armMoveTicks}. This is the upper bound; see {@link #armCycle}.
     */
    public static int armCycleMax(float rpm) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_ARM_MAX, 0, 0, 0, rb), () -> armOneCycle(armStep(rpm), Integer.MAX_VALUE / 2, 10, false)[0]);
    }

    /**
     * Arm long-run ticks per cycle, exact: re-enacts {@code ArmBlockEntity.tick}, {@code tickMovementProgress},
     * {@code lazyTick} together with {@code SmartBlockEntity.tick}'s lazy counter (rate 10: every 11 ticks).
     * Each phase advances {@code chasedPointProgress} by {@code min(256, |speed|) / 1024} (float, capped at
     * 1); at 1 the arm searches inputs, collects, or deposits in that tick; SEARCH_OUTPUTS has no branch at
     * 1 and is ended by the {@code lazyTick()} that a just-reached target triggers. The periodic lazy tick
     * (before the progress step) also searches for a destination whenever the phase is SEARCH_OUTPUTS and
     * progress is at least 0.5, cutting that phase short. So the cycle depends on the lazy counter's phase;
     * the arm and counter form a deterministic joint orbit. This returns the slowest orbit's mean over all
     * 11 counter phases (the throughput lower bound) as ticks per cycle.
     *
     * <p>Assumes inputs always ready and outputs always accepting at the moment of each search; an output
     * that refuses is retried only on lazy ticks, so its wait is phase dependent.
     */
    public static Ratio armCycle(float rpm) {
        int rb = bits(rpm);
        long packed = cached(new Key(K_ARM, 0, 0, 0, rb), () -> {
            float step = armStep(rpm);
            int lazyRate = 10;
            long worstNum = 0;
            long worstDen = 1;
            for (int c0 = 0; c0 <= lazyRate; c0++) {
                Map<Integer, Integer> seenAt = new HashMap<>();
                List<Integer> lengths = new ArrayList<>();
                int counter = c0;
                while (!seenAt.containsKey(counter)) {
                    seenAt.put(counter, lengths.size());
                    int[] r = armOneCycle(step, counter, lazyRate, true);
                    lengths.add(r[0]);
                    counter = r[1];
                }
                long num = 0;
                long den = 0;
                for (int i = seenAt.get(counter); i < lengths.size(); i++) {
                    num += lengths.get(i);
                    den++;
                }
                if (num * worstDen > worstNum * den) {
                    worstNum = num;
                    worstDen = den;
                }
            }
            long g = gcd(worstNum, worstDen);
            return (worstNum / g) << 32 | (worstDen / g);
        });
        return new Ratio(packed >>> 32, packed & 0xFFFFFFFFL);
    }

    private static float armStep(float rpm) {
        return Math.min(256.0F, Math.abs(rpm)) / 1024.0F;
    }

    /** One cycle from the tick after a deposit; returns {ticks, lazy counter after the deposit tick}. */
    private static int[] armOneCycle(float step, int counter, int lazyRate, boolean lazy) {
        final int searchInputs = 0, moveToInput = 1, searchOutputs = 2, moveToOutput = 3;
        int phase = searchInputs;
        float progress = 0.0F;
        int ticks = 0;
        while (true) {
            guard(++ticks);
            if (lazy && counter-- <= 0) {
                counter = lazyRate;
                if (!(progress < 0.5F) && phase == searchOutputs) {
                    phase = moveToOutput;
                    progress = 0.0F;
                }
            }
            boolean reachedPreviously = progress >= 1.0F;
            progress = progress + step;
            if (progress > 1.0F) {
                progress = 1.0F;
            }
            boolean targetReached = !reachedPreviously && progress >= 1.0F;
            if (!(progress < 1.0F)) {
                if (phase == moveToInput) {
                    phase = searchOutputs;
                    progress = 0.0F;
                } else if (phase == moveToOutput) {
                    return new int[] {ticks, counter};
                } else if (phase == searchInputs) {
                    phase = moveToInput;
                    progress = 0.0F;
                }
                if (targetReached && !(progress < 0.5F) && phase == searchOutputs) {
                    phase = moveToOutput;
                    progress = 0.0F;
                }
            }
        }
    }

    private static long gcd(long a, long b) {
        while (b != 0) {
            long t = a % b;
            a = b;
            b = t;
        }
        return a == 0 ? 1 : a;
    }

    // ---------------------------------------------------------------- Funnels, belts, chutes

    /**
     * Extracting funnel ticks per extraction when the target always accepts: re-enacts
     * {@code FunnelBlockEntity.tick} and {@code startCooldown}. The extraction tick sets
     * {@code extractionCooldown = T}; the next T ticks only decrement; the following tick extracts.
     * Period {@code T + 1}. Amount per extraction ({@code getAmountToExtract}): andesite 1 (filtering
     * inactive), brass the filter count (default 64, up to). A belt funnel also waits for the segment to be
     * free; a funnel into the world waits until its last dropped entity leaves the scan box.
     *
     * @param extractionTimer {@code CLogistics.defaultExtractionTimer} (8)
     */
    public static int funnelPeriod(int extractionTimer) {
        return Math.max(0, extractionTimer) + 1;
    }

    /** Belt item speed in blocks per tick: {@code BeltBlockEntity.getBeltMovementSpeed}, {@code speed / 480}. */
    public static float beltSpeed(float rpm) {
        return Math.abs(rpm) / 480.0F;
    }

    /**
     * Belt moves after a side or top insertion before the same side can insert at the same segment again:
     * {@code BeltBlockEntity.tryInsertingFromSide} places the item at {@code index + 0.5 - 1/16} and
     * {@code BeltInventory.canInsertAtFromSide}/{@code isBlocking} refuses while it is at or before
     * {@code index + 0.5 + 1}; counts belt moves of v until it is past (it must travel 1.0625 blocks, not 1).
     * Only earlier insertions at that segment from that side block it ({@code isBlocking} tests
     * {@code insertedAt} and {@code insertedFrom}); items passing by from upstream do not. The insertion
     * waits in {@code toInsert} until the belt's next tick, which also moves it. A funnel's own 9-tick period
     * dominates above about 60 RPM. Calibrated in game (a segment's item capability, one item per
     * insertion): 32, 16, 8, 4, 2 ticks per item at 16 to 256 RPM, exact.
     */
    public static int beltInsertMoves(float rpm) {
        int rb = bits(rpm);
        return (int) cached(new Key(K_BELT_INS, 0, 0, 0, rb), () -> {
            float v = rpm / 480.0F;
            float pos = 0.0F + 0.5F - 1.0F / 16.0F;
            float limit = 0.0F + 0.5F + 1.0F;
            int moves = 0;
            while (pos <= limit) {
                guard(++moves);
                pos += v;
            }
            return moves;
        });
    }

    /**
     * Belt moves for an item queued 1 block behind a processed item held at a processing segment's centre
     * (at {@code centre + 1/512}) to reach that centre once the front item moves on: re-enacts the
     * {@code BeltInventory.tick} spacing rule ({@code movement = min(v, diff - 1)}) and the centre test
     * ({@code segment + 0.5 <= nextOffset}). Used by {@link #pressBelt}, {@link #deployerBelt},
     * {@link #spoutBelt}. UNVERIFIED in game: the queued item's start position is taken as exactly 1.0
     * behind, in segment 0's float precision.
     */
    public static int beltFollowMoves(float beltRpm) {
        int rb = bits(beltRpm);
        return (int) cached(new Key(K_BELT_FOLLOW, 0, 0, 0, rb), () -> {
            float v = beltRpm / 480.0F;
            float centre = 0.5F;
            float front = centre + 0.001953125F;
            float follower = front - 1.0F;
            int moves = 0;
            while (true) {
                guard(++moves);
                front += v;
                float diff = front - follower;
                if (Math.abs(diff) <= 1.0F) {
                    continue;
                }
                float movement = Math.min(v, diff - 1.0F);
                float nextOffset = follower + movement;
                if (centre <= nextOffset) {
                    return moves;
                }
                follower = nextOffset;
            }
        });
    }

    /**
     * Chute item motion in blocks per tick (positive up): {@code ChuteBlockEntity.getItemMotion},
     * {@code (clamp((push + pull) / 64, -20, 20) + (motion <= 0 ? -4 : 0)) / 20} in float. Gravity only:
     * -0.2.
     *
     * @param push signed {@code push} (a fan below facing up: its getSpeed(); else the target chute's push
     *     over the branch count)
     * @param pull signed {@code pull} (a fan above facing down: its getSpeed(); else the sum of input chutes')
     */
    public static float chuteMotion(float push, float pull) {
        float motion = (push + pull) * 0.015625F;
        return (clamp(motion, -20.0F, 20.0F) + (motion <= 0.0F ? -4.0F : 0.0F)) / 20.0F;
    }

    /**
     * Chute ticks for a held stack to pass one downward chute: re-enacts {@code ChuteBlockEntity.tick}'s
     * downward branch. An item handed down from a chute above (or pulled from an inventory above) starts at
     * 1.0; each tick {@code nextOffset = position + motion}; below 0.5 the output is simulated (assumed to
     * accept); below 0 it is handed on. Counts moves until hand-off: 6 under gravity (the fifth leaves
     * {@code +2.98e-8}). The chute is empty after that tick.
     *
     * @param motion {@link #chuteMotion}, negative
     */
    public static int chuteTicksPerBlock(float motion) {
        if (!(motion < 0.0F)) {
            throw new IllegalArgumentException("downward motion must be negative: " + motion);
        }
        return (int) cached(new Key(K_CHUTE, Float.floatToIntBits(motion), 0, 0, 0), () -> {
            float position = 1.0F;
            int ticks = 0;
            while (true) {
                guard(++ticks);
                float nextOffset = position + motion;
                if (nextOffset < 0.5F) {
                    if (nextOffset < 0.0F) {
                        return ticks;
                    }
                }
                position = nextOffset;
            }
        });
    }

    /**
     * Steady period of a top chute pulling from an inventory above into chutes or an inventory below that
     * always accept: one tick grabs ({@code handleInputFromAbove}, no movement that tick) and
     * {@link #chuteTicksPerBlock} ticks move it out, so {@code 1 + chuteTicksPerBlock} ticks per
     * {@link #CHUTE_EXTRACTION} items (7 under gravity). Chutes below are stages of {@link #chuteTicksPerBlock}
     * ticks and do not slow it, in either ticker order. UNVERIFIED in game for the bottom chute's insertion
     * into an inventory (a partial insert keeps the remainder).
     */
    public static int chuteGrabPeriod(float motion) {
        return 1 + chuteTicksPerBlock(motion);
    }

    /**
     * Steps a parked stack takes to leave a downward chute once its target accepts: {@code ChuteBlockEntity.tick}
     * holds a stack at 0.5 while {@code handleDownwardOutput(true)} fails; once it succeeds the stack moves on
     * by {@code motion} a tick and is handed on in the step that takes it below 0. 3 under gravity (0.3, 0.1,
     * then -0.1 hands it on).
     *
     * @param motion {@link #chuteMotion}, negative
     */
    public static int chuteParkedSteps(float motion) {
        if (!(motion < 0.0F)) {
            throw new IllegalArgumentException("downward motion must be negative: " + motion);
        }
        float position = 0.5F;
        int ticks = 0;
        while (true) {
            guard(++ticks);
            float nextOffset = position + motion;
            if (nextOffset < 0.0F) {
                return ticks;
            }
            position = nextOffset;
        }
    }

    /**
     * Ticks a hand-off loop adds to a machine's own cycle when its next input can only arrive after it has
     * emptied (choice GB43). Each actor in the loop (the machine, the mover that empties it if not itself,
     * the parked chute that refills it) acts on its own tick; a hand-off from one to the next costs a tick
     * unless the receiver ticks after the giver in the level's ticker order, in which case it acts in the
     * same tick. Around a loop of {@code actors} block entities that happens for at least one and at most
     * {@code actors - 1} of the hand-offs, whatever the order:
     * <ul>
     *   <li>two actors (a saw or crushing wheels that empty themselves, and the parked chute above): exactly
     *       one, so the cycle is the machine's own plus {@code chuteParkedSteps - 1}, in either order;</li>
     *   <li>three (a depot station, the arm that empties it, the parked chute): one or two by the order,
     *       which the compile does not read; this returns the mean over orders as a ratio
     *       ({@code emptierTicks + chuteParkedSteps - 1 + 1/2}).</li>
     * </ul>
     * The machine's own cycle is the one {@link #saw}, {@link #crusher} and {@link #fanTicks} give: the
     * always-ready feeder that ticks right after it (the calibration tests).
     *
     * @param emptierTicks ticks from the moment the emptier sees the outputs to the tick it takes them
     *     ({@link #armMoveTicks}); 0 when the machine empties itself
     * @param selfEmptying true for a machine that hands its outputs on in its own tick
     */
    public static Ratio chuteHandOff(float motion, int emptierTicks, boolean selfEmptying) {
        int steps = chuteParkedSteps(motion);
        if (selfEmptying) {
            return new Ratio(steps - 1, 1);
        }
        return new Ratio(2L * (emptierTicks + steps - 1) + 1, 2);
    }

    /**
     * A mechanical crafter grid in a hand-off loop with the one downward chute that fills it (choice GB43):
     * ticks from one fill to the next, by the level's tick order. The grid takes items only while IDLE, so
     * the chute's next batch waits parked at 0.5; the crafters run as {@link #crafter(int, float)} counts,
     * but each hand-off (chute to the leaves that begin on the insertion, a crafter's export to its target,
     * the final crafter's output that sets the grid IDLE to the chute's next step) acts in the same game tick
     * when the receiver ticks after the giver, else in the next. In half ticks, with gap(x, y) = 2 when x
     * ticks before y, 0 after, 1 unknown (the mean over orders):
     * <ul>
     *   <li>a leaf's ASSEMBLING tick, from the insertion: gap(leaf, chute);</li>
     *   <li>a target's: the latest of its preceding crafters' {@code a + 2E + gap(target, pred)}, E =
     *       {@code floor(1000/c) + 1} export ticks;</li>
     *   <li>the output: the final crafter's {@code a + 2C}, C = {@code floor(2000/c) + 1} crafting ticks;</li>
     *   <li>the next insertion: {@code chuteParkedSteps - 1} steps after the output, plus gap(chute, final).</li>
     * </ul>
     * Corpus 7 (3 x 3 at 64 RPM, the chute placed first, every target after its sources): 3 + 4 x 16 + 32 =
     * 99. Empty cells (export countDown 0) are not modelled, as in {@link #crafter(int, float)}.
     *
     * @param preds preds[k]: the crafters whose export goes into crafter k ({@code getPrecedingCrafters})
     * @param fin the final crafter (no target)
     * @param order order[k]: crafter k's place in the level's ticker list, -1 unknown
     * @param chuteOrder the chute's place, -1 unknown
     * @return ticks per craft (den 2 when an order is unknown), or null when the arrows form a cycle
     */
    public static Ratio crafterChuteLoop(int[][] preds, int fin, int[] order, int chuteOrder, float motion, float rpm) {
        int c = clamp((int) Math.abs(rpm), 4, 250);
        int e = 1000 / c + 1, cr = 2000 / c + 1;
        int n = preds.length;
        long[] a = new long[n];
        int[] state = new int[n]; // 0 new, 1 on the stack, 2 done
        java.util.ArrayDeque<Integer> stack = new java.util.ArrayDeque<>();
        stack.push(fin);
        while (!stack.isEmpty()) {
            int k = stack.peek();
            if (state[k] == 2) {
                stack.pop();
                continue;
            }
            state[k] = 1;
            boolean ready = true;
            for (int p : preds[k]) {
                if (state[p] == 1)
                    return null; // the arrows form a cycle
                if (state[p] == 0) {
                    stack.push(p);
                    ready = false;
                }
            }
            if (!ready)
                continue;
            long t = preds[k].length == 0 ? gap(order[k], chuteOrder) : Long.MIN_VALUE;
            for (int p : preds[k])
                t = Math.max(t, a[p] + 2L * e + gap(order[k], order[p]));
            a[k] = t;
            state[k] = 2;
            stack.pop();
        }
        long half = a[fin] + 2L * cr + 2L * (chuteParkedSteps(motion) - 1) + gap(chuteOrder, order[fin]);
        return half % 2 == 0 ? new Ratio(half / 2, 1) : new Ratio(half, 2);
    }

    /** Half ticks a receiver at {@code to} waits for a giver at {@code from}'s act: 2 next tick, 0 same tick, 1 unknown. */
    private static long gap(int to, int from) {
        if (to < 0 || from < 0)
            return 1;
        return to < from ? 2 : 0;
    }

    /**
     * Mean ticks between shearings at a deployer over a pen (corpus 13's renewal model, SheepWoolFactory):
     * each of the {@code sheep} eligible adults alternates sheared, {@code regrow} ticks until it eats, and
     * woolly, until the deployer picks it. Each activation picks one of the {@code entities} in its target
     * block at random ({@code DeployerHandler}: {@code entities.get(random.nextInt(size))}), so a woolly
     * sheep waits {@code (M - 1/2) D} for its pick, D the deployer's mean cycle: m = {@link #deployer} when
     * the pick finds no wool (the shears do nothing), h when it shears ({@link #shearingDeployer} plus a
     * third of {@link #DEPLOYER_OVERFLOW_WAIT}: with a hopper under the deployer, 8 ticks an item, the third
     * wool of {@code 1 + nextInt(3)} is still in the overflow at the first check; UNVERIFIED for other
     * drains). With R shearings per tick, R D of the cycles shear: D = m + R D (h - m) and
     * R = N / (T + (M - 1/2) D). Eliminating D, {@code T k R^2 - (T + (M - 1/2) m + N k) R + N = 0} with
     * k = h - m; its smaller root (R at most 1/h) is {@code 2N / (B + sqrt(B^2 - 4 T k N))}. Corpus 13
     * (16 sheep, 256 RPM, T 2,033.5): 135.8 ticks, 17.68 wool a minute.
     *
     * @return ticks per shearing (1/R), or 0 when the deployer does not run or there is no sheep
     */
    public static double shearingPeriod(float rpm, int sheep, int entities, double regrow) {
        int m = deployer(rpm);
        if (m <= 0 || sheep <= 0)
            return 0;
        double h = shearingDeployer(rpm) + DEPLOYER_OVERFLOW_WAIT / 3.0, k = h - m;
        double n = sheep, wait = Math.max(entities, sheep) - 0.5;
        double b = regrow + wait * m + n * k;
        double disc = Math.max(0, b * b - 4 * regrow * k * n);
        return (b + Math.sqrt(disc)) / (2 * n);
    }

    /**
     * Smart chute extraction amount ({@code SmartChuteBlockEntity.getExtractionAmount}): the filter count when
     * the count is shown (extracting from a non-chute inventory) and not "any amount", else 64.
     */
    public static int smartChuteExtraction(boolean countVisible, boolean anyAmount, int filterCount) {
        return countVisible && !anyAmount ? filterCount : 64;
    }

    // ---------------------------------------------------------------- Pumps

    /**
     * mB per tick a fluid network moves at its source: {@code FluidNetwork.tick}'s
     * {@code transferSpeed = (int) max(1, pressure / 2)}, with the pump's contribution
     * {@code |speed| / parallelBranches} ({@code PumpBlockEntity}, {@code parallelBranches = max(1, faces at that
     * distance - 1)}). Several pumps on one face add their pressures before the halving. The transfer is
     * split evenly among outputs, the remainder to the first ones.
     */
    public static int pumpTransfer(float rpm, int parallelBranches) {
        float pressure = Math.abs(rpm) / (float) Math.max(1, parallelBranches);
        return (int) Math.max(1.0F, pressure / 2.0F);
    }

    // ---------------------------------------------------------------- Spout, drain

    /**
     * Spout over a depot holding a stack, steady period per filled item: co-simulates
     * {@code SpoutBlockEntity.whenItemHeld} (from the depot: start {@code processingTicks = 20} when it reads
     * -1, fill when it reads 5) with the spout's tick ({@code processingTicks >= 0}: decrement). The fill is
     * 15 ticks after the start; the depot reads -1 again 21 ticks after it, in both orders. Not kinetic.
     */
    public static int spout() {
        return (int) cached(new Key(K_SPOUT, 0, 0, 0, 0), () -> Math.max(spoutDepotSim(true), spoutDepotSim(false)));
    }

    private static int spoutDepotSim(boolean depotFirst) {
        int processingTicks = -1;
        int lastStart = -1;
        int period = -1;
        int starts = 0;
        for (int tick = 0; starts < 4; tick++) {
            guard(tick);
            for (int part = 0; part < 2; part++) {
                boolean depot = (part == 0) == depotFirst;
                if (depot) {
                    if (processingTicks == -1) {
                        processingTicks = 20;
                        if (lastStart >= 0) {
                            period = tick - lastStart;
                        }
                        lastStart = tick;
                        starts++;
                    }
                } else if (processingTicks >= 0) {
                    processingTicks--;
                }
            }
        }
        return period;
    }

    /**
     * Spout over a belt that is never short of items: the filled outputs move off from the belt tick after
     * the fill, the next item reaches the centre k moves later ({@link #beltFollowMoves}), is held, and the
     * spout starts on the next belt tick that reads -1. Period {@code max(21, 16 + k)}. UNVERIFIED in game
     * (same belt caveats as {@link #pressBelt}).
     */
    public static int spoutBelt(float beltRpm) {
        return Math.max(spout(), 16 + beltFollowMoves(beltRpm));
    }

    /**
     * Item drain ticks from a side insertion to the tick the item leaves: re-enacts
     * {@code ItemDrainBlockEntity.tick} and {@code continueProcessing}. The item enters at 0.0 and moves
     * 0.125 per tick; the tick it crosses 0.5 sets {@code processingTicks = 20}; 20 ticks count down (the one
     * reaching 5 empties it, tick 19 overall); then it moves on and leaves on the tick it passes 1.0. With a
     * feeder always ready this is the steady period, in either order (the drain accepts only when empty).
     * UNVERIFIED: an item inserted vertically enters at 0.5 with {@code prevBeltPosition} 0.5 and never
     * satisfies the crossing test in this code, so it would pass undrained.
     */
    public static int drain() {
        return (int) cached(new Key(K_DRAIN, 0, 0, 0, 0), () -> {
            float position = 0.0F;
            int processingTicks = 0;
            int ticks = 0;
            while (true) {
                guard(++ticks);
                if (processingTicks > 0) {
                    processingTicks--;
                } else {
                    float prev = position;
                    position = position + 0.125F;
                    if (position > 1.0F) {
                        return ticks;
                    }
                    if (prev < 0.5F && position >= 0.5F) {
                        position = 0.5F;
                        processingTicks = 20;
                    }
                }
            }
        });
    }

    // ---------------------------------------------------------------- Create 6 logistics

    /**
     * Packager, steady period per queued package when the held box is taken as soon as allowed: re-enacts
     * {@code PackagerBlockEntity.tick} (at {@code animationTicks == 0} with an empty hand it takes the next
     * queued package and sets 20, no decrement that tick; otherwise it decrements) and
     * {@code PackagerItemHandler.extractItem} (only at {@code animationTicks == 0}). Period 21 ({@code CYCLE}
     * 20 plus the tick that starts the next), in either order with an extractor that is ready.
     *
     * <p>Not calibrated by the calibration GameTests. The corpus measured 21 per package on requests (up to
     * 9 x 64 items each), 22 in redstone mode, and 22 per package for a receiving packager's unpacking;
     * the last two are UNVERIFIED here.
     */
    public static int packager() {
        return (int) cached(new Key(K_PACKAGER, 0, 0, 0, 0), () -> {
            int animationTicks = 0;
            int ticks = 0;
            int startTick = -1;
            while (true) {
                guard(++ticks);
                if (animationTicks == 0) {
                    if (startTick >= 0) {
                        return ticks - startTick;
                    }
                    startTick = ticks;
                    animationTicks = 20;
                } else {
                    animationTicks--;
                }
            }
        });
    }

    /**
     * Frogport animation ticks per package: {@code FrogportBlockEntity.startAnimation} chases
     * {@code animationProgress} from 0 to 1 at 0.1 per tick ({@code Chaser.LINEAR}); a deposit exports at
     * {@code > 0.5}, and the animation ends at {@code >= 1}. Returns 10. UNVERIFIED: catnip's
     * {@code LerpedFloat}/{@code Chaser} source is not in the decompile, so the float stepping and the
     * end tick are assumed. While idle, the port pushes and pulls only on its lazy tick (rate 10: every
     * 11 ticks), except that a packager below wakes it directly when its own animation ends.
     */
    public static int frogport() {
        return 10;
    }

    /**
     * Chain conveyor package speed between conveyors in blocks per tick:
     * {@code ChainConveyorBlockEntity.tick}, {@code distancePerTick = |speed / 360|}.
     */
    public static float chainConveyorSpeed(float rpm) {
        return Math.abs(rpm / 360.0F);
    }

    /**
     * Chain conveyor loop speed in degrees per tick around one conveyor: {@code speed / (PI * 1.5) * 360}
     * in float ({@code radius 1.5}).
     */
    public static float chainConveyorDegreesPerTick(float rpm) {
        float speed = Math.abs(rpm) / 360.0F;
        return speed / ((float) Math.PI * 1.5F) * 360.0F;
    }

    // ---------------------------------------------------------------- Boilers and steam engines

    /**
     * Boiler level: {@code BoilerData.getActualHeat},
     * {@code min(activeHeat, min(min(18, ceil(waterSupply) / 10), min(18, tankBlocks / 4)))}.
     *
     * @param activeHeat summed heater levels under the tank ({@code updateTemperature}; a burner gives 1 if
     *     at least FADING, 2 if SEETHING)
     * @param tankBlocks the tank's total size in blocks (the {@code boilerSize} argument)
     * @param waterSupply mB per tick, the maximum of the last 10 five-tick samples
     */
    public static int boilerLevel(int activeHeat, int tankBlocks, float waterSupply) {
        int forBoilerSize = Math.min(18, tankBlocks / 4);
        int forWaterSupply = Math.min(18, ceil(waterSupply) / 10);
        return Math.min(activeHeat, Math.min(forWaterSupply, forBoilerSize));
    }

    /**
     * Engine efficiency: {@code BoilerData.getEngineEfficiency}. Passive: {@code 0.125 / engines}. Else 0 at
     * level 0, 1 while {@code engines <= level}, else {@code level / engines} in float.
     *
     * @param level {@link #boilerLevel}
     * @param passive Create's {@code isPassive()}: passive heat, no active heat, and both the size and the
     *     water limits above 0
     */
    public static float boilerEfficiency(int level, int engines, boolean passive) {
        if (engines <= 0) {
            return 0.0F;
        }
        if (passive) {
            return 0.125F / (float) engines;
        }
        if (level == 0) {
            return 0.0F;
        }
        return engines <= level ? 1.0F : (float) level / (float) engines;
    }

    /**
     * Steam engine speed modifier m: {@code PoweredShaftBlockEntity.getSpeedModifier},
     * {@code (int) (1 + (eff >= 1 ? 3 : min(2, floor(eff * 4))))}.
     */
    public static int engineSpeedModifier(float efficiency) {
        return (int) (1.0 + (efficiency >= 1.0F ? 3.0 : Math.min(2.0, Math.floor((double) (efficiency * 4.0F)))));
    }

    /**
     * Steam engine generated speed magnitude: {@code PoweredShaftBlockEntity.getGeneratedSpeed},
     * {@code 16 * m}, or 0 when the combined capacity is 0 (efficiency 0).
     */
    public static float engineSpeed(float efficiency) {
        return efficiency > 0.0F ? (float) (16 * engineSpeedModifier(efficiency)) : 0.0F;
    }

    /**
     * Steam engine capacity in SU per RPM: {@code PoweredShaftBlockEntity.calculateAddedStressCapacity},
     * {@code (float) (eff * capacity) / m} with {@code capacity = BlockStressValues.getCapacity} of the engine
     * block (1024 by default).
     */
    public static float engineCapacityPerRpm(float efficiency, double steamEngineCapacity) {
        float combined = (float) ((double) efficiency * steamEngineCapacity);
        return combined / (float) engineSpeedModifier(efficiency);
    }
}
