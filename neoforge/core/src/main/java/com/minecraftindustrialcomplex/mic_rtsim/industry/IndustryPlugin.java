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

    /**
     * Whether this block entity is a <i>machine</i> that starts a district: true seeds, false never does
     * (storage, transmission, transport and decoration of this plugin's own: a vault, a shaft, a belt),
     * null has no opinion and the core's generic rule decides (a modded block entity that ticks on the
     * server and has an item, fluid or energy capability). Vanilla blocks and anything in
     * {@code #mic_rtsim:district_stock} or {@code #mic_rtsim:district_not_seed} never seed whatever
     * this says. The default: what {@link #isIndustrial} recognises seeds, the rest has no opinion.
     * Seeding is a separate question from {@link #isIndustrial}: an industrial block entity that does not
     * seed still joins a district that has a seed, and still joins the chunks it touches.
     */
    default Boolean seed(BlockEntity be) {
        return isIndustrial(be) ? Boolean.TRUE : null;
    }

    /**
     * Whether this block entity <i>can become</i> a seed later without a player placing anything: the anchor of
     * a contraption (a bearing, a piston, a pulley), which seeds only while the contraption it carries has
     * working blocks. The core records such a block at placement and keeps it recorded while {@link #seed}
     * says false; it counts only while {@link #seed} says true.
     */
    default boolean mayBecomeSeed(BlockEntity be) {
        return false;
    }

    /**
     * Positions in other chunks joined to this block entity beyond a face touch (SECTIONS.md §2.1): J2, what
     * moves goods across a border without touching (a fan's current, an arm's points, an ejector's target),
     * and J4, rotation that is not a face (diagonal cogs). Read only; never loads a chunk. The core drops a
     * target in the block entity's own chunk, in a chunk that is not loaded, or that is not an industrial
     * block entity, and joins the two chunks for the rest.
     */
    default void links(BlockEntity be, Consumer<BlockPos> out) {}

    /**
     * False while this block entity's {@link #links} cannot be read yet (a mechanical arm before its first
     * tick): its chunk keeps the join pairs its district saved (SECTIONS.md §2.3, choice SC4). The default: true.
     */
    default boolean linksKnown(BlockEntity be) {
        return true;
    }

    /**
     * Whether a face touch of this industrial block entity across a chunk border joins the two chunks (J1).
     * False for blocks that carry goods or trains between districts without making them one (a Create chain
     * conveyor, a station: SECTIONS.md §2.1, choices SC1 and SC6). The default: true.
     */
    default boolean touchJoins(BlockEntity be) {
        return true;
    }

    /**
     * Whether this block entity is a part of a network that a district's compile follows out of its chunks
     * (SECTIONS.md §3.6, choice SC77: a kinetic part, a chain conveyor). A player placing or breaking one in a
     * chunk of no district edits each district whose network reaches that chunk; any other block entity there
     * (a chest, a sign) edits none. The default: no.
     */
    default boolean networkPart(BlockEntity be) {
        return false;
    }

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
     * plugin's. Without one the core reads the shape from the capabilities. Also asked for an inventory whose
     * parts are not all loaded ({@link #partsLoaded}): its shape then, without reading the parts; without one
     * the detection leaves it out, as an inventory in a chunk that is not loaded.
     */
    default long[] describe(ServerLevel level, BlockPos pos) {
        return null;
    }

    /**
     * Whether reading {@code be}'s capabilities reads only chunks that are loaded (FULL), {@code be}'s own
     * chunk being loaded (R2-11): false for a multiblock inventory with a part in a chunk that is not, whose
     * handler reads that part through {@code Level.getBlockEntity} (a Create vault's controller builds its
     * handler from every part). That read waits for a chunk still loading, running every other chunk load due
     * on the server thread meanwhile, and loads a chunk that is not loaded. The core then treats the inventory
     * as one in a chunk not loaded: a sweep skips it, a detection takes its shape from {@link #describe}. The
     * default: true.
     */
    default boolean partsLoaded(ServerLevel level, BlockEntity be) {
        return true;
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
     * Whether this block entity is a station, even before it has an identity (a Create station's track graph
     * forms a moment after it is placed). A station's chunk with no seed of its own joins exactly one district
     * (J6): the one its interfaces serve, else the lowest beside it; it never starts one (DS-10).
     */
    default boolean isStation(BlockEntity be) {
        return station(be) != null;
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

    // ---- fights (MOWZIE.md §4.13, ledger 19) ---------------------------------------------------

    /**
     * An attacker's strength in an observed fight (IND_FIGHT's attacker strength), in the engine's unit:
     * one ordinary mob (a zombie, health x attack 20 x 3). NaN when this plugin does not know the entity;
     * the first plugin with a finite, non-negative answer wins, and the core counts 1 otherwise. Asked
     * once per attacker per fight.
     */
    default float fightStrength(net.minecraft.world.entity.Entity attacker) {
        return Float.NaN;
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
