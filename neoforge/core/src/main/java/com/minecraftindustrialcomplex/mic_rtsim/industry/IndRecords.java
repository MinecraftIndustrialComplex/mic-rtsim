package com.minecraftindustrialcomplex.mic_rtsim.industry;

/**
 * The industry records of {@code docs/industry-boundary.md}: kinds, offsets and codes. The Java mirror
 * of {@code crates/rtsim-industry/src/wire.rs} (the reference codec, with round-trip tests); the two
 * and the page change together. Offsets are from the record start, header included.
 *
 * <p><b>Kind ranges</b> (F1), reserved in the bridge's kind space:
 * <ul>
 *   <li>0x0040 to 0x005F: Java to engine, latest-wins (state; may be dropped when a buffer is full);</li>
 *   <li>0x0400 to 0x04FF: Java to engine, journaled (events; never dropped);</li>
 *   <li>0x8400 to 0x84FF: engine to Java, commands.</li>
 * </ul>
 * Latest-wins records carry {@code i64 gameTime} at 8; journaled ones the bridge's prefix
 * ({@code u64 obsSeq} at 8, {@code i64 gameTime} at 16); commands {@code u64 cmdSeq} at 8.
 */
public final class IndRecords {
    private IndRecords() {}

    // ---- ranges -----------------------------------------------------------------------------------
    public static final int LATEST_FIRST = 0x0040;
    public static final int LATEST_LAST = 0x005F;
    public static final int JOURNALED_FIRST = 0x0400;
    public static final int JOURNALED_LAST = 0x04FF;
    public static final int COMMAND_FIRST = 0x8400;
    public static final int COMMAND_LAST = 0x84FF;

    public static boolean isIndustryCommand(int kind) {
        return kind >= COMMAND_FIRST && kind <= COMMAND_LAST;
    }

    // ---- dictionaries (F2) ------------------------------------------------------------------------
    /** {@code <class>/<max stack>/<registry id>[/<component hash>]}. */
    public static final int DICT_GOOD = 0x10;
    /** UTF-8 text: station names, package addresses, district names. */
    public static final int DICT_NAME = 0x11;
    /** A block state's string form, {@code minecraft:oak_log[axis=y]}. */
    public static final int DICT_BLOCK_STATE = 0x12;

    /** One goods-list entry: u32 good, u32 reserved, i64 amount. */
    public static final int ENTRY16 = 16;
    /** Result24: u32 inventory (or carriage), u32 good, i64 requested, i64 applied. */
    public static final int RESULT24 = 24;
    /** Pos16: i32 x, i32 y, i32 z, u32 reserved. */
    public static final int POS16 = 16;

    /** The inventory index of a finite deposit in a reconcile (the engine's {@code DEPOSIT_INVENTORY}). */
    public static final int DEPOSIT_INVENTORY = 0xFFFF_FFFF;
    /** No inventory (IND_PORT). */
    public static final int NO_INVENTORY = 0xFFFF_FFFF;

    // ---- latest-wins observations -----------------------------------------------------------------
    public static final int IND_STOCK_SAMPLE = 0x0040;
    public static final int IND_SWEEP_END = 0x0041;
    public static final int IND_DEPOSIT_SAMPLE = 0x0042;
    public static final int IND_CHANNEL_SAMPLE = 0x0043;
    public static final int IND_LEDGER_REQUEST = 0x0044;
    public static final int IND_WATCHED = 0x0045;
    /**
     * Java-side proposal (choice H23), not yet in wire.rs: a player approaches an unloaded district.
     * The engine skips it as an unknown kind until it reads it (the stale-tick race, option d).
     */
    public static final int IND_APPROACH = 0x0046;

