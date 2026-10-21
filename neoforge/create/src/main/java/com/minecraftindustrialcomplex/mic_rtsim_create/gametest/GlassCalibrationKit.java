package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim_create.glass.Cycles;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.belt.item.BeltConnectorItem;
import com.simibubi.create.content.kinetics.belt.transport.TransportedItemStack;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmInteractionPoint;
import com.simibubi.create.content.kinetics.motor.CreativeMotorBlockEntity;
import com.simibubi.create.content.logistics.depot.DepotBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.function.Consumer;

/**
 * Helpers for the glass box calibration GameTests ({@link GlassCalibrationGameTests}): placement (copied
 * from the glass corpus kit, not imported from it), feeding and the {@link Meter} that turns observed
 * completions into the one {@code [gametest] calib} line per machine and speed. Positions are absolute
 * unless a method says otherwise; {@link #at} turns a yard-relative position into an absolute one.
 */
public final class GlassCalibrationKit {
    private GlassCalibrationKit() {}

    // ---- positions and blocks -------------------------------------------------------------------

    /** The absolute position of a yard-relative block. The yard's polished andesite floor is the y 1 layer (blocks set there replace it); y 2 is the first air. */
    public static BlockPos at(GameTestHelper h, int x, int y, int z) {
        return h.absolutePos(new BlockPos(x, y, z));
    }

    public static void set(GameTestHelper h, BlockPos abs, BlockState state) {
        h.getLevel().setBlock(abs, state, 3);
    }

    @SuppressWarnings("unchecked")
    public static <T extends BlockEntity> T be(GameTestHelper h, BlockPos abs, Class<T> type) {
        BlockEntity be = h.getLevel().getBlockEntity(abs);
        if (!type.isInstance(be))
            throw new GameTestAssertException("expected a " + type.getSimpleName() + " at " + abs.toShortString() + ", found " + be);
        return (T) be;
    }

    /** The block entity at {@code abs} if it is a {@code type}, else null (not yet created, or replaced). */
    @SuppressWarnings("unchecked")
    public static <T extends BlockEntity> T beOrNull(GameTestHelper h, BlockPos abs, Class<T> type) {
        BlockEntity be = h.getLevel().getBlockEntity(abs);
        return type.isInstance(be) ? (T) be : null;
    }

    // ---- kinetics -------------------------------------------------------------------------------

    /**
     * A creative motor facing {@code facing} (its shaft comes out of that face) at {@code rpm}; negative
     * turns the other way. A motor facing a positive direction gives its value, facing a negative one
     * minus its value ({@code KineticBlockEntity.convertToDirection}).
     */
    public static void motor(GameTestHelper h, BlockPos abs, Direction facing, int rpm) {
        set(h, abs, AllBlocks.CREATIVE_MOTOR.getDefaultState().setValue(BlockStateProperties.FACING, facing));
        be(h, abs, CreativeMotorBlockEntity.class).generatedSpeed.setValue(rpm);
    }

    public static void shaft(GameTestHelper h, BlockPos abs, Direction.Axis axis) {
        set(h, abs, AllBlocks.SHAFT.getDefaultState().setValue(BlockStateProperties.AXIS, axis));
    }

    public static void cog(GameTestHelper h, BlockPos abs, Direction.Axis axis) {
        set(h, abs, AllBlocks.COGWHEEL.getDefaultState().setValue(BlockStateProperties.AXIS, axis));
    }

    /** The speed Create gives the block at {@code abs} now (its {@code getSpeed()}), 0 if not kinetic. */
    public static float speed(GameTestHelper h, BlockPos abs) {
        return h.getLevel().getBlockEntity(abs) instanceof KineticBlockEntity k ? k.getSpeed() : 0;
    }

    /**
     * A belt from {@code start} to {@code end} (both ends get shafts first, as a player's belt connector
     * needs): Create's own {@code BeltConnectorItem.createBelts}. Its axis is horizontal.
     */
    public static void belt(GameTestHelper h, BlockPos start, BlockPos end) {
        Direction.Axis along = start.getX() != end.getX() ? Direction.Axis.X : Direction.Axis.Z;
        Direction.Axis shaftAxis = along == Direction.Axis.X ? Direction.Axis.Z : Direction.Axis.X;
        shaft(h, start, shaftAxis);
        shaft(h, end, shaftAxis);
        BeltConnectorItem.createBelts(h.getLevel(), start, end);
    }

    // ---- stores ---------------------------------------------------------------------------------

    public static void chest(GameTestHelper h, BlockPos abs) {
        set(h, abs, Blocks.CHEST.defaultBlockState());
    }

    public static void depot(GameTestHelper h, BlockPos abs) {
        set(h, abs, AllBlocks.DEPOT.getDefaultState());
    }

    public static IItemHandler items(GameTestHelper h, BlockPos abs) {
        IItemHandler inv = h.getLevel().getCapability(Capabilities.ItemHandler.BLOCK, abs, null);
        if (inv == null)
            throw new GameTestAssertException("no item handler at " + abs.toShortString());
        return inv;
    }

