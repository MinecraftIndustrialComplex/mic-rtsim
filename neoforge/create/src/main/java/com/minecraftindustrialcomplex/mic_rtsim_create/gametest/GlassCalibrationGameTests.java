package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassCalibrationKit.Meter;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.SteamHelpers;
import com.minecraftindustrialcomplex.mic_rtsim_create.glass.Cycles;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllItems;
import com.simibubi.create.content.fluids.pipes.FluidPipeBlock;
import com.simibubi.create.content.fluids.tank.CreativeFluidTankBlockEntity;
import com.simibubi.create.content.kinetics.base.DirectionalAxisKineticBlock;
import com.simibubi.create.content.kinetics.base.DirectionalKineticBlock;
import com.simibubi.create.content.kinetics.base.HorizontalKineticBlock;
import com.simibubi.create.content.kinetics.belt.BeltBlockEntity;
import com.simibubi.create.content.kinetics.belt.transport.BeltInventory;
import com.simibubi.create.content.kinetics.belt.transport.TransportedItemStack;
import com.simibubi.create.content.kinetics.crafter.MechanicalCrafterBlock;
import com.simibubi.create.content.kinetics.crusher.CrushingWheelControllerBlockEntity;
import com.simibubi.create.content.kinetics.deployer.DeployerBlockEntity;
import com.simibubi.create.content.kinetics.deployer.DeployerFakePlayer;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.kinetics.millstone.MillstoneBlockEntity;
import com.simibubi.create.content.kinetics.saw.SawBlock;
import com.simibubi.create.content.kinetics.saw.SawBlockEntity;
import com.simibubi.create.content.logistics.depot.DepotBehaviour;
import com.simibubi.create.content.logistics.funnel.FunnelBlock;
import com.simibubi.create.content.processing.basin.BasinBlockEntity;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import com.simibubi.create.foundation.item.SmartInventory;
import com.simibubi.create.infrastructure.config.AllConfigs;
import net.createmod.catnip.math.Pointing;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.stream.Collectors;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassCalibrationKit.*;