    // ---- journaled observations -------------------------------------------------------------------
    public static final int IND_DISTRICT = 0x0400;
    public static final int IND_DISTRICT_CHUNKS = 0x0401;
    public static final int IND_INVENTORY = 0x0402;
    public static final int IND_DISTRICT_REMOVED = 0x0403;
    public static final int IND_EDIT = 0x0404;
    public static final int IND_DISCARD = 0x0405;
    public static final int IND_LOAD_STATE = 0x0406;
    public static final int IND_CHUNK_LOADED = 0x0407;
    public static final int IND_CONDITIONS = 0x0408;
    public static final int IND_POWER_LINK = 0x0409;
    public static final int IND_PORT = 0x0410;
    public static final int IND_TRAIN_ARRIVED = 0x0411;
    public static final int IND_TRAIN_DEPARTED = 0x0412;
    public static final int IND_STATION = 0x0413;
    public static final int IND_PACKAGE = 0x0414;
    public static final int IND_RECONCILE_RESULT = 0x0420;
    public static final int IND_CARRIAGE_RESULT = 0x0421;
    public static final int IND_SPAWN_SAMPLE = 0x0430;
    public static final int IND_FIGHT = 0x0431;
    public static final int IND_SURVEY = 0x0432;
    public static final int IND_SURVEY_BLOCKS = 0x0433;

    // ---- commands ---------------------------------------------------------------------------------
    public static final int IND_CHUNK_RECONCILE = 0x8400;
    public static final int IND_CARRIAGE_DELTA = 0x8401;
    public static final int IND_CHANNEL_DELTA = 0x8402;
    public static final int IND_DEPOSIT_DELTA = 0x8403;
    public static final int IND_WRITE_PACKAGE = 0x8404;
    public static final int IND_LEDGER = 0x8405;

    // ---- latest-wins layouts ----------------------------------------------------------------------

    /** IND_STOCK_SAMPLE, aux = n entries. */
    public static final class StockSample {
        public static final int GAME_TIME = 8;  // i64
        public static final int DISTRICT = 16;  // u64
        public static final int INVENTORY = 24; // u32
        public static final int SWEEP = 28;     // u32
        public static final int ENTRIES = 32;   // n x Entry16
        /** The most entries one record holds (65,528 bytes at most). */
        public static final int MAX_ENTRIES = (0xFFF8 - ENTRIES) / ENTRY16;

        public static int size(int n) {
            return ENTRIES + n * ENTRY16;
        }
    }

    /** IND_SWEEP_END, aux = flags. */
    public static final class SweepEnd {
        public static final int GAME_TIME = 8;    // i64
        public static final int DISTRICT = 16;    // u64
        public static final int SWEEP = 24;       // u32
        public static final int RECORDS = 28;     // u32
        public static final int INVENTORIES = 32; // u32
        // 36: u32 reserved
        public static final int SIZE = 40;

        public static final int FLAG_LIVE = 1;
        public static final int FLAG_CREATIVE = 2;
        public static final int FLAG_OVERSTRESSED = 4;
    }

    /** IND_LEDGER_REQUEST and IND_WATCHED (aux 1 watched, 0 not). */
    public static final class DistrictState {
        public static final int GAME_TIME = 8; // i64
        public static final int DISTRICT = 16; // u64
        public static final int SIZE = 24;
    }

    /** IND_APPROACH (Java-side proposal, H23), aux = flags (1: the player is closing in). */
    public static final class Approach {
        public static final int GAME_TIME = 8;  // i64
        public static final int DISTRICT = 16;  // u64
        public static final int DISTANCE = 24;  // u32, blocks from the player to the district's nearest chunk edge
        public static final int ETA_TICKS = 28; // u32, distance / the player's speed toward it (0xFFFFFFFF unknown)
        public static final int SIZE = 32;

        public static final int FLAG_CLOSING = 1;
    }

    // ---- journaled layouts (bodies from 24) -------------------------------------------------------

    /** IND_DISTRICT, aux = not-learnable reason (0 learnable). */
    public static final class District {
        public static final int DISTRICT = 24;        // u64
        public static final int REVISION = 32;        // u32
        public static final int DIM = 36;             // u32 (bridge dimension dictionary)
        public static final int NAME = 40;            // u32 (NAME dictionary)
        public static final int INVENTORY_COUNT = 44; // u32
        public static final int SIZE = 48;
    }

