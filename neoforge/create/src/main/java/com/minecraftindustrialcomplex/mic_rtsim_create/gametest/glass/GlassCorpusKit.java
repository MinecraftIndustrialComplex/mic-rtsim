package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.belt.BeltBlockEntity;
import com.simibubi.create.content.kinetics.belt.item.BeltConnectorItem;
import com.simibubi.create.content.kinetics.motor.CreativeMotorBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.items.IItemHandler;
import net.neoforged.neoforge.items.ItemHandlerHelper;

import java.util.Collection;
import java.util.Map;
import java.util.TreeMap;

/**
 * Placement and counting helpers for the glass corpus (the shared {@code IndustryKit} stays untouched).
 * Every position here is absolute unless a method says otherwise; {@link #at} turns a yard-relative
 * position into an absolute one.
 */
public final class GlassCorpusKit {
    private GlassCorpusKit() {}

    public static final int TICKS_PER_MINUTE = 1200;

    // ---- positions and blocks -------------------------------------------------------------------

    /** The absolute position of a yard-relative block (y 0 is the yard's floor; build from y 1). */
    public static BlockPos at(GameTestHelper h, int x, int y, int z) {
        return h.absolutePos(new BlockPos(x, y, z));
    }

    public static Block block(String id) {
        Block b = BuiltInRegistries.BLOCK.get(ResourceLocation.parse(id));
        if (b == Blocks.AIR && !id.equals("minecraft:air"))
            throw new GameTestAssertException("no block " + id);
        return b;
    }

    public static Item item(String id) {
        return BuiltInRegistries.ITEM.get(ResourceLocation.parse(id));
    }

    public static String id(Item item) {
        return BuiltInRegistries.ITEM.getKey(item).toString();
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

    // ---- kinetics -------------------------------------------------------------------------------

    /**
     * A creative motor facing {@code facing} (its shaft comes out of that face) at {@code rpm}; negative
     * turns the other way. Its capacity is 16,384 SU per RPM ({@code AllBlocks.CREATIVE_MOTOR},
     * {@code CStress.setCapacity(16384.0)}), far above any corpus build's stress.
     */
    public static void motor(GameTestHelper h, BlockPos abs, Direction facing, int rpm) {
        set(h, abs, AllBlocks.CREATIVE_MOTOR.getDefaultState().setValue(BlockStateProperties.FACING, facing));
        CreativeMotorBlockEntity m = be(h, abs, CreativeMotorBlockEntity.class);
        m.generatedSpeed.setValue(rpm);
    }

    public static void shaft(GameTestHelper h, BlockPos abs, Direction.Axis axis) {
        set(h, abs, AllBlocks.SHAFT.getDefaultState().setValue(BlockStateProperties.AXIS, axis));
    }

    /** Shafts on every block from {@code from} to {@code to} inclusive (one axis). */
    public static void shafts(GameTestHelper h, BlockPos from, BlockPos to) {
        Direction.Axis axis = from.getX() != to.getX() ? Direction.Axis.X : from.getY() != to.getY() ? Direction.Axis.Y : Direction.Axis.Z;
        for (BlockPos p : BlockPos.betweenClosed(from, to))
            shaft(h, p.immutable(), axis);
    }

    public static void cog(GameTestHelper h, BlockPos abs, Direction.Axis axis) {
        set(h, abs, AllBlocks.COGWHEEL.getDefaultState().setValue(BlockStateProperties.AXIS, axis));
    }

    public static void largeCog(GameTestHelper h, BlockPos abs, Direction.Axis axis) {
        set(h, abs, AllBlocks.LARGE_COGWHEEL.getDefaultState().setValue(BlockStateProperties.AXIS, axis));
    }

    /** The speed Create gives the block at {@code abs} now (its {@code getSpeed()}), 0 if not kinetic. */
    public static float speed(GameTestHelper h, BlockPos abs) {
        return h.getLevel().getBlockEntity(abs) instanceof KineticBlockEntity k ? k.getSpeed() : 0;
    }

    public static boolean overstressed(GameTestHelper h, BlockPos abs) {
        return h.getLevel().getBlockEntity(abs) instanceof KineticBlockEntity k && k.isOverStressed();
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

    /** The direction items move on the belt at {@code abs} now, or null (stopped, or no belt). */
    public static Direction beltMovement(GameTestHelper h, BlockPos abs) {
        if (!(h.getLevel().getBlockEntity(abs) instanceof BeltBlockEntity b) || b.getSpeed() == 0)
            return null;
        return b.getMovementFacing();
    }

    // ---- stores ---------------------------------------------------------------------------------

    public static void chest(GameTestHelper h, BlockPos abs) {
        set(h, abs, Blocks.CHEST.defaultBlockState());
    }

    public static void barrel(GameTestHelper h, BlockPos abs) {
        set(h, abs, Blocks.BARREL.defaultBlockState());
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

    /**
     * The contents of the stores, per good: items by id, fluids as {@code fluid:<id>} in mB. Each position
     * is read once through its block capability (null side), as the sampler does; a position listed twice
     * counts once.
     */
    public static Map<String, Long> totals(ServerLevel level, Collection<BlockPos> stores) {
        Map<String, Long> out = new TreeMap<>();
        java.util.HashSet<BlockPos> seen = new java.util.HashSet<>();
        for (BlockPos p : stores) {
            if (!seen.add(p))
                continue;
            IItemHandler inv = level.getCapability(Capabilities.ItemHandler.BLOCK, p, null);
            if (inv != null)
                for (int i = 0; i < inv.getSlots(); i++) {
                    ItemStack s = inv.getStackInSlot(i);
                    if (!s.isEmpty())
                        out.merge(id(s.getItem()), (long) s.getCount(), Long::sum);
                }
            IFluidHandler fl = level.getCapability(Capabilities.FluidHandler.BLOCK, p, null);
            if (fl != null)
                for (int i = 0; i < fl.getTanks(); i++) {
                    FluidStack f = fl.getFluidInTank(i);
                    if (!f.isEmpty())
                        out.merge("fluid:" + BuiltInRegistries.FLUID.getKey(f.getFluid()), (long) f.getAmount(), Long::sum);
                }
        }
        return out;
    }

    public static void log(String factory, String what, Object value) {
        MicRtsim.LOGGER.info("[gametest] glass corpus {}: {} = {}", factory, what, value);
    }

    /** Per minute from per tick. */
    public static double perMinute(double perTick) {
        return perTick * TICKS_PER_MINUTE;
    }
}
