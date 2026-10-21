package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmInteractionPoint;
import com.simibubi.create.content.logistics.chute.SmartChuteBlock;
import com.simibubi.create.content.logistics.funnel.FunnelBlock;
import com.simibubi.create.content.processing.burner.BlazeBurnerBlock;
import com.simibubi.create.content.processing.burner.BlazeBurnerBlockEntity;
import com.simibubi.create.foundation.blockEntity.SmartBlockEntity;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.world.InteractionResultHolder;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.ChestBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.ChestType;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Placement helpers for the steam and heat factories of the glass corpus (8 arm-fed burners and depots,
 * 9 boiler, 12 threshold smelter), on top of {@link GlassCorpusKit}. Positions are absolute. Block entity
 * settings a player makes with an item or a screen (arm targets, burner fuel, filters) are set through
 * Create's own API or the block entity's NBT ({@code loadWithComponents}), as the older station tests do.
 */
public final class SteamHelpers {
    private SteamHelpers() {}

    /** Rewrites a block entity's saved NBT through {@code edit} and loads it back. */
    public static void editNbt(GameTestHelper h, BlockEntity be, java.util.function.Consumer<CompoundTag> edit) {
        var reg = h.getLevel().registryAccess();
        CompoundTag tag = be.saveWithFullMetadata(reg);
        edit.accept(tag);
        be.loadWithComponents(tag, reg);
        be.setChanged();
    }

    /**
     * Marks every block entity in the box from {@code from} to {@code to} (absolute, inclusive) as placed by a
     * player, as it would be in play: each becomes a district seed if it is industrial
     * ({@code Districts}: a player-placed block entity with a capability or that a plugin recognises). A
     * factory wider than a chunk needs this. {@link GlassCorpus#prepare} seeds only the stores and the
     * anchor, and a chunk with neither joins only through a face-adjacent industrial neighbour across its
     * border: a large cog meshing a small cog diagonally, or a gap, leaves it out, and with it the anchor
     * when the yard's random position puts a chunk border there.
     */
    public static void seedAll(GameTestHelper h, BlockPos from, BlockPos to) {
        for (BlockPos p : BlockPos.betweenClosed(from, to))
            if (h.getLevel().getBlockEntity(p) != null)
                IndustryKit.playerPlaced(h, p.immutable());
    }

    // ---- blaze burners --------------------------------------------------------------------------

    /**
     * A blaze burner with a blaze and no fuel (heat SMOULDERING). The block's default state is the empty
     * burner (heat NONE), which has no block entity ({@code BlazeBurnerBlock.newBlockEntity}).
     */
    public static void burner(GameTestHelper h, BlockPos abs) {
        set(h, abs, AllBlocks.BLAZE_BURNER.getDefaultState().setValue(BlazeBurnerBlock.HEAT_LEVEL, BlazeBurnerBlock.HeatLevel.SMOULDERING));
    }

    /**
     * What a player does with {@code count} of {@code fuel} in hand on the burner: {@code BlazeBurnerBlock.tryInsert}
     * with {@code forceOverflow} true (only a non-fake player may fill past {@code INSERTION_THRESHOLD}, up to
     * {@code MAX_HEAT_CAPACITY} 10,000 ticks). Returns the burn time left afterwards.
     */
    public static int topUpAsPlayer(GameTestHelper h, BlockPos abs, Item fuel, int count) {
        Level level = h.getLevel();
        for (int i = 0; i < count; i++) {
            InteractionResultHolder<ItemStack> r = BlazeBurnerBlock.tryInsert(level.getBlockState(abs), level, abs, new ItemStack(fuel), false, true, false);
            if (!r.getResult().consumesAction())
                throw new GameTestAssertException("the burner at " + abs.toShortString() + " refused " + id(fuel));
        }
        return be(h, abs, BlazeBurnerBlockEntity.class).getRemainingBurnTime();
    }

    /**
     * Sets a burner's regular fuel to {@code ticks} of burn time directly (NBT {@code fuelLevel} 1 = NORMAL,
     * {@code burnTimeRemaining}): a starting state a player could reach by hand, used to stagger burners.
     */
    public static void burnerFuel(GameTestHelper h, BlockPos abs, int ticks) {
        editNbt(h, be(h, abs, BlazeBurnerBlockEntity.class), tag -> {
            tag.putInt("fuelLevel", 1);
            tag.putInt("burnTimeRemaining", ticks);
        });
    }

    public static int burnTime(GameTestHelper h, BlockPos abs) {
        return h.getLevel().getBlockEntity(abs) instanceof BlazeBurnerBlockEntity b ? b.getRemainingBurnTime() : -1;
    }

    public static BlazeBurnerBlock.HeatLevel heat(GameTestHelper h, BlockPos abs) {
        return BlazeBurnerBlock.getHeatLevelOf(h.getLevel().getBlockState(abs));
    }

    // ---- logistics ------------------------------------------------------------------------------

    public static void depot(GameTestHelper h, BlockPos abs) {
        set(h, abs, AllBlocks.DEPOT.getDefaultState());
    }

    public static void chute(GameTestHelper h, BlockPos abs) {
        set(h, abs, AllBlocks.CHUTE.getDefaultState());
    }