    /** IND_DISTRICT_CHUNKS, aux = n chunks. */
    public static final class DistrictChunks {
        public static final int DISTRICT = 24; // u64
        public static final int REVISION = 32; // u32
        public static final int PART = 36;     // u32
        public static final int CHUNKS = 40;   // n x (i32 x, i32 z)
        public static final int MAX_CHUNKS = (0xFFF8 - CHUNKS) / 8;

        public static int size(int n) {
            return CHUNKS + 8 * n;
        }
    }

    /** IND_INVENTORY, aux = kind flags. */
    public static final class Inventory {
        public static final int DISTRICT = 24;        // u64
        public static final int REVISION = 32;        // u32
        public static final int INDEX = 36;           // u32
        public static final int X = 40;               // i32
        public static final int Y = 44;               // i32
        public static final int Z = 48;               // i32
        public static final int ITEM_SLOTS = 52;      // u32
        public static final int FLUID_CAPACITY = 56;  // i64
        public static final int ENERGY_CAPACITY = 64; // i64
        public static final int SIZE = 72;

        public static final int KIND_ITEMS = 1;
        public static final int KIND_FLUIDS = 2;
        public static final int KIND_ENERGY = 4;
        public static final int KIND_POSTBOX = 8;
        public static final int KIND_PACKAGER_TARGET = 16;
        public static final int KIND_PLUGIN = 32;
    }

    /** IND_DISTRICT_REMOVED. */
    public static final class DistrictRemoved {
        public static final int DISTRICT = 24; // u64
        public static final int SIZE = 32;
    }

    /** IND_EDIT, aux = 1 placed, 2 broken, 3 configured, 4 command. */
    public static final class Edit {
        public static final int DISTRICT = 24;     // u64
        public static final int PLAYER_MOST = 32;  // u64
        public static final int PLAYER_LEAST = 40; // u64
        public static final int X = 48;            // i32
        public static final int Y = 52;            // i32
        public static final int Z = 56;            // i32
        // 60: u32 reserved
        public static final int SIZE = 64;

        public static final int PLACED = 1;
        public static final int BROKEN = 2;
        public static final int CONFIGURED = 3;
        public static final int COMMAND = 4;
    }

    /** IND_DISCARD, aux = reason. */
    public static final class Discard {
        public static final int DISTRICT = 24; // u64
        public static final int SIZE = 32;

        public static final int UNATTRIBUTED_SESSION = 1;
        public static final int CREATIVE_BLOCK = 2;
        public static final int OVERSTRESSED = 3;
        public static final int PROGRESSIVE_CONTRAPTION = 4;
        public static final int OTHER = 5;
    }

    /** IND_LOAD_STATE, aux = 0 unloaded, 1 partial, 2 live. */
    public static final class LoadState {
        public static final int DISTRICT = 24;        // u64
        public static final int TICKING_CHUNKS = 32;  // u32
        public static final int TOTAL_CHUNKS = 36;    // u32
        public static final int SIZE = 40;

        public static final int UNLOADED = 0;
        public static final int PARTIAL = 1;
        public static final int LIVE = 2;
    }

    /** IND_CHUNK_LOADED. */
    public static final class ChunkLoaded {
        public static final int DISTRICT = 24;      // u64
        public static final int DIM = 32;           // u32
        public static final int X = 36;             // i32
        public static final int Z = 40;             // i32
        public static final int APPLIED_EPOCH = 44; // u32
        public static final int SIZE = 48;
    }

    /** IND_PORT, aux = n entries. Amounts are + into the district. */
    public static final class Port {
        public static final int DISTRICT = 24;    // u64
        public static final int PORT_KIND = 32;   // u32
        public static final int INVENTORY = 36;   // u32, NO_INVENTORY for none
        public static final int COUNTERPART = 40; // u64
        public static final int OP_ID = 48;       // u64, 0 none
        public static final int ENTRIES = 56;     // n x Entry16
        public static final int MAX_ENTRIES = (0xFFF8 - ENTRIES) / ENTRY16;