    /** Puts {@code count} of {@code item} into the store at {@code abs}, in full stacks. Fails if it does not fit. */
    public static void fill(GameTestHelper h, BlockPos abs, Item item, int count) {
        IItemHandler inv = items(h, abs);
        int max = item.getDefaultMaxStackSize();
        while (count > 0) {
            int n = Math.min(max, count);
            ItemStack rest = ItemHandlerHelper.insertItemStacked(inv, new ItemStack(item, n), false);
            if (!rest.isEmpty())
                throw new GameTestAssertException("the store at " + abs.toShortString() + " is full");
            count -= n;
        }
    }

    /** How many of {@code item} the store at {@code abs} holds (its block capability, null side). */
    public static int count(GameTestHelper h, BlockPos abs, Item item) {
        IItemHandler inv = items(h, abs);
        int n = 0;
        for (int i = 0; i < inv.getSlots(); i++) {
            ItemStack s = inv.getStackInSlot(i);
            if (s.is(item))
                n += s.getCount();
        }
        return n;
    }

    public static DepotBehaviour depotBehaviour(GameTestHelper h, BlockPos abs) {
        DepotBehaviour d = BlockEntityBehaviour.get(h.getLevel(), abs, DepotBehaviour.TYPE);
        if (d == null)
            throw new GameTestAssertException("no depot at " + abs.toShortString());
        return d;
    }

    /** Empties a depot's output buffer (slots 1 to 8 of its item handler) and returns how many items it held. */
    public static int clearDepotOutputs(GameTestHelper h, BlockPos abs) {
        IItemHandler inv = items(h, abs);
        int n = 0;
        for (int i = 1; i < inv.getSlots(); i++)
            n += inv.extractItem(i, 64, false).getCount();
        return n;
    }

    /** Puts a fresh, centred stack on a depot (what a belt hands over once it has settled). */
    public static void placeOnDepot(GameTestHelper h, BlockPos abs, ItemStack stack) {
        depotBehaviour(h, abs).setCenteredHeldItem(new TransportedItemStack(stack));
    }

    /** Removes every item entity in and around the test's structure. */
    public static void killItems(GameTestHelper h) {
        h.killAllEntitiesOfClass(ItemEntity.class);
    }

    // ---- block entity settings ------------------------------------------------------------------

    /** Rewrites a block entity's saved NBT through {@code edit} and loads it back. */
    public static void editNbt(GameTestHelper h, BlockEntity be, Consumer<CompoundTag> edit) {
        var reg = h.getLevel().registryAccess();
        CompoundTag tag = be.saveWithFullMetadata(reg);
        edit.accept(tag);
        be.loadWithComponents(tag, reg);
        be.setChanged();
    }

    /**
     * A mechanical arm on the floor (a small cog with a Y axis: it turns with the cogs beside it) whose
     * targets are set as a player's arm item would: {@code take} in TAKE mode, then {@code deposit} in
     * DEPOSIT mode, with {@code mode} as its selection mode. Place it after every target: the arm resolves
     * its points on its first tick ({@code ArmBlockEntity.initInteractionPoints}).
     */
    public static void arm(GameTestHelper h, BlockPos abs, List<BlockPos> take, List<BlockPos> deposit, ArmBlockEntity.SelectionMode mode) {
        set(h, abs, AllBlocks.MECHANICAL_ARM.getDefaultState());
        armTargets(h, abs, take, deposit, mode);
    }

    /**
     * Sets the targets of the arm already at {@code abs}, as {@link #arm} does. A test that wants the arm
     * early in the level's ticker order (block entities tick in the order they were created) places the
     * arm block first and calls this once every target exists.
     */
    public static void armTargets(GameTestHelper h, BlockPos abs, List<BlockPos> take, List<BlockPos> deposit, ArmBlockEntity.SelectionMode mode) {
        Level level = h.getLevel();
        ListTag points = new ListTag();
        for (BlockPos p : take) {
            ArmInteractionPoint pt = armPoint(level, p);
            pt.cycleMode();
            if (pt.getMode() != ArmInteractionPoint.Mode.TAKE)
                throw new GameTestAssertException("the arm cannot take from " + p.toShortString());
            points.add(pt.serialize(abs));
        }
        for (BlockPos p : deposit)
            points.add(armPoint(level, p).serialize(abs));
        editNbt(h, be(h, abs, ArmBlockEntity.class), tag -> {
            tag.put("InteractionPoints", points);
            tag.putInt("ScrollValue", mode.ordinal());
        });
    }

    private static ArmInteractionPoint armPoint(Level level, BlockPos p) {
        ArmInteractionPoint pt = ArmInteractionPoint.create(level, p, level.getBlockState(p));
        if (pt == null)
            throw new GameTestAssertException("no arm interaction point for " + level.getBlockState(p) + " at " + p.toShortString());
        return pt;
    }

    // ---- measuring ------------------------------------------------------------------------------

    /** Ticks of operations to aim for: 5 to 20 operations, about 3,000 ticks of them. */
    public static int targetOps(double model) {
        if (!(model > 0))
            return 5;
        return (int) Math.max(5, Math.min(20, Math.round(3000.0 / model)));
    }