/**
 * Glass box calibration: each {@link Cycles} evaluator beside the real Create 6.0.10 block entity. Every
 * test builds one machine on a creative motor at a fixed speed, keeps it fed (topping inputs up and
 * emptying outputs from the test's own tick, which runs after the level's), watches the block entity or
 * its store every tick and logs the mean ticks between consecutive completions after the first, on one
 * line per machine and speed:
 *
 * <pre>[gametest] calib &lt;machine&gt; rpm=&lt;r&gt; key=&lt;duration/batch&gt; model=&lt;ticks&gt; measured=&lt;mean ticks&gt; ops=&lt;n&gt; diff=&lt;measured-model&gt;</pre>
 *
 * <p>The pump logs mB per tick instead of ticks (key {@code mB/t}); funnels and the chute have no speed
 * (rpm 0). A test passes once it has measured its operations, whatever the difference: these measure the
 * model, they do not gate on it. It fails only when the machine never completed an operation (a model of
 * {@link Cycles#NEVER} passes either way and says what it saw).
 *
 * <p>Feeding from the test tick is a hand-off of its own: the test refills or clears right after the
 * level ticked, so a machine that empties its input in tick t finds the next one in tick t + 1, which is
 * what an always-ready feeder that ticks before it would give. Differences of a tick per cycle from the
 * placement order of neighbours (who ticks first) are hand-off effects, not model errors.
 *
 * <p>The hand-off cases use the corpus's real feeders instead, so {@link Cycles#chuteHandOff} is measured,
 * not fitted: {@code saw_chute} (corpus 5: a smart chute pulling one alloy over the saw),
 * {@code crusher_chute} (corpus 1: a smart chute pulling 2 raw iron over the crushing wheels) and
 * {@code fan_depot_arm} (corpus 8: a chute over a blasting depot, emptied by a mechanical arm into a filtered
 * funnel, in two placement orders, since the three-actor loop depends on the ticker order). Their lines add
 * {@code periods=<ticks>x<count>}, the periods seen after the warm-up.
 *
 * <p>Each machine is its own batch (its speeds run side by side). The environment variable
 * {@code MIC_GLASS_CALIB} (comma-separated machine names, or {@code none}) limits the run; unset, all run.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class GlassCalibrationGameTests {
    private GlassCalibrationGameTests() {}

    static final int[] SPEEDS = {16, 32, 64, 128, 256};
    /** Defaults for the timeout estimates only (the tests read the live config). */
    static final int FAN_TIME = 150;
    static final float MEDIUM_SPEED = 30.0F;
    static final int EXTRACTION_TIMER = 8;
    /** A chute's item motion under gravity alone. */
    static final float GRAVITY = Cycles.chuteMotion(0.0F, 0.0F);

    record Run(Meter meter, Runnable step) {}

    interface Body {
        Run build(GameTestHelper h, int target);
    }

    @GameTestGenerator
    public static Collection<TestFunction> glassCalibration() {
        String only = System.getenv("MIC_GLASS_CALIB");
        Set<String> filter = only == null || only.isBlank() ? Set.of()
                : Arrays.stream(only.split(",")).map(String::trim).filter(s -> !s.isEmpty()).collect(Collectors.toSet());
        List<TestFunction> out = new ArrayList<>();
        for (int rpm : SPEEDS) {
            String v = "rpm" + rpm;
            add(out, filter, "millstone", v, Cycles.millstone(150, rpm), (h, t) -> millstone(h, t, rpm));
            add(out, filter, "crusher", "n1_" + v, Cycles.crusher(400, 1, rpm), (h, t) -> crusher(h, t, rpm, 1));
            add(out, filter, "crusher", "n16_" + v, Cycles.crusher(400, 16, rpm), (h, t) -> crusher(h, t, rpm, 16));
            add(out, filter, "press_depot", v, Cycles.pressDepot(rpm), (h, t) -> pressDepot(h, t, rpm));
            add(out, filter, "press_belt", v, Cycles.pressBelt(rpm, rpm), (h, t) -> pressBelt(h, t, rpm, 1));
            add(out, filter, "mixer", v, Cycles.mixerSteady(0, rpm, MEDIUM_SPEED), (h, t) -> mixer(h, t, rpm));
            add(out, filter, "saw", v, Cycles.saw(200, 1, rpm), (h, t) -> saw(h, t, rpm));
            add(out, filter, "deployer_depot", v, Cycles.deployerDepot(rpm), (h, t) -> deployerDepot(h, t, rpm));
            add(out, filter, "crafter", "d1_" + v, Cycles.crafter(1, rpm, 1), (h, t) -> crafter(h, t, rpm, 1));
            add(out, filter, "crafter", "d2_" + v, Cycles.crafter(2, rpm, 2), (h, t) -> crafter(h, t, rpm, 2));
            add(out, filter, "belt", v, Cycles.beltInsertMoves(rpm), (h, t) -> belt(h, t, rpm));
            Cycles.Ratio arm = Cycles.armCycle(rpm);
            add(out, filter, "arm", v, arm.num() / (double) arm.den(), (h, t) -> arm(h, t, rpm));
            add(out, filter, "pump", v, Cycles.NEVER, (h, t) -> pump(h, t, rpm));
        }
        for (int rpm : new int[] {64, 256})
            add(out, filter, "press_belt", "x4_rpm" + rpm, Cycles.pressBelt(rpm, rpm), (h, t) -> pressBelt(h, t, rpm, 4));
        for (int rpm : new int[] {16, 64, 128})
            add(out, filter, "fan_depot", "n1_rpm" + rpm, Cycles.fanTicks(1, FAN_TIME), (h, t) -> fan(h, t, rpm, 1));
        for (int n : new int[] {16, 17, 64})
            add(out, filter, "fan_depot", "n" + n + "_rpm64", Cycles.fanTicks(n, FAN_TIME), (h, t) -> fan(h, t, 64, n));
        add(out, filter, "funnel_andesite", "rpm0", Cycles.funnelPeriod(EXTRACTION_TIMER), (h, t) -> funnel(h, t, false));
        add(out, filter, "funnel_brass", "rpm0", Cycles.funnelPeriod(EXTRACTION_TIMER), (h, t) -> funnel(h, t, true));
        add(out, filter, "chute", "rpm0", Cycles.chuteGrabPeriod(Cycles.chuteMotion(0.0F, 0.0F)), GlassCalibrationGameTests::chute);
        // Hand-off loops with the corpus's real feeders: the machine takes its next input only once empty,
        // from a parked chute above it (Cycles.chuteHandOff).
        double self = Cycles.chuteHandOff(GRAVITY, 0, true).ticks();
        for (int rpm : new int[] {16, 32, 64, 96, 128, 256})
            add(out, filter, "saw_chute", "rpm" + rpm, Cycles.saw(200, 1, rpm) + self, (h, t) -> sawChute(h, t, rpm));
        for (int rpm : new int[] {16, 32, 50, 64, 128, 256})
            add(out, filter, "crusher_chute", "n2_rpm" + rpm, Cycles.crusher(400, 2, rpm) + self, (h, t) -> crusherChute(h, t, rpm));
        for (int rpm : new int[] {64, 256})
            for (String order : new String[] {"cfa", "afc"})
                add(out, filter, "fan_depot_arm", order + "_rpm" + rpm,
                        Cycles.fanTicks(16, FAN_TIME) + Cycles.chuteHandOff(GRAVITY, Cycles.armMoveTicks(rpm), false).ticks(),
                        (h, t) -> fanDepotArm(h, t, rpm, order));
        return out;
    }

    /**
     * One test: batch {@code glass_calib_<machine>}, timeout from the model's estimate ({@code estimate} ticks
     * per operation; {@link Cycles#NEVER} or a rate gets a fixed 600-tick window).
     */
    static void add(List<TestFunction> out, Set<String> filter, String machine, String variant, double estimate, Body body) {
        if (filter.contains("none") || (!filter.isEmpty() && !filter.contains(machine)))
            return;
        int target = targetOps(estimate);
        int deadline = deadline(estimate, target);
        String name = ("glasscalib." + machine + "_" + variant).toLowerCase(Locale.ROOT);
        out.add(new TestFunction("glass_calib_" + machine, name, MicRtsimCreate.MODID + ":" + IndustryKit.YARD, Rotation.NONE,
                deadline + 100, 0L, true, false, 1, 1, true, h -> {
                    Run run = body.build(h, target);
                    drive(h, run.meter(), deadline, run.step());
                }));
    }

    // ---- machines -------------------------------------------------------------------------------

    /**
     * Millstone milling wheat ({@code milling/wheat}, 150 ticks), driven by a cogwheel beside it. The test
     * keeps 32 to 64 wheat in the input slot and empties the outputs every tick; an operation is one wheat
     * gone from the input ({@code MillstoneBlockEntity.process}).
     */
    static Run millstone(GameTestHelper h, int target, int rpm) {
        Meter m = new Meter("millstone", Integer.toString(rpm), "D150", Cycles.millstone(150, rpm), target);
        BlockPos mill = at(h, 8, 2, 8);
        set(h, mill, AllBlocks.MILLSTONE.getDefaultState());
        cog(h, at(h, 9, 2, 8), Direction.Axis.Y);
        motor(h, at(h, 9, 1, 8), Direction.UP, rpm);
        int[] last = {-1};
        return new Run(m, () -> {
            MillstoneBlockEntity be = beOrNull(h, mill, MillstoneBlockEntity.class);
            if (be == null)
                return;
            for (int i = 0; i < be.outputInv.getSlots(); i++)
                be.outputInv.setStackInSlot(i, ItemStack.EMPTY);
            int c = be.inputInv.getStackInSlot(0).getCount();
            if (last[0] >= 0 && c < last[0])
                m.mark(h.getTick(), last[0] - c);
            if (c < 32) {
                be.inputInv.setStackInSlot(0, new ItemStack(Items.WHEAT, 64));
                c = 64;
            }
            last[0] = c;
        });
    }

    /**
     * Crushing wheels (Z axis, one block apart, turning opposite ways so the controller between them faces
     * down) crushing raw iron ({@code crushing/raw_iron}, 400 ticks). The test puts a stack of {@code n}
     * straight into the controller's inventory whenever it is empty (it accepts only then), and kills the
     * item entities the controller drops; an operation is the inventory emptying after a load.
     */
    static Run crusher(GameTestHelper h, int target, int rpm, int n) {
        Meter m = new Meter("crusher", Integer.toString(rpm), "D400 n" + n, Cycles.crusher(400, n, rpm), target);
        BlockPos a = at(h, 7, 2, 8), b = at(h, 9, 2, 8), ctrl = at(h, 8, 2, 8);
        BlockState wheel = AllBlocks.CRUSHING_WHEEL.getDefaultState().setValue(BlockStateProperties.AXIS, Direction.Axis.Z);
        set(h, a, wheel);
        set(h, b, wheel);
        // The controller faces cross(Z, side) x sign(speed) x side step: west wheel negative, east positive gives DOWN.
        motor(h, at(h, 7, 2, 7), Direction.SOUTH, -rpm);
        motor(h, at(h, 9, 2, 7), Direction.SOUTH, rpm);
        boolean[] loaded = {false};
        boolean[] noted = {false};
        return new Run(m, () -> {
            killItems(h);
            CrushingWheelControllerBlockEntity c = beOrNull(h, ctrl, CrushingWheelControllerBlockEntity.class);
            if (c == null || c.crushingspeed == 0)
                return;
            if (!noted[0]) {
                noted[0] = true;
                m.note("facing=" + h.getLevel().getBlockState(ctrl).getValue(BlockStateProperties.FACING).getName()
                        + " crushingspeed=" + fmt(c.crushingspeed));
            }
            if (c.inventory.isEmpty()) {
                if (loaded[0])
                    m.mark(h.getTick());
                loaded[0] = c.inventory.insertItem(0, new ItemStack(Items.RAW_IRON, n), false).isEmpty();
            }
        });
    }

    /**
     * Keeps a depot's held stack of {@code item} between 32 and 64 (growing it in place, so a press or
     * deployer working on it is not disturbed) and empties its output buffer every tick; marks every item
     * the held stack loses (one per press or deployer application, {@code BeltPressingCallbacks},
     * {@code BeltDeployerCallbacks.activate}: one at a time, the rest stays held).
     */
    static Runnable depotFeeder(GameTestHelper h, Meter m, BlockPos depot, Item item) {
        int[] last = {-1};
        return () -> {
            clearDepotOutputs(h, depot);
            DepotBehaviour d = depotBehaviour(h, depot);
            ItemStack held = d.getHeldItemStack();
            int c = held.is(item) ? held.getCount() : 0;
            if (last[0] >= 0 && c < last[0])
                m.mark(h.getTick(), last[0] - c);
            if (c == 0) {
                placeOnDepot(h, depot, new ItemStack(item, 64));
                c = 64;
            } else if (c < 32) {
                held.grow(64 - c);
                c = 64;
            }
            last[0] = c;
        };
    }

    /** Mechanical press over a depot pressing iron ingots ({@code pressing/iron_ingot}). */
    static Run pressDepot(GameTestHelper h, int target, int rpm) {
        Meter m = new Meter("press_depot", Integer.toString(rpm), "iron_ingot", Cycles.pressDepot(rpm), target);
        BlockPos depot = at(h, 8, 1, 8), press = at(h, 8, 3, 8);
        depot(h, depot);
        set(h, press, AllBlocks.MECHANICAL_PRESS.getDefaultState().setValue(HorizontalKineticBlock.HORIZONTAL_FACING, Direction.NORTH));
        motor(h, at(h, 8, 3, 9), Direction.NORTH, rpm);
        return new Run(m, depotFeeder(h, m, depot, Items.IRON_INGOT));
    }

    /**
     * Tracks a belt's items by identity: an item of {@code product} gone since the last look left the belt
     * (dropped off its end into the world, where {@link GlassCalibrationKit#killItems} removes it). Only the
     * product counts: processing on the belt also replaces items ({@code TransportedResult.convertTo}), so
     * the input's entry vanishes at the press as well.
     */
    static final class BeltWatch {
        final BlockPos start;
        final Item product;
        /** Items seen on the last look, with their belt positions then. */
        Map<TransportedItemStack, Float> seen = new IdentityHashMap<>();
        /** Items other than the product that left over the end (last seen in the final segment). */
        int otherExits;

        BeltWatch(BlockPos start, Item product) {
            this.start = start;
            this.product = product;
        }

        /** Products that left since the last call, or -1 while the belt is not built or not turning. */
        int exits(GameTestHelper h) {
            BeltBlockEntity b = beOrNull(h, start, BeltBlockEntity.class);
            if (b == null || b.getSpeed() == 0)
                return -1;
            BeltInventory inv = b.getInventory();
            if (inv == null)
                return -1;
            Map<TransportedItemStack, Float> now = new IdentityHashMap<>();
            for (TransportedItemStack t : inv.getTransportedItems())
                now.put(t, t.beltPosition);
            int gone = 0;
            for (Map.Entry<TransportedItemStack, Float> e : seen.entrySet()) {
                if (now.containsKey(e.getKey()))
                    continue;
                if (e.getKey().stack.is(product))
                    gone += e.getKey().stack.getCount();
                else if (e.getValue() >= b.beltLength - 1)
                    otherExits += e.getKey().stack.getCount();
            }
            seen = now;
            return gone;
        }

        /** Offers {@code count} of {@code item} at the start segment through its capability (single-point insertion). */
        void offer(GameTestHelper h, Item item, int count) {
            IItemHandler inv = h.getLevel().getCapability(Capabilities.ItemHandler.BLOCK, start, null);
            if (inv != null)
                inv.insertItem(0, new ItemStack(item, count), false);
        }
    }

    /**
     * Mechanical press over the middle of a 7-block belt at the press's speed, fed one ingot (or a stack of
     * {@code n}) at a time at the start segment whenever the belt takes it; an operation is a pressed sheet
     * leaving the belt's end. The press is the bottleneck at every speed, so the exit period is its belt
     * cycle. With stacks the note counts the ingots that leave unpressed: the press takes one item of a
     * held stack and the belt re-inserts the rest just before the centre ({@code BeltInventory.applyToEachWithin}).
     */
    static Run pressBelt(GameTestHelper h, int target, int rpm, int n) {
        Meter m = new Meter("press_belt", Integer.toString(rpm), "iron_ingot x" + n + " belt=" + rpm, Cycles.pressBelt(rpm, rpm), target);
        BlockPos start = at(h, 5, 2, 8), end = at(h, 11, 2, 8), press = at(h, 8, 4, 8);
        GlassCalibrationKit.belt(h, start, end);
        // An X belt moves +X at negative speed; a motor facing south gives its value.
        motor(h, at(h, 5, 2, 7), Direction.SOUTH, -rpm);
        set(h, press, AllBlocks.MECHANICAL_PRESS.getDefaultState().setValue(HorizontalKineticBlock.HORIZONTAL_FACING, Direction.NORTH));
        motor(h, at(h, 8, 4, 9), Direction.NORTH, rpm);
        BeltWatch w = new BeltWatch(start, AllItems.IRON_SHEET.get());
        return new Run(m, () -> {
            killItems(h);
            int gone = w.exits(h);
            if (gone < 0)
                return;
            if (gone > 0)
                m.mark(h.getTick(), gone);
            if (n > 1)
                m.note("unpressed_ingots_out=" + w.otherExits + " sheets_out=" + m.completions());
            w.offer(h, Items.IRON_INGOT, n);
        });
    }

    /**
     * Mechanical mixer over a basin mixing andesite alloy ({@code mixing/andesite_alloy}: andesite and an
     * iron nugget, no heat, no processing time). The test keeps 16 to 48 of each input in the basin and
     * empties its outputs every tick; an operation is one andesite gone.
     */
    static Run mixer(GameTestHelper h, int target, int rpm) {
        float medium = AllConfigs.server().kinetics.mediumSpeed.getF();
        Meter m = new Meter("mixer", Integer.toString(rpm), "D0 andesite_alloy medium=" + fmt(medium), Cycles.mixerSteady(0, rpm, medium), target);
        BlockPos basin = at(h, 8, 1, 8), mixer = at(h, 8, 3, 8);
        set(h, basin, AllBlocks.BASIN.getDefaultState());
        set(h, mixer, AllBlocks.MECHANICAL_MIXER.getDefaultState());
        cog(h, at(h, 9, 3, 8), Direction.Axis.Y);
        motor(h, at(h, 9, 2, 8), Direction.UP, rpm);
        int[] last = {-1};
        return new Run(m, () -> {
            BasinBlockEntity b = beOrNull(h, basin, BasinBlockEntity.class);
            if (b == null)
                return;
            SmartInventory outInv = b.getOutputInventory();
            for (int i = 0; i < outInv.getSlots(); i++)
                if (!outInv.getStackInSlot(i).isEmpty())
                    outInv.setStackInSlot(i, ItemStack.EMPTY);
            SmartInventory in = b.getInputInventory();
            int andesite = countIn(in, Items.ANDESITE);
            if (last[0] >= 0 && andesite < last[0])
                m.mark(h.getTick(), last[0] - andesite);
            if (andesite < 16)
                ItemHandlerHelper.insertItemStacked(in, new ItemStack(Items.ANDESITE, 32), false);
            if (countIn(in, Items.IRON_NUGGET) < 16)
                ItemHandlerHelper.insertItemStacked(in, new ItemStack(Items.IRON_NUGGET, 32), false);
            last[0] = countIn(in, Items.ANDESITE);
        });
    }

    static int countIn(IItemHandler inv, Item item) {
        int n = 0;
        for (int i = 0; i < inv.getSlots(); i++) {
            ItemStack s = inv.getStackInSlot(i);
            if (s.is(item))
                n += s.getCount();
        }
        return n;
    }

    /**
     * Mechanical saw facing up, filtered to shafts, cutting andesite alloy ({@code cutting/andesite_alloy},
     * 200 ticks, 6 shafts). The test puts one alloy straight into the saw's inventory whenever it is empty
     * and kills the shafts it throws out; an operation is the inventory emptying after a load.
     */
    static Run saw(GameTestHelper h, int target, int rpm) {
        Meter m = new Meter("saw", Integer.toString(rpm), "D200 n1", Cycles.saw(200, 1, rpm), target);
        BlockPos saw = at(h, 8, 1, 8);
        set(h, saw, AllBlocks.MECHANICAL_SAW.getDefaultState().setValue(SawBlock.FACING, Direction.UP)
                .setValue(DirectionalAxisKineticBlock.AXIS_ALONG_FIRST_COORDINATE, true));
        motor(h, at(h, 7, 1, 8), Direction.EAST, rpm);
        FilteringBehaviour f = BlockEntityBehaviour.get(h.getLevel(), saw, FilteringBehaviour.TYPE);
        if (f == null)
            throw new GameTestAssertException("no filter on the saw");
        f.setFilter(new ItemStack(AllBlocks.SHAFT.get()));
        boolean[] loaded = {false};
        return new Run(m, () -> {
            killItems(h);
            SawBlockEntity s = beOrNull(h, saw, SawBlockEntity.class);
            if (s == null || s.getSpeed() == 0)
                return;
            if (s.inventory.isEmpty()) {
                if (loaded[0])
                    m.mark(h.getTick());
                loaded[0] = s.inventory.insertItem(0, new ItemStack(AllItems.ANDESITE_ALLOY.get()), false).isEmpty();
            }
        });
    }

    /**
     * Deployer facing down over a depot, holding andesite alloy, casing stripped oak logs
     * ({@code item_application/andesite_casing_from_log}). The test keeps 32 to 64 alloy in the fake
     * player's hand and 32 to 64 logs on the depot; an operation is one log gone.
     */
    static Run deployerDepot(GameTestHelper h, int target, int rpm) {
        Meter m = new Meter("deployer_depot", Integer.toString(rpm), "andesite_casing", Cycles.deployerDepot(rpm), target);
        BlockPos depot = at(h, 8, 1, 8), dep = at(h, 8, 3, 8);
        depot(h, depot);
        set(h, dep, AllBlocks.DEPLOYER.getDefaultState().setValue(DirectionalKineticBlock.FACING, Direction.DOWN)
                .setValue(DirectionalAxisKineticBlock.AXIS_ALONG_FIRST_COORDINATE, true));
        motor(h, at(h, 7, 3, 8), Direction.EAST, rpm);
        Runnable feed = depotFeeder(h, m, depot, Items.STRIPPED_OAK_LOG);
        Item alloy = AllItems.ANDESITE_ALLOY.get();
        return new Run(m, () -> {
            DeployerBlockEntity d = beOrNull(h, dep, DeployerBlockEntity.class);
            if (d == null)
                return;
            DeployerFakePlayer p = d.getPlayer();
            if (p == null)
                return;
            ItemStack hand = p.getMainHandItem();
            if (!hand.is(alloy))
                p.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(alloy, 64));
            else if (hand.getCount() < 32)
                hand.setCount(64);
            feed.run();
        });
    }

    /**
     * Encased fan blowing east through a water source onto a depot two blocks away (washing). The test puts
     * a fresh centred stack of {@code n} sand on the depot whenever it holds anything else (washed into clay,
     * or nothing: {@code splashing/sand} is a 25% chance); an operation is the sand being washed.
     */
    static Run fan(GameTestHelper h, int target, int rpm, int n) {
        int time = AllConfigs.server().kinetics.fanProcessingTime.get();
        Meter m = new Meter("fan_depot", Integer.toString(rpm), "T" + time + " n" + n, Cycles.fanTicks(n, time), target);
        BlockPos fan = at(h, 5, 1, 8), water = at(h, 6, 1, 8), depot = at(h, 7, 1, 8);
        motor(h, at(h, 4, 1, 8), Direction.EAST, rpm);
        set(h, fan, AllBlocks.ENCASED_FAN.getDefaultState().setValue(BlockStateProperties.FACING, Direction.EAST));
        set(h, at(h, 6, 1, 7), Blocks.GLASS.defaultBlockState());
        set(h, at(h, 6, 1, 9), Blocks.GLASS.defaultBlockState());
        depot(h, depot);
        set(h, water, Blocks.WATER.defaultBlockState());
        boolean[] loaded = {false};
        return new Run(m, () -> {
            DepotBehaviour d = depotBehaviour(h, depot);
            if (!d.getHeldItemStack().is(Items.SAND)) {
                if (loaded[0])
                    m.mark(h.getTick());
                d.removeHeldItem();
                clearDepotOutputs(h, depot);
                placeOnDepot(h, depot, new ItemStack(Items.SAND, n));
                loaded[0] = true;
            }
        });
    }

    /** A crafter facing {@code facing} whose arrow points at {@code target} (a world direction). */
    static BlockState crafterState(Direction facing, Direction target) {
        BlockState base = AllBlocks.MECHANICAL_CRAFTER.getDefaultState().setValue(HorizontalKineticBlock.HORIZONTAL_FACING, facing);
        for (Pointing p : Pointing.values()) {
            BlockState s = base.setValue(MechanicalCrafterBlock.POINTING, p);
            if (MechanicalCrafterBlock.getTargetDirection(s) == target)
                return s;
        }
        throw new GameTestAssertException("no pointing of a crafter facing " + facing + " targets " + target);
    }

    /**
     * Mechanical crafters facing north, inputs not connected, the final one pointing down into a chest:
     * d = 1 is two in a column (two oak planks, 4 sticks), d = 2 three in a row (three planks, 6 slabs).
     * Each crafter is placed after the ones that feed it, so it ticks after them and every hand-off lands in
     * the same game tick: the model is {@code crafter(d, rpm, d)}. The test puts a plank into every empty
     * crafter (they take one only while idle); an operation is the chest's count of the product rising.
     */
    static Run crafter(GameTestHelper h, int target, int rpm, int d) {
        Meter m = new Meter("crafter", Integer.toString(rpm), "d" + d + (d == 1 ? " sticks" : " slabs"), Cycles.crafter(d, rpm, d), target);
        List<BlockPos> crafters = new ArrayList<>();
        BlockPos out;
        Item product;
        if (d == 1) {
            crafters.add(at(h, 8, 3, 8));
            crafters.add(at(h, 8, 2, 8));
            for (BlockPos p : crafters)
                set(h, p, crafterState(Direction.NORTH, Direction.DOWN));
            out = at(h, 8, 1, 8);
            product = Items.STICK;
            cog(h, at(h, 9, 2, 8), Direction.Axis.Z);
            motor(h, at(h, 9, 2, 9), Direction.NORTH, rpm);
        } else {
            for (int x = 7; x <= 9; x++) {
                BlockPos p = at(h, x, 2, 8);
                crafters.add(p);
                set(h, p, crafterState(Direction.NORTH, x < 9 ? Direction.EAST : Direction.DOWN));
            }
            out = at(h, 9, 1, 8);
            product = Items.OAK_SLAB;
            cog(h, at(h, 10, 2, 8), Direction.Axis.Z);
            motor(h, at(h, 10, 2, 9), Direction.NORTH, rpm);
        }
        chest(h, out);
        int[] last = {0};
        return new Run(m, () -> {
            int c = count(h, out, product);
            if (c > last[0])
                m.mark(h.getTick());
            last[0] = c;
            for (BlockPos p : crafters) {
                IItemHandler inv = h.getLevel().getCapability(Capabilities.ItemHandler.BLOCK, p, null);
                if (inv != null && inv.getStackInSlot(0).isEmpty())
                    inv.insertItem(0, new ItemStack(Items.OAK_PLANKS), false);
            }
        });
    }

    /**
     * A 6-block belt fed one cobblestone at a time at its start segment, through the segment's capability,
     * whenever it takes one (single-point insertion: {@code BeltInventory.canInsertAt}); items drop off the
     * far end. An operation is an item leaving the belt; the model is the insertion spacing.
     */
    static Run belt(GameTestHelper h, int target, int rpm) {
        Meter m = new Meter("belt", Integer.toString(rpm), "single-point v=" + fmt(Cycles.beltSpeed(rpm)), Cycles.beltInsertMoves(rpm), target);
        BlockPos start = at(h, 4, 2, 8), end = at(h, 9, 2, 8);
        GlassCalibrationKit.belt(h, start, end);
        motor(h, at(h, 4, 2, 7), Direction.SOUTH, -rpm);
        BeltWatch w = new BeltWatch(start, Items.COBBLESTONE);
        return new Run(m, () -> {
            killItems(h);
            int gone = w.exits(h);
            if (gone < 0)
                return;
            if (gone > 0)
                m.mark(h.getTick(), gone);
            w.offer(h, Items.COBBLESTONE, 1);
        });
    }

    /**
     * An extracting funnel on a chest's east side, dropping into the world (the test kills the drops, so the
     * funnel never waits for its last item to clear). Andesite takes one item, brass (no filter set) up to
     * its filter count. An operation is the chest losing items.
     */
    static Run funnel(GameTestHelper h, int target, boolean brass) {
        int timer = AllConfigs.server().logistics.defaultExtractionTimer.get();
        Meter m = new Meter(brass ? "funnel_brass" : "funnel_andesite", "0", "T" + timer, Cycles.funnelPeriod(timer), target);
        BlockPos chest = at(h, 8, 1, 8), funnel = at(h, 9, 1, 8);
        chest(h, chest);
        int full = 27 * 64;
        fill(h, chest, Items.COBBLESTONE, full);
        BlockState state = brass ? AllBlocks.BRASS_FUNNEL.getDefaultState() : AllBlocks.ANDESITE_FUNNEL.getDefaultState();
        set(h, funnel, state.setValue(BlockStateProperties.FACING, Direction.EAST).setValue(FunnelBlock.EXTRACTING, true));
        int[] last = {-1};
        long[] moved = {0};
        return new Run(m, () -> {
            killItems(h);
            int c = count(h, chest, Items.COBBLESTONE);
            if (last[0] >= 0 && c < last[0]) {
                m.mark(h.getTick());
                moved[0] += last[0] - c;
                m.note("items/op=" + fmt(moved[0] / (double) m.completions()));
            }
            if (c < full / 2) {
                fill(h, chest, Items.COBBLESTONE, full - c);
                c = count(h, chest, Items.COBBLESTONE);
            }
            last[0] = c;
        });
    }

    /**
     * A mechanical arm between two depots, turned by a cogwheel beside it: it takes from the west depot
     * and deposits on the east one. The test keeps a stack of 16 on the west depot and clears the east one;
     * an operation is a deposit. The model is {@code armCycle} (the slowest lazy-tick orbit's mean); the
     * key carries {@code armCycleMax}, the bound without lazy ticks.
     */
    static Run arm(GameTestHelper h, int target, int rpm) {
        Cycles.Ratio r = Cycles.armCycle(rpm);
        Meter m = new Meter("arm", Integer.toString(rpm), "depot-depot max=" + Cycles.armCycleMax(rpm) + " exact=" + r.num() + "/" + r.den(),
                r.num() / (double) r.den(), target);
        BlockPos armPos = at(h, 8, 1, 8), in = at(h, 6, 1, 8), out = at(h, 10, 1, 8);
        depot(h, in);
        depot(h, out);
        cog(h, at(h, 8, 1, 9), Direction.Axis.Y);
        motor(h, at(h, 8, 2, 9), Direction.DOWN, rpm);
        GlassCalibrationKit.arm(h, armPos, List.of(in), List.of(out), ArmBlockEntity.SelectionMode.ROUND_ROBIN);
        return new Run(m, () -> {
            DepotBehaviour src = depotBehaviour(h, in), dst = depotBehaviour(h, out);
            boolean deposited = !dst.getHeldItemStack().isEmpty();
            if (clearDepotOutputs(h, out) > 0)
                deposited = true;
            if (deposited) {
                m.mark(h.getTick());
                dst.removeHeldItem();
            }
            if (src.getHeldItemStack().isEmpty())
                placeOnDepot(h, in, new ItemStack(Items.COBBLESTONE, 16));
        });
    }

    /**
     * A chute between two chests, gravity only: it pulls 16 at a time ({@code ChuteBlockEntity}, extraction
     * amount 16) from the chest above. The test keeps the upper chest at least half full and empties the
     * lower one; an operation is the upper chest losing items.
     */
    static Run chute(GameTestHelper h, int target) {
        Meter m = new Meter("chute", "0", "gravity x" + Cycles.CHUTE_EXTRACTION, Cycles.chuteGrabPeriod(Cycles.chuteMotion(0.0F, 0.0F)), target);
        BlockPos top = at(h, 8, 3, 8), chute = at(h, 8, 2, 8), bottom = at(h, 8, 1, 8);
        chest(h, top);
        fill(h, top, Items.COBBLESTONE, 1024);
        set(h, chute, AllBlocks.CHUTE.getDefaultState());
        chest(h, bottom);
        int[] last = {-1};
        long[] moved = {0};
        return new Run(m, () -> {
            IItemHandler low = items(h, bottom);
            for (int i = 0; i < low.getSlots(); i++)
                low.extractItem(i, 64, false);
            int c = count(h, top, Items.COBBLESTONE);
            if (last[0] >= 0 && c < last[0]) {
                m.mark(h.getTick());
                moved[0] += last[0] - c;
                m.note("items/op=" + fmt(moved[0] / (double) m.completions()));
            }
            if (c < 512) {
                fill(h, top, Items.COBBLESTONE, 1024 - c);
                c = count(h, top, Items.COBBLESTONE);
            }
            last[0] = c;
        });
    }

    /**
     * A mechanical pump facing east between a creative fluid tank of water and a fluid tank, a pipe on each
     * side, turned by a cogwheel above it (pumps mesh like small cogs on their facing axis). The test drains
     * the tank every tick; after the first water and 20 ticks of settling it averages mB per tick over 100
     * ticks. The model is {@code pumpTransfer(rpm, 1)} in mB per tick.
     */
    static Run pump(GameTestHelper h, int target, int rpm) {
        Meter m = new Meter("pump", Integer.toString(rpm), "mB/t", Cycles.pumpTransfer(rpm, 1), target);
        BlockPos src = at(h, 5, 1, 8), p1 = at(h, 6, 1, 8), pump = at(h, 7, 1, 8), p2 = at(h, 8, 1, 8), dst = at(h, 9, 1, 8);
        set(h, src, AllBlocks.CREATIVE_FLUID_TANK.getDefaultState());
        FluidPipeBlock pipe = (FluidPipeBlock) AllBlocks.FLUID_PIPE.get();
        set(h, p1, pipe.getAxisState(Direction.Axis.X));
        set(h, pump, AllBlocks.MECHANICAL_PUMP.getDefaultState().setValue(BlockStateProperties.FACING, Direction.EAST));
        set(h, p2, pipe.getAxisState(Direction.Axis.X));
        set(h, dst, AllBlocks.FLUID_TANK.getDefaultState());
        cog(h, at(h, 7, 2, 8), Direction.Axis.X);
        motor(h, at(h, 6, 2, 8), Direction.EAST, rpm);
        long[] first = {-1};
        long[] sum = {0};
        int[] ticks = {0};
        int window = 100;
        return new Run(m, () -> {
            CreativeFluidTankBlockEntity c = beOrNull(h, src, CreativeFluidTankBlockEntity.class);
            if (c != null && c.getTankInventory().getFluid().isEmpty()
                    && c.getTankInventory() instanceof CreativeFluidTankBlockEntity.CreativeSmartFluidTank creative)
                creative.setContainedFluid(new FluidStack(Fluids.WATER, 1000));
            IFluidHandler t = h.getLevel().getCapability(Capabilities.FluidHandler.BLOCK, dst, null);
            if (t == null)
                return;
            int amount = t.getFluidInTank(0).getAmount();
            if (amount > 0)
                t.drain(amount, IFluidHandler.FluidAction.EXECUTE);
            if (first[0] < 0 && amount > 0) {
                first[0] = h.getTick();
                m.mark(h.getTick());
            }
            if (first[0] >= 0 && h.getTick() > first[0] + 20 && ticks[0] < window) {
                sum[0] += amount;
                ticks[0]++;
                if (ticks[0] == window)
                    m.rate(sum[0] / (double) ticks[0], ticks[0]);
            }
        });
    }

    // ---- hand-off loops: real feeders -----------------------------------------------------------

    /**
     * Marks a meter and keeps a histogram of the periods it measures (those after its warm-up), shown on
     * the calib line as {@code periods=<ticks>x<count>,...}: an order effect shows as two values.
     */
    static final class Periods {
        final Meter m;
        final TreeMap<Long, Integer> hist = new TreeMap<>();
        long last = -1;
        int seen;
        String extra = "";

        Periods(Meter m) {
            this.m = m;
        }

        void mark(long tick) {
            m.mark(tick);
            seen++;
            if (last >= 0 && seen > Meter.WARM + 1)
                hist.merge(tick - last, 1, Integer::sum);
            last = tick;
            note();
        }

        void extra(String s) {
            extra = s;
            note();
        }

        private void note() {
            StringBuilder b = new StringBuilder("periods=");
            hist.forEach((k, v) -> b.append(b.length() > 8 ? "," : "").append(k).append('x').append(v));
            m.note(b + (extra.isEmpty() ? "" : " " + extra));
        }
    }

    /**
     * Corpus 5's saw station: a chest of andesite alloy over an unpowered smart chute (filter alloy, count
     * 1) over an upward saw filtered to shafts, on a motor. The chute grabs the next alloy long before the
     * saw is free and parks it at 0.5; an operation is the saw's inventory emptying after a load (the
     * shafts are killed). Model: {@code saw + chuteHandOff(gravity, 0, true)}, the two-actor loop.
     */
    static Run sawChute(GameTestHelper h, int target, int rpm) {
        Cycles.Ratio ho = Cycles.chuteHandOff(GRAVITY, 0, true);
        int own = Cycles.saw(200, 1, rpm);
        Meter m = new Meter("saw_chute", Integer.toString(rpm), "D200 n1 saw=" + own + " handoff=" + ho.num() + "/" + ho.den(),
                own + ho.ticks(), target);
        BlockPos saw = at(h, 8, 1, 8), chute = at(h, 8, 2, 8), chest = at(h, 8, 3, 8);
        set(h, saw, AllBlocks.MECHANICAL_SAW.getDefaultState().setValue(SawBlock.FACING, Direction.UP)
                .setValue(DirectionalAxisKineticBlock.AXIS_ALONG_FIRST_COORDINATE, true));
        motor(h, at(h, 7, 1, 8), Direction.EAST, rpm);
        FilteringBehaviour f = BlockEntityBehaviour.get(h.getLevel(), saw, FilteringBehaviour.TYPE);
        if (f == null)
            throw new GameTestAssertException("no filter on the saw");
        f.setFilter(new ItemStack(AllBlocks.SHAFT.get()));
        Item alloy = AllItems.ANDESITE_ALLOY.get();
        SteamHelpers.smartChute(h, chute, alloy, 1);
        chest(h, chest);
        fill(h, chest, alloy, 256);
        Periods per = new Periods(m);
        boolean[] loaded = {false};
        return new Run(m, () -> {
            killItems(h);
            SawBlockEntity s = beOrNull(h, saw, SawBlockEntity.class);
            if (s == null || s.getSpeed() == 0)
                return;
            boolean now = !s.inventory.isEmpty();
            if (loaded[0] && !now)
                per.mark(h.getTick());
            loaded[0] = now;
            if (count(h, chest, alloy) < 64)
                fill(h, chest, alloy, 128);
        });
    }

    /**
     * Corpus 1's crusher station: crushing wheels as in {@link #crusher}, a chest of raw iron over an
     * unpowered smart chute (filter raw iron, count 2) right above the controller. The controller takes
     * items only while empty; an operation is its inventory emptying after a load (the drops are killed).
     * Model: {@code crusher(400, 2) + chuteHandOff(gravity, 0, true)}.
     */
    static Run crusherChute(GameTestHelper h, int target, int rpm) {
        Cycles.Ratio ho = Cycles.chuteHandOff(GRAVITY, 0, true);
        int own = Cycles.crusher(400, 2, rpm);
        Meter m = new Meter("crusher_chute", Integer.toString(rpm), "D400 n2 crusher=" + own + " handoff=" + ho.num() + "/" + ho.den(),
                own + ho.ticks(), target);
        BlockPos a = at(h, 7, 2, 8), b = at(h, 9, 2, 8), ctrl = at(h, 8, 2, 8), chute = at(h, 8, 3, 8), chest = at(h, 8, 4, 8);
        BlockState wheel = AllBlocks.CRUSHING_WHEEL.getDefaultState().setValue(BlockStateProperties.AXIS, Direction.Axis.Z);
        set(h, a, wheel);
        set(h, b, wheel);
        motor(h, at(h, 7, 2, 7), Direction.SOUTH, -rpm);
        motor(h, at(h, 9, 2, 7), Direction.SOUTH, rpm);
        SteamHelpers.smartChute(h, chute, Items.RAW_IRON, 2);
        chest(h, chest);
        fill(h, chest, Items.RAW_IRON, 256);
        Periods per = new Periods(m);
        boolean[] loaded = {false};
        int[] most = {0};
        return new Run(m, () -> {
            killItems(h);
            CrushingWheelControllerBlockEntity c = beOrNull(h, ctrl, CrushingWheelControllerBlockEntity.class);
            if (c == null || c.crushingspeed == 0)
                return;
            int in = c.inventory.getStackInSlot(0).is(Items.RAW_IRON) ? c.inventory.getStackInSlot(0).getCount() : 0;
            if (in > most[0]) {
                most[0] = in;
                per.extra("load=" + in + " facing=" + h.getLevel().getBlockState(ctrl).getValue(BlockStateProperties.FACING).getName());
            }
            boolean now = !c.inventory.isEmpty();
            if (loaded[0] && !now)
                per.mark(h.getTick());
            loaded[0] = now;
            if (count(h, chest, Items.RAW_IRON) < 64)
                fill(h, chest, Items.RAW_IRON, 128);
        });
    }

    /**
     * Corpus 8's depot station in miniature: a fan blowing up through a walled lava source onto a depot
     * (blasting raw iron into iron ingots), a plain chute from a chest of raw iron above the depot (16 per
     * grab, parked at 0.5 while the depot holds a stack), and a mechanical arm meshed with the fan's cog that
     * takes from the depot into a brass funnel filtered to iron ingots on a chest (so it never lifts raw
     * iron). {@code order} is the placement order of the three actors, which is their order in the level's
     * ticker list: {@code c} the chute (and its chest), {@code f} the depot and fan (with the cog, motor and
     * lava), {@code a} the arm (its targets set once all exist). An operation is the output chest gaining
     * items. Model: {@code fanTicks(16) + chuteHandOff(gravity, armMoveTicks, false)}, the mean of a
     * three-actor loop: 159.5 at 256 RPM, where {@code cfa} (a cyclic order) should give 159 and {@code afc}
     * (the reverse) 160.
     */
    static Run fanDepotArm(GameTestHelper h, int target, int rpm, String order) {
        int time = AllConfigs.server().kinetics.fanProcessingTime.get();
        Cycles.Ratio ho = Cycles.chuteHandOff(GRAVITY, Cycles.armMoveTicks(rpm), false);
        int own = Cycles.fanTicks(16, time);
        Meter m = new Meter("fan_depot_arm", Integer.toString(rpm), "order=" + order + " T" + time + " n16 fan=" + own + " arm="
                + Cycles.armMoveTicks(rpm) + " handoff=" + ho.num() + "/" + ho.den(), own + ho.ticks(), target);
        BlockPos motorPos = at(h, 8, 1, 8), cogPos = at(h, 8, 2, 8), fanPos = at(h, 8, 3, 8), lava = at(h, 8, 4, 8),
                depotPos = at(h, 8, 5, 8), chutePos = at(h, 8, 6, 8), src = at(h, 8, 7, 8), armPos = at(h, 9, 2, 8),
                funnel = at(h, 9, 2, 6), out = at(h, 9, 2, 5);
        chest(h, out);
        SteamHelpers.brassFunnel(h, funnel, Direction.SOUTH, Items.IRON_INGOT);
        for (Direction d : Direction.Plane.HORIZONTAL)
            set(h, lava.relative(d), Blocks.GLASS.defaultBlockState());
        for (char c : order.toCharArray()) {
            switch (c) {
                case 'c' -> {
                    chest(h, src);
                    fill(h, src, Items.RAW_IRON, 640);
                    SteamHelpers.chute(h, chutePos);
                }
                case 'f' -> {
                    depot(h, depotPos);
                    motor(h, motorPos, Direction.UP, rpm);
                    cog(h, cogPos, Direction.Axis.Y);
                    SteamHelpers.fan(h, fanPos, Direction.UP);
                    set(h, lava, Blocks.LAVA.defaultBlockState());
                }
                case 'a' -> set(h, armPos, AllBlocks.MECHANICAL_ARM.getDefaultState());
                default -> throw new GameTestAssertException("unknown part " + c);
            }
        }
        armTargets(h, armPos, List.of(depotPos), List.of(funnel), ArmBlockEntity.SelectionMode.PREFER_FIRST);
        Periods per = new Periods(m);
        int[] last = {0};
        return new Run(m, () -> {
            int c = count(h, out, Items.IRON_INGOT);
            if (c > last[0]) {
                per.mark(h.getTick());
                per.extra("batch=" + (c - last[0]) + " arm=" + fmt(speed(h, armPos)));
            }
            last[0] = c;
            if (count(h, src, Items.RAW_IRON) < 128)
                fill(h, src, Items.RAW_IRON, 256);
        });
    }
}