        public static int size(int n) {
            return ENTRIES + n * ENTRY16;
        }

        public static final int TRAIN_CARGO = 1;
        public static final int PACKAGE = 2;
        public static final int PLAYER_SESSION = 3;
        public static final int PLAYER_TOSS = 4;
        public static final int PLAYER_PICKUP = 5;
        public static final int STOCK_BROKEN = 6;
        public static final int CHANNEL = 7;
        public static final int OTHER = 9;
    }

    /**
     * IND_TRAIN_ARRIVED (aux = n carriages; 56 carriage blocks) and IND_TRAIN_DEPARTED (aux = n
     * carriages; 48 u64 destination, 56 i64 etaTicks, 64 carriage blocks).
     */
    public static final class Train {
        public static final int TRAIN = 24;    // u64
        public static final int STATION = 32;  // u64
        public static final int DISTRICT = 40; // u64
        public static final int ARRIVED_FLAGS = 48;     // u32 (1 simulated)
        public static final int ARRIVED_CARRIAGES = 56;
        public static final int DEPARTED_DESTINATION = 48; // u64, 0 none
        public static final int DEPARTED_ETA = 56;         // i64
        public static final int DEPARTED_CARRIAGES = 64;

        public static final int FLAG_SIMULATED = 1;

        /** Carriage block: u32 index, u32 itemSlots, i64 fluidCapacity, u32 entryCount, u32 reserved, entries. */
        public static final int C_INDEX = 0;
        public static final int C_ITEM_SLOTS = 4;
        public static final int C_FLUID_CAPACITY = 8;
        public static final int C_ENTRY_COUNT = 16;
        public static final int C_ENTRIES = 24;

        public static int carriageSize(int entries) {
            return C_ENTRIES + entries * ENTRY16;
        }
    }

    /** IND_STATION. */
    public static final class Station {
        public static final int STATION = 24;  // u64
        public static final int DISTRICT = 32; // u64, 0 none
        public static final int DIM = 40;      // u32
        public static final int X = 44;        // i32
        public static final int Y = 48;        // i32
        public static final int Z = 52;        // i32
        public static final int NAME = 56;     // u32 (NAME)
        // 60: u32 reserved
        public static final int SIZE = 64;
    }

    /** IND_PACKAGE, aux = n entries. */
    public static final class Package {
        public static final int DISTRICT = 24;  // u64
        public static final int INVENTORY = 32; // u32
        public static final int DIRECTION = 36; // u32: 1 in, 2 out
        public static final int ADDRESS = 40;   // u32 (NAME)
        public static final int PACKAGES = 44;  // u32
        public static final int OP_ID = 48;     // u64, 0 none
        public static final int ENTRIES = 56;

        public static int size(int n) {
            return ENTRIES + n * ENTRY16;
        }

        public static final int IN = 1;
        public static final int OUT = 2;
    }

    /** IND_RECONCILE_RESULT, aux = n entries; then m Pos16 of the blocks broken. */
    public static final class ReconcileResult {
        public static final int OP_ID = 24;      // u64
        public static final int DISTRICT = 32;   // u64
        public static final int EPOCH = 40;      // u32
        public static final int DIM = 44;        // u32
        public static final int CHUNK_X = 48;    // i32
        public static final int CHUNK_Z = 52;    // i32
        public static final int DESTROYED = 56;  // u32 m
        public static final int SKIPPED = 60;    // u32
        public static final int ENTRIES = 64;    // n x Result24, then m x Pos16

        public static int size(int n, int m) {
            return ENTRIES + n * RESULT24 + m * POS16;
        }
    }

    /** IND_CARRIAGE_RESULT, aux = n entries (Result24, first field: carriage). */
    public static final class CarriageResult {
        public static final int OP_ID = 24;  // u64
        public static final int TRAIN = 32;  // u64
        public static final int ENTRIES = 40;

        public static int size(int n) {
            return ENTRIES + n * RESULT24;
        }
    }

    // ---- commands (bodies from 16) ----------------------------------------------------------------