    /**
     * A smart chute (unpowered) filtering {@code filter} that pulls at most {@code count} per extraction from the
     * inventory above ({@code SmartChuteBlockEntity.getExtractionAmount}: the filter count while the count
     * is shown, which it is when the block above is an inventory).
     */
    public static void smartChute(GameTestHelper h, BlockPos abs, Item filter, int count) {
        // the block's default state is POWERED true (SmartChuteBlock's constructor): a powered smart chute
        // never activates (canActivate), so place it unpowered as a player would without a signal
        set(h, abs, AllBlocks.SMART_CHUTE.getDefaultState().setValue(SmartChuteBlock.POWERED, false));
        FilteringBehaviour f = filtering(h, abs);
        f.setFilter(new ItemStack(filter));
        f.count = count;
        f.upTo = true;
    }

    /**
     * A brass funnel in insert mode, facing {@code facing} (it inserts into the block behind it,
     * {@code facing.getOpposite()}), filtered to {@code filter} (null: no filter).
     */
    public static void brassFunnel(GameTestHelper h, BlockPos abs, Direction facing, Item filter) {
        set(h, abs, AllBlocks.BRASS_FUNNEL.getDefaultState().setValue(BlockStateProperties.FACING, facing).setValue(FunnelBlock.EXTRACTING, false));
        if (filter != null)
            filtering(h, abs).setFilter(new ItemStack(filter));
    }

    public static FilteringBehaviour filtering(GameTestHelper h, BlockPos abs) {
        FilteringBehaviour f = be(h, abs, SmartBlockEntity.class).getBehaviour(FilteringBehaviour.TYPE);
        if (f == null)
            throw new GameTestAssertException("no filter at " + abs.toShortString());
        return f;
    }

    /**
     * A double chest: {@code left} gets type LEFT and {@code right} type RIGHT, both facing {@code facing};
     * {@code right} must be {@code left}'s neighbour on the facing's clockwise side
     * ({@code ChestBlock.getConnectedDirection}). Either half's capability covers both: list one as the store.
     */
    public static void doubleChest(GameTestHelper h, BlockPos left, BlockPos right, Direction facing) {
        BlockState base = Blocks.CHEST.defaultBlockState().setValue(ChestBlock.FACING, facing);
        if (!left.relative(facing.getClockWise()).equals(right))
            throw new GameTestAssertException("a double chest's right half sits clockwise of the left half");
        set(h, left, base.setValue(ChestBlock.TYPE, ChestType.LEFT));
        set(h, right, base.setValue(ChestBlock.TYPE, ChestType.RIGHT));
    }

    // ---- kinetics -------------------------------------------------------------------------------

    /**
     * The kinetic network's stress and capacity as the block at {@code abs} last heard them (its saved
     * {@code Network} tag), "stress/capacity SU", for logs.
     */
    public static String stressText(GameTestHelper h, BlockPos abs) {
        BlockEntity be = h.getLevel().getBlockEntity(abs);
        if (be == null)
            return "no block entity";
        CompoundTag n = be.saveWithoutMetadata(h.getLevel().registryAccess()).getCompound("Network");
        return n.isEmpty() ? "no network" : n.getFloat("Stress") + "/" + n.getFloat("Capacity") + " SU";
    }

    /** An encased fan blowing out of its {@code facing} side (its shaft is on the opposite side). */
    public static void fan(GameTestHelper h, BlockPos abs, Direction facing) {
        set(h, abs, AllBlocks.ENCASED_FAN.getDefaultState().setValue(BlockStateProperties.FACING, facing));
    }

    /**
     * A mechanical arm on the floor (a small cog with a Y axis: it turns with the cogs beside it) whose
     * targets are set as a player's arm item would: {@code take} in TAKE mode, then {@code deposit} in
     * DEPOSIT mode, each in list order (the arm's input and output order), with {@code mode} as its
     * selection mode. Place it after every target: the arm resolves its points on its first tick
     * ({@code ArmBlockEntity.initInteractionPoints}) and drops any whose block is missing.
     */
    public static void arm(GameTestHelper h, BlockPos abs, List<BlockPos> take, List<BlockPos> deposit, ArmBlockEntity.SelectionMode mode) {
        set(h, abs, AllBlocks.MECHANICAL_ARM.getDefaultState());
        Level level = h.getLevel();
        ListTag points = new ListTag();
        for (BlockPos p : take) {
            ArmInteractionPoint pt = point(level, p);
            pt.cycleMode();
            if (pt.getMode() != ArmInteractionPoint.Mode.TAKE)
                throw new GameTestAssertException("the arm cannot take from " + p.toShortString());
            points.add(pt.serialize(abs));
        }
        for (BlockPos p : deposit)
            points.add(point(level, p).serialize(abs));
        editNbt(h, be(h, abs, ArmBlockEntity.class), tag -> {
            tag.put("InteractionPoints", points);
            tag.putInt("ScrollValue", mode.ordinal());
        });
    }

    private static ArmInteractionPoint point(Level level, BlockPos p) {
        ArmInteractionPoint pt = ArmInteractionPoint.create(level, p, level.getBlockState(p));
        if (pt == null)
            throw new GameTestAssertException("no arm interaction point for " + level.getBlockState(p) + " at " + p.toShortString());
        return pt;
    }
}