    /** The last tick a test keeps measuring: setup, a discarded first operation and the target, with room. */
    public static int deadline(double model, int ops) {
        if (!(model > 0))
            return 600;
        return 300 + (int) Math.ceil(model * (ops + 3));
    }

    /**
     * One calibration measurement: a machine at one speed. A test calls {@link #mark} on every tick an
     * operation completes (the tick it observes the completion; a constant observation lag cancels out of
     * the periods). The first {@link #WARM} completions are discarded (they include spin-up and the first
     * placement); the mean is {@code (t_last - t_first) / ops} over the {@code ops} periods after them. A
     * rate measurement (the pump) uses {@link #rate} instead.
     */
    public static final class Meter {
        public static final int WARM = 1;

        public final String machine;
        public final String rpm;
        public final String key;
        /** The model's ticks per operation (or units per tick for a rate); {@link Cycles#NEVER} for never. */
        public final double model;
        public final int target;
        private final List<Long> marks = new ArrayList<>();
        private Double rate;
        private int rateTicks;
        private boolean logged;
        private String note = "";

        public Meter(String machine, String rpm, String key, double model, int target) {
            this.machine = machine;
            this.rpm = rpm;
            this.key = key;
            this.model = model;
            this.target = target;
        }

        public boolean modelNever() {
            return model == Cycles.NEVER;
        }

        public void mark(long tick) {
            marks.add(tick);
        }

        public void mark(long tick, int n) {
            for (int i = 0; i < n; i++)
                marks.add(tick);
        }

        /** Completions seen so far, warm-up included. */
        public int completions() {
            return marks.size();
        }

        public int ops() {
            if (rate != null)
                return rateTicks;
            return Math.max(0, marks.size() - 1 - WARM);
        }

        public double mean() {
            if (rate != null)
                return rate;
            int n = ops();
            if (n <= 0)
                return Double.NaN;
            return (marks.get(marks.size() - 1) - marks.get(WARM)) / (double) n;
        }

        public void rate(double perTick, int ticks) {
            rate = perTick;
            rateTicks = ticks;
        }

        /** Extra words for the log line (after the standard fields). */
        public void note(String note) {
            this.note = note;
        }

        public boolean done() {
            return rate != null || ops() >= target;
        }

        public void log() {
            if (logged)
                return;
            logged = true;
            String m = modelNever() ? "never" : fmt(model);
            boolean none = rate == null && ops() <= 0;
            String measured = none ? (completions() == 0 ? "never" : "n/a") : fmt(mean());
            String diff = none || modelNever() ? "n/a" : fmt(mean() - model);
            MicRtsim.LOGGER.info("[gametest] calib {} rpm={} key={} model={} measured={} ops={} diff={}{}",
                    machine, rpm, key, m, measured, ops(), diff, note.isEmpty() ? "" : " " + note);
        }

        @Override
        public String toString() {
            return machine + " rpm=" + rpm + " key=" + key + ": " + completions() + " completions, " + ops() + " of " + target + " ops";
        }
    }

    /** A number for the log: an integer as is, else up to three decimals. */
    public static String fmt(double v) {
        if (Double.isNaN(v))
            return "n/a";
        if (v == Math.rint(v) && Math.abs(v) < 1e9)
            return Long.toString((long) v);
        String s = String.format(Locale.ROOT, "%.3f", v);
        s = s.replaceAll("0+$", "");
        return s.endsWith(".") ? s.substring(0, s.length() - 1) : s;
    }

    /**
     * Runs {@code step} every tick from the test's first tick (right after the build, before any block
     * entity has ticked) until the meter is done, then logs the calib line and succeeds. At
     * {@code deadline} it logs what it has and succeeds if the machine completed at least one operation (or
     * the model says it never does); otherwise it keeps failing until the timeout, so the test fails only
     * when the machine never completed an operation. Any other exception from {@code step} (only
     * {@link GameTestAssertException} is caught by a sequence; anything else would stop the server) is
     * kept and reported as that assertion every tick until the timeout.
     */
    public static void drive(GameTestHelper h, Meter m, int deadline, Runnable step) {
        RuntimeException[] error = {null};
        h.succeedWhen(() -> {
            if (error[0] != null)
                throw new GameTestAssertException(m.machine + " at rpm " + m.rpm + ": " + error[0]);
            try {
                step.run();
            } catch (GameTestAssertException e) {
                throw e;
            } catch (RuntimeException e) {
                error[0] = e;
                MicRtsim.LOGGER.error("[gametest] calib {} rpm={} key={}: step failed", m.machine, m.rpm, m.key, e);
                throw new GameTestAssertException(m.machine + " at rpm " + m.rpm + ": " + e);
            }
            if (m.done()) {
                m.log();
                return;
            }
            if (h.getTick() >= deadline) {
                if (m.completions() > 0 || m.modelNever()) {
                    m.log();
                    return;
                }
                throw new GameTestAssertException(m.machine + " at rpm " + m.rpm + " (" + m.key + ") never completed an operation in "
                        + deadline + " ticks");
            }
            throw new GameTestAssertException("measuring " + m);
        });
    }
}