    /** IND_CHUNK_RECONCILE, aux = n deltas; then destroyCount x Destroy16. */
    public static final class ChunkReconcile {
        public static final int OP_ID = 16;         // u64
        public static final int DISTRICT = 24;      // u64
        public static final int EPOCH = 32;         // u32
        public static final int DIM = 36;           // u32 (engine dimension dictionary)
        public static final int CHUNK_X = 40;       // i32
        public static final int CHUNK_Z = 44;       // i32
        public static final int DESTROY_COUNT = 48; // u32
        // 52: u32 reserved
        public static final int DELTAS = 56;

        /** Delta32: u32 inventory, u32 good, i32 x, i32 y, i32 z, u32 reserved, i64 delta. */
        public static final int D_INVENTORY = 0;
        public static final int D_GOOD = 4;
        public static final int D_X = 8;
        public static final int D_Y = 12;
        public static final int D_Z = 16;
        public static final int D_DELTA = 24;
        public static final int DELTA_SIZE = 32;

        /** Destroy16: i32 x, i32 y, i32 z, u32 expectedState (BLOCK_STATE). */
        public static final int B_X = 0;
        public static final int B_Y = 4;
        public static final int B_Z = 8;
        public static final int B_STATE = 12;
        public static final int DESTROY_SIZE = 16;

        public static int size(int deltas, int destroys) {
            return DELTAS + deltas * DELTA_SIZE + destroys * DESTROY_SIZE;
        }
    }

    /** IND_CARRIAGE_DELTA, aux = n entries of (u32 carriage, u32 good, i64 delta into the carriage). */
    public static final class CarriageDelta {
        public static final int OP_ID = 16;  // u64
        public static final int TRAIN = 24;  // u64
        public static final int REASON = 32; // u32: 1 exchange, 2 loot
        // 36: u32 reserved
        public static final int ENTRIES = 40;
        public static final int E_CARRIAGE = 0;
        public static final int E_GOOD = 4;
        public static final int E_DELTA = 8;
        public static final int ENTRY = 16;

        public static int size(int n) {
            return ENTRIES + n * ENTRY;
        }

        public static final int EXCHANGE = 1;
        public static final int LOOT = 2;
    }

    /** IND_CHANNEL_DELTA, aux = n entries. */
    public static final class ChannelDelta {
        public static final int OP_ID = 16;   // u64
        public static final int CHANNEL = 24; // u64
        public static final int ENTRIES = 32;
    }

    /** IND_DEPOSIT_DELTA, aux = plugin. */
    public static final class DepositDelta {
        public static final int OP_ID = 16;   // u64
        public static final int DIM = 24;     // u32
        public static final int CHUNK_X = 28; // i32
        public static final int CHUNK_Z = 32; // i32
        public static final int GOOD = 36;    // u32
        public static final int DELTA = 40;   // i64
        public static final int SIZE = 48;
    }

    /** IND_WRITE_PACKAGE, aux = n entries (one package's contents). */
    public static final class WritePackage {
        public static final int OP_ID = 16;     // u64
        public static final int DISTRICT = 24;  // u64
        public static final int INVENTORY = 32; // u32
        public static final int X = 36;         // i32
        public static final int Y = 40;         // i32
        public static final int Z = 44;         // i32
        public static final int ADDRESS = 48;   // u32 (NAME)
        public static final int COUNT = 52;     // u32, packages of these contents
        public static final int ENTRIES = 56;

        public static int size(int n) {
            return ENTRIES + n * ENTRY16;
        }
    }

    /** IND_LEDGER, aux = n goods of (u32 good, u32 reserved, i64 perHour, i64 sinceStart). */
    public static final class Ledger {
        public static final int DISTRICT = 16;         // u64
        public static final int STATE = 24;            // u32
        public static final int FREEZE = 28;           // u32
        public static final int NEXT_BREAKPOINT = 32;  // i64, -1 none
        public static final int NEXT_KIND = 40;        // u32
        public static final int NEXT_GOOD = 44;        // u32, 0 none
        public static final int SINCE = 48;           // i64
        public static final int GOODS = 56;
        public static final int G_GOOD = 0;
        public static final int G_PER_HOUR = 8;
        public static final int G_SINCE_START = 16;
        public static final int GOOD = 24;

