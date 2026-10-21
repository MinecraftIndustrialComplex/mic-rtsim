package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.BlockPos;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;

import java.util.List;
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

    /**
     * The shape of a store this plugin declared with {@code KIND_PLUGIN} (a PowerGrid battery):
     * {@code {extra kind bits, item slots, fluid capacity, energy capacity}}, or null when it is not this
     * plugin's. Without one the core reads the shape from the capabilities.
     */
    default long[] describe(ServerLevel level, BlockPos pos) {
        return null;
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

    /** This plugin's own counters as one line of {@code /rtsim district perf}, or null for none. */
    default String perfLine() {
        return null;
    }

    /**
     * The industry module was switched off and is on again (U10): the engine knows nothing of what was
     * sent before, so send again whatever this plugin keeps the engine told (a power link between
     * districts), as at a session start. Runs once, after the core's districts are announced.
     */
    default void announce(Industry industry) {}

    /**
     * Reads and writes an inventory of a kind this plugin declared (a postbox, whose packages are
     * counted by their contents), or null to use the capabilities.
     */
    default StockAccess access(ServerLevel level, District.Inventory inventory) {
        return null;
    }

    // ---- deposits (I11, F15; docs/overnight-choices-indplugins.md) ------------------------------

    /** Where {@link #deposits} reports a finite deposit: a good of class {@code d}. */
    interface DepositSink {
        /**
         * {@code plugin}: an {@code IndRecords.DepositPlugin} code. {@code good}: the deposit's GOOD name,
         * unique to this deposit (the engine keys a district's deposits by good, so two chunks' veins are two
         * goods). {@code remaining} is in the plugin's own unit; {@code infinite} deposits are never stock.
         */
        void add(int plugin, String good, long remaining, boolean infinite);
    }

    /** The deposits of one loaded member chunk of a district, read once per sweep (IND_DEPOSIT_SAMPLE). */
    default void deposits(ServerLevel level, District district, LevelChunk chunk, DepositSink out) {}

    /**
     * Applies {@code delta} to the deposit named {@code good} (class {@code d}) in chunk (chunkX, chunkZ):
     * from a reconcile, with the chunk loaded and its epoch written in the same tick, or from
     * IND_DEPOSIT_DELTA, for a store in level SavedData that may have no chunk loaded. Returns the amount
     * applied, with the sign of {@code delta}, or null when the good is not this plugin's.
     */
    default Long applyDeposit(ServerLevel level, int chunkX, int chunkZ, String good, long delta) {
        return null;
    }

    // ---- damage (C18, I23) and repair (I1) -----------------------------------------------------

    /**
     * Positions besides {@code pos} and its six neighbours that breaking {@code pos} removes too (the other
     * segments of a Create belt). Called before a reconcile breaks it, so their states are known before.
     */
    default void alsoBroken(ServerLevel level, BlockPos pos, BlockState state, Consumer<BlockPos> out) {}

    /**
     * The block-entity data a rebuild of this block may carry (what a Schematicannon places safely), or
     * null for none. Asked before a reconcile breaks it; the first plugin with an answer gives it.
     */
    default CompoundTag rebuildData(ServerLevel level, BlockPos pos, BlockState state, BlockEntity be) {
        return null;
    }

    /**
     * A reconcile broke blocks (an unwatched attack's damage, C18, I23): every block it removed, the target
     * and what went with it, with its state before. Only the sim's breaks: never a player's.
     */
    default void destroyed(Industry industry, ServerLevel level, District district, List<DestroyedBlock> blocks, long now) {}

    // ---- the defence survey (I21) ---------------------------------------------------------------

    /** True for a machine that hurts mobs that walk into it (Create saws, drills, crushing wheels). */
    default boolean killMachine(BlockEntity be) {
        return false;
    }

    /** Where {@link #turret} reports an auto-firing turret. */
    interface TurretSink {
        /**
         * One turret: its strength against each threat class ({@code IndRecords.ThreatClass}, in the
         * engine's attacker units), the GOOD name of the ammunition it holds (null: needs none or unknown)
         * and how much of it is loaded.
         */
        void add(BlockPos pos, float[] strength, String ammoGood, long ammo);
    }

    /** Auto-firing turrets this block entity is (Create Radars, Create Big Cannons). */
    default void turret(BlockEntity be, TurretSink out) {}

    // ---- the glass box (GLASSBOX.md §7.1) -------------------------------------------------------

    /** This plugin's graph extractor for the glass box's compile (G1), or null. */
    default com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassExtractor glass() {
        return null;
    }

    // ---- conditions (I12) -----------------------------------------------------------------------

    /**
     * The world's season calendar, {@code {season, seasonCount, seasonLength (ticks), seasonOrigin (the
     * game time season 0 last began)}}, or null when this plugin knows none (IND_CONDITIONS).
     */
    default long[] season(MinecraftServer server) {
        return null;
    }

    /**
     * The temperature at a position in degrees Celsius, or NaN when this plugin knows none
     * (mic-climate, Thermoo). Vanilla's biome temperature is the core's fallback.
     */
    default float temperature(ServerLevel level, BlockPos pos) {
        return Float.NaN;
    }
}
