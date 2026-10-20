package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.entity.BlockEntity;

import java.util.function.Consumer;

/**
 * What a plugin (INDUSTRY.md §12, layer 2 and 3: {@code mic_rtsim_create}, later PowerGrid, Destroy ...)
 * adds to the mod-agnostic district sampler. Every method has a default that adds nothing, so the core
 * runs the black box with NeoForge capabilities alone (D4, C19). Register with
 * {@link IndustryApi#register}. Called on the server thread.
 */
public interface IndustryPlugin {

    /**
     * True for a block entity this plugin counts as industrial although it may expose no item, fluid or
     * energy capability: Create kinetics (shafts, cogs, belts), pipes, chutes. Industrial block entities
     * that touch across a chunk border join their chunks into one district (I2: closed under physical
     * connection).
     */
    default boolean isIndustrial(BlockEntity be) {
        return false;
    }

    /** Positions joined to this block entity beyond touching (chain conveyor connections, wires). */
    default void links(BlockEntity be, Consumer<BlockPos> out) {}

    /**
     * For a multiblock inventory, the position whose capability is the whole inventory (a vault's or a
     * tank's controller), so it is counted once; null when this plugin does not know the block.
     */
    default BlockPos canonicalInventory(BlockEntity be) {
        return null;
    }

    /** Where {@link #inventories} reports a boundary inventory. */
    interface InventorySink {
        /** {@code extraKind}: {@code IndRecords.Inventory.KIND_POSTBOX}, {@code KIND_PACKAGER_TARGET} or {@code KIND_PLUGIN}. */
        void add(BlockPos pos, int extraKind);
    }

    /** Boundary inventories this block entity adds beyond tagged storage (I3): packager targets, postboxes. */
    default void inventories(BlockEntity be, InventorySink out) {}

    /** A station inside a district: its stable id (a hash of its UUID), position and name. */
    record StationInfo(long id, BlockPos pos, String name) {}

    /** The station this block entity is, or null. The first one (lowest position) names the district. */
    default StationInfo station(BlockEntity be) {
        return null;
    }

    /**
     * The not-learnable code (IndRecords.NotLearnable) when industrial block entities {@code a} and
     * {@code b} touch across a district boundary (a shaft split by {@code /rtsim district split}), or 0.
     */
    default int crossing(BlockEntity a, BlockEntity b) {
        return 0;
    }

    /** What a district scan saw that the sweep needs: a creative block, positions to watch. */
    interface ScanNotes {
        void creative();

        void watch(BlockPos pos);
    }

    /** Called for every block entity a district scan visits. */
    default void scanned(BlockEntity be, ScanNotes notes) {}

    /** Sweep flags ({@code IndRecords.SweepEnd.FLAG_OVERSTRESSED}) from the watched positions, read each sweep. */
    default int sweepFlags(ServerLevel level, District district) {
        return 0;
    }

    /** Where {@link #expand} puts a container item's contents. */
    interface ItemSink {
        void add(ItemStack stack, long count);
    }

    /**
     * Expands a container item (a Create package) into its contents, wherever it is counted: stock,
     * carriages, ports. Return true when it did; the item itself is then not counted.
     */
    default boolean expand(ItemStack stack, ItemSink out) {
        return false;
    }

    /** Once per tick, after the core's own industry work (train departures waiting for a destination). */
    default void tick(Industry industry, long now) {}

    /**
     * Reads and writes an inventory of a kind this plugin declared (a postbox, whose packages are
     * counted by their contents), or null to use the capabilities.
     */
    default StockAccess access(ServerLevel level, District.Inventory inventory) {
        return null;
    }
}