        public static int size(int n) {
            return GOODS + n * GOOD;
        }

        public static final int STATE_LIVE = 0;
        public static final int STATE_LEARNING = 1;
        public static final int STATE_LEARNED = 2;
        public static final int STATE_STALE = 3;
        public static final int STATE_NOT_LEARNABLE = 4;
        public static final int STATE_SUSPENDED = 5;
        public static final int STATE_DAMAGED = 6;
        public static final int STATE_OUT_OF_SEASON = 7;

        public static final int NEXT_NONE = 0;
        public static final int NEXT_INPUT_EMPTIES = 1;
        public static final int NEXT_OUTPUT_FULL = 2;
        public static final int NEXT_TARGET = 3;
        public static final int NEXT_DEPOSIT_EXHAUSTED = 4;
        public static final int NEXT_SOURCE_STOPPED = 5;
        public static final int NEXT_RESTART = 6;
    }

    // ---- plugin and threat layouts (overnight agent 14, docs/overnight-choices-indplugins.md) ------

    /** IND_DEPOSIT_SAMPLE (latest-wins), aux = plugin ({@link DepositPlugin}). */
    public static final class DepositSample {
        public static final int GAME_TIME = 8;  // i64
        public static final int DISTRICT = 16;  // u64
        public static final int GOOD = 24;      // u32 (GOOD, class d)
        public static final int SWEEP = 28;     // u32
        public static final int REMAINING = 32; // i64
        public static final int DIM = 40;       // u32
        public static final int CHUNK_X = 44;   // i32
        public static final int CHUNK_Z = 48;   // i32
        public static final int FLAGS = 52;     // u32 (1 infinite)
        public static final int SIZE = 56;

        public static final int FLAG_INFINITE = 1;
    }

    /** The deposit plugin codes (IND_DEPOSIT_SAMPLE and IND_DEPOSIT_DELTA aux; wire.rs {@code plugin_code}). */
    public static final class DepositPlugin {
        public static final int ORE_EXCAVATION = 1;
        public static final int DIESEL_OIL = 2;
        public static final int DESTROY_CRUDE = 3;
    }

    /** IND_CHANNEL_SAMPLE (latest-wins), aux = n entries. */
    public static final class ChannelSample {
        public static final int GAME_TIME = 8; // i64
        public static final int CHANNEL = 16;  // u64
        public static final int CAPACITY = 24; // i64
        public static final int ENTRIES = 32;  // n x Entry16
        public static final int MAX_ENTRIES = (0xFFF8 - ENTRIES) / ENTRY16;

        public static int size(int n) {
            return ENTRIES + n * ENTRY16;
        }
    }

    /** IND_CONDITIONS (journaled). */
    public static final class Conditions {
        public static final int SEASON = 24;        // u32
        public static final int SEASON_COUNT = 28;  // u32
        public static final int SEASON_LENGTH = 32; // i64, ticks
        public static final int SEASON_ORIGIN = 40; // i64, the game time season 0 began
        public static final int SIZE = 48;
    }

    /** IND_POWER_LINK (journaled): {@code sink}'s loads fed from {@code source}'s generators; source 0 ends it. */
    public static final class PowerLink {
        public static final int SINK = 24;   // u64
        public static final int SOURCE = 32; // u64, 0 none
        public static final int SIZE = 40;
    }

    /** IND_SPAWN_SAMPLE (journaled). */
    public static final class SpawnSample {
        public static final int DISTRICT = 24;       // u64
        public static final int OBSERVED_TICKS = 32; // i64
        public static final int DARK_BLOCKS = 40;    // i64
        public static final int SPAWNS = 48;         // 4 x u32, by threat class
        public static final int SIZE = 64;
    }

    /** IND_FIGHT (journaled), aux = threat class. */
    public static final class Fight {
        public static final int DISTRICT = 24;           // u64
        public static final int ATTACKERS = 32;          // u32
        public static final int ATTACKER_STRENGTH = 36;  // f32
        public static final int ATTACKERS_KILLED = 40;   // u32
        public static final int BLOCKS_DESTROYED = 44;   // u32
        public static final int MACHINES_DESTROYED = 48; // u32
        public static final int GUARDS_LOST = 52;        // u32
        public static final int ITEMS_STOLEN = 56;       // i64
        public static final int DURATION_TICKS = 64;     // i64
        public static final int SIZE = 72;
    }

    /** IND_SURVEY (journaled), aux = flags (1 mobGriefing). */
    public static final class Survey {
        public static final int DISTRICT = 24;            // u64
        public static final int REVISION = 32;            // u32
        public static final int PERIMETER_COLUMNS = 36;   // u32
        public static final int PERIMETER_GAPS = 40;      // u32
        public static final int DARK_SPAWNABLE = 44;      // u32
        public static final int MACHINE_BLOCKS = 48;      // u32
        public static final int ROOFED_MACHINES = 52;     // u32
        public static final int KILL_MACHINES_AT_GAPS = 56; // u32
        public static final int AMMO_GOOD = 60;           // u32 (GOOD, 0 none)
        public static final int TURRET_STRENGTH = 64;     // 4 x f32, by threat class
        public static final int GUARDS = 80;              // 4 x u32, by threat class
        public static final int AMMO_PER_ENGAGEMENT = 96; // u32
        // 100: u32 reserved
        public static final int SIZE = 104;

        public static final int FLAG_MOB_GRIEFING = 1;
    }

    /** IND_SURVEY_BLOCKS (journaled), aux = n Block24. */
    public static final class SurveyBlocks {
        public static final int DISTRICT = 24; // u64
        public static final int REVISION = 32; // u32
        public static final int PART = 36;     // u32
        public static final int BLOCKS = 40;   // n x Block24
        /** Block24: i32 x, i32 y, i32 z, u32 state (BLOCK_STATE), u32 flags, u32 inventory (0xFFFFFFFF none). */
        public static final int B_X = 0;
        public static final int B_Y = 4;
        public static final int B_Z = 8;
        public static final int B_STATE = 12;
        public static final int B_FLAGS = 16;
        public static final int B_INVENTORY = 20;
        public static final int BLOCK = 24;
        public static final int MAX_BLOCKS = (0xFFF8 - BLOCKS) / BLOCK;

        public static int size(int n) {
            return BLOCKS + n * BLOCK;
        }

        public static final int FLAG_PERIMETER = 1;
        public static final int FLAG_ROOFLESS = 2;
        public static final int FLAG_MACHINE = 4;
        public static final int FLAG_INVENTORY = 8;
    }

    /** Threat classes (IND_FIGHT aux, the SPAWNS, TURRET_STRENGTH and GUARDS arrays; wire.rs {@code class_code}). */
    public static final class ThreatClass {
        public static final int GROUND_MELEE = 0;
        public static final int GROUND_RANGED = 1;
        public static final int FLYING = 2;
        public static final int SIEGE = 3;
        public static final int COUNT = 4;
    }

    // ---- codes ------------------------------------------------------------------------------------

    /** Not-learnable reasons (IND_DISTRICT aux), INDUSTRY.md §6; others show as "not learnable". */
    public static final class NotLearnable {
        public static final int ROTATION_CROSSES = 1;
        public static final int ELECTRIC_LINE_CROSSES = 2;
        public static final int MIXTURE_NOT_STEADY = 3;
        public static final int FINITE_DEPOSIT_UNKNOWN = 4;
        public static final int MOVING_CONTRAPTION = 5;
        public static final int SPAWN_FED_FARM = 6;
        /** Java-side code (choice H7): a belt, chute, pipe or conveyor crosses the boundary. */
        public static final int CONVEYOR_CROSSES = 7;
    }
}
