package com.minecraftindustrialcomplex.mic_rtsim.boundary;

/**
 * Observation records: Java writes them, the engine reads them. Kinds are below 0x8000.
 *
 * <p>Offsets are from the start of the record (its 8-byte header included). Types are given as
 * u32, i64, f64 and so on; all are little-endian. See {@code docs/boundary.md}.
 *
 * <p><b>Journaled kinds</b> (0x0100 to 0x7FFF) start with an observation sequence number and a game
 * time, are kept in the journal until the engine acknowledges them, and are replayed on the next
 * session if it never did (PLAN.md §4.9). The other kinds are latest-wins state: dropped when a
 * buffer is full, sent again at the next pass.
 */
public final class Obs {
    private Obs() {}

    // ---- kinds ----------------------------------------------------------------------------------
    public static final int SESSION_START = 0x0001;
    public static final int DICT = 0x0002;
    public static final int TICK = 0x0003;
    public static final int MODULES = 0x0004;
    public static final int WORLD_SAVED = 0x0005;
    public static final int PLAYER = 0x0010;
    public static final int PUPPET_STATE = 0x0011;
    /** Phase 1 (latest-wins): a loaded village's census. */
    public static final int VILLAGE_CENSUS = 0x0020;
    /** Phase 1 (latest-wins): vanilla's own gossip in a loaded village, read back on census. */
    public static final int VILLAGE_GOSSIP = 0x0021;
    /** Phase 1 (latest-wins): the works catalogue (SINK.md §4.2), sent once a session. */
    public static final int WORK_CATALOG = 0x0022;

    public static final int MATERIALIZED = 0x0100;
    public static final int SPAWN_REJECTED = 0x0101;
    public static final int DEMATERIALIZED = 0x0102;
    public static final int MEMBER_STATE = 0x0103;
    public static final int TAKEN = 0x0104;
    public static final int DIED = 0x0105;
    public static final int TRANSFER = 0x0110;
    public static final int COMMAND_RESULT = 0x0111;

    // Phase 1: caravans and markets (PLAN.md §8). All journaled.
    public static final int VILLAGE_ADOPTED = 0x0120;
    public static final int TRADE = 0x0121;
    public static final int HURT = 0x0122;
    public static final int RESCUE = 0x0123;
    public static final int CARGO_CHANGED = 0x0124;
    public static final int BASE = 0x0125;
    public static final int HOUSE_PLACED = 0x0126;

    // Generations and copies (docs/boundary.md §3.2). Journaled.
    /** A session's generation: the first record each session journals. */
    public static final int JOURNAL_GEN = 0x0130;
    /** A copy of a record a chunk or entity carried, journaled again when its holder loads. */
    public static final int REASSERT = 0x0131;

    public static boolean isJournaled(int kind) {
        return kind >= 0x0100 && kind < 0x8000;
    }

    // ---- prefix of every journaled record ---------------------------------------------------------
    /** u64. Observation sequence number: unique and increasing per world, never reused. */
    public static final int OBS_SEQ = 8;
    /** i64. Game time when it happened (journaled records may be sent ticks or sessions later). */
    public static final int GAME_TIME = 16;
    /** First byte of a journaled record's body. */
    public static final int BODY = 24;

    // ---- dictionary kinds (DICT aux, in both directions) ---------------------------------------
    public static final int DICT_DIMENSION = 1;
    public static final int DICT_ITEM = 2;
    public static final int DICT_BODY = 3;
    /** A structure id, e.g. {@code minecraft:village_plains}, {@code ribbits:ribbit_village}. */
    public static final int DICT_STRUCTURE = 4;
    /** A culture name (data, PLAN.md §8.4), e.g. {@code human}, {@code ribbit}: the names rtsim-settle uses. */
    public static final int DICT_CULTURE = 5;
    /** A resident's profession, e.g. {@code minecraft:farmer}, {@code ribbits:merchant}. */
    public static final int DICT_PROFESSION = 6;
    /** A structure template, e.g. {@code ribbits:houses/small_house_red_1}. */
    public static final int DICT_TEMPLATE = 7;
    /** A structure processor list, e.g. {@code ribbits:main}. */
    public static final int DICT_PROCESSORS = 8;
    /** A works catalogue kind (SINK.md §4.2), e.g. {@code mic_rtsim:plains_lamp}. */
    public static final int DICT_WORK = 9;

    /**
     * SESSION_START. The first record of a session's first hand-over. The two journal fields are
     * what the kernel's {@code Replay::reconcile} compares with its pages: the first sequence still
     * in the journal, and the next sequence the world save would have assigned, before Java moved it
     * past the pages.
     */
    public static final class SessionStart {
        public static final int GAME_TIME = 8;              // i64
        public static final int LAST_APPLIED_CMD_SEQ = 16;  // u64
        public static final int JOURNAL_FIRST_OBS_SEQ = 24; // u64, 0 when the journal is empty
        public static final int WORLD_NEXT_OBS_SEQ = 32;    // u64, as loaded from the world save
        public static final int MODULE_MASK = 40;           // u32
        // 44: u32 reserved
        public static final int SIZE = 48;
    }

    /**
     * DICT, aux = dictionary kind. Java's dictionary: ids are per world, persisted in the journal, and
     * the whole dictionary is sent again at every session start. An entry always precedes, in the same
     * or an earlier hand-over, every record that uses its id.
     */
    public static final class Dict {
        public static final int ID = 8;        // u32, from 1
        public static final int NAME_LEN = 12; // u32, bytes of UTF-8
        public static final int NAME = 16;     // UTF-8, zero-padded to 8

        public static int size(int nameBytes) {
            return Layout.align(NAME + nameBytes);
        }
    }

    /** TICK. Starts the latest-wins records of one tick. */
    public static final class Tick {
        public static final int GAME_TIME = 8; // i64
        public static final int SIZE = 16;
    }

    /** MODULES. The module switches (bit = {@code Module.ordinal()}); sent at start and on change. */
    public static final class Modules {
        public static final int MASK = 8; // u32
        // 12: u32 reserved
        public static final int SIZE = 16;
    }

    /**
     * WORLD_SAVED. A world save is on disk: every observation up to {@code DURABLE_OBS_SEQ} is in it
     * (as a journal entry, or trimmed after the engine acknowledged it). The engine may forget op ids
     * applied at or before it (the kernel's {@code Replay::forget_ops_through}). Sent once per save.
     */
    public static final class WorldSaved {
        public static final int DURABLE_OBS_SEQ = 8; // u64
        public static final int SIZE = 16;
    }

    /** PLAYER, aux = dimension (dictionary id). Every 20 ticks per online player (PLAN.md §6.3). */
    public static final class Player {
        public static final int UUID_MOST = 8;  // u64
        public static final int UUID_LEAST = 16; // u64
        public static final int X = 24;          // f64
        public static final int Y = 32;          // f64
        public static final int Z = 40;          // f64
        public static final int SIZE = 48;
    }

    /** PUPPET_STATE, aux = member index. The engine's shadow of an embodied member. */
    public static final class PuppetState {
        public static final int AGENT = 8;   // u64
        public static final int EPOCH = 16;  // u32
        public static final int DIM = 20;    // u32 dictionary id
        public static final int HEALTH = 24; // f32
        public static final int FLAGS = 28;  // u32, MemberState flags
        public static final int X = 32;      // f64
        public static final int Y = 40;      // f64
        public static final int Z = 48;      // f64
        public static final int SIZE = 56;
    }

    /** MATERIALIZED (journaled), aux = member count. A SPAWN_GROUP was applied. */
    public static final class Materialized {
        public static final int AGENT = 24; // u64
        public static final int EPOCH = 32; // u32
        public static final int DIM = 36;   // u32
        public static final int SIZE = 40;
    }

    /** SPAWN_REJECTED (journaled), aux = reason. A SPAWN_GROUP was not applied; the agent stays abstract. */
    public static final class SpawnRejected {
        public static final int AGENT = 24;   // u64
        public static final int EPOCH = 32;   // u32
        // 36: u32 reserved
        public static final int CMD_SEQ = 40; // u64
        public static final int SIZE = 48;

        public static final int NOT_TICKING = 1;
        public static final int UNKNOWN_KIND = 2;
        public static final int UNKNOWN_DIMENSION = 3;
        public static final int ALREADY_EMBODIED = 4;
        public static final int SPAWN_FAILED = 5;
        public static final int BAD_RECORD = 6;
        /** The server would discard the body on its first tick ({@code spawn-animals} or {@code spawn-npcs} off). */
        public static final int DISCARDED_BY_SERVER = 7;
        /** A cargo stack names no item, or does not fit the body (stack size, chest size). */
        public static final int BAD_CARGO = 8;
    }

    /**
     * DEMATERIALIZED (journaled), aux = reason. A group was captured and removed. Its members'
     * MEMBER_STATE records come right before it.
     */
    public static final class Dematerialized {
        public static final int AGENT = 24;   // u64
        public static final int EPOCH = 32;   // u32
        public static final int MEMBERS = 36; // u32, MEMBER_STATE records before this one
        public static final int SIZE = 40;

        public static final int COMMAND = 1;
        public static final int NOT_TICKING = 2;
        public static final int UNLOADED = 3;
        public static final int SESSION_END = 4;
        public static final int MEMBERS_GONE = 5;
        /** The answer to a DESPAWN_GROUP for an agent or epoch that is not embodied (MEMBERS = 0). */
        public static final int NOT_EMBODIED = 6;
    }

    /**
     * MEMBER_STATE (journaled), aux = member index. What a body adapter captured: health, position,
     * body bits and the chest's cargo. Always followed by the event that captured it (DEMATERIALIZED,
     * TAKEN or DIED).
     */
    public static final class MemberState {
        public static final int AGENT = 24;      // u64
        public static final int EPOCH = 32;      // u32
        public static final int BODY_KIND = 36;  // u32 dictionary id (the member's body kind)
        public static final int DIM = 40;        // u32 dictionary id
        public static final int FLAGS = 44;      // u32, below
        public static final int HEALTH = 48;     // f32
        public static final int MAX_HEALTH = 52; // f32
        public static final int X = 56;          // f64
        public static final int Y = 64;          // f64
        public static final int Z = 72;          // f64
        public static final int BODY_A = 80;     // i32, adapter-defined (llama: variant)
        public static final int BODY_B = 84;     // i32, adapter-defined (llama: strength)
        public static final int ITEM_COUNT = 88; // u32
        // 92: u32 reserved
        public static final int ITEMS = 96;      // ITEM_COUNT x Item

        public static int size(int items) {
            return ITEMS + items * Item.SIZE;
        }

        public static final int FLAG_LEASHED_TO_LEADER = 1;
        public static final int FLAG_HAS_CHEST = 2;
        public static final int FLAG_TAMED = 4;
        public static final int FLAG_LEASHED_ELSEWHERE = 8;
        public static final int FLAG_TICKING = 16;
    }

    /** One item stack in a MEMBER_STATE or SPAWN_GROUP member block. 16 bytes. */
    public static final class Item {
        public static final int SLOT = 0;  // u32, adapter slot (llama: chest slot from 0)
        public static final int ITEM = 4;  // u32, dictionary id of the writer's direction
        public static final int COUNT = 8; // i32
        // 12: u32 reserved
        public static final int SIZE = 16;
    }

    /** TAKEN (journaled), aux = how. A member left the sim (A10, PLAN.md §6.2); its MEMBER_STATE precedes it. */
    public static final class Taken {
        public static final int AGENT = 24;        // u64
        public static final int EPOCH = 32;        // u32
        public static final int MEMBER = 36;       // u32
        public static final int PLAYER_MOST = 40;  // u64, 0 when no player
        public static final int PLAYER_LEAST = 48; // u64
        public static final int OP_ID = 56;        // u64, the cargo's cross-store transfer
        public static final int SIZE = 64;

        public static final int LEAD = 1;
        public static final int TAME = 2;
        public static final int PICKUP = 3;
        public static final int REMOVED_BY_OTHER = 4;
        public static final int PLUGIN = 5;
    }

    /** DIED (journaled), aux = member index. Deaths are always kept. Its MEMBER_STATE (cargo dropped) precedes it. */
    public static final class Died {
        public static final int AGENT = 24;        // u64
        public static final int EPOCH = 32;        // u32
        // 36: u32 reserved
        public static final int KILLER_MOST = 40;  // u64, 0 when none
        public static final int KILLER_LEAST = 48; // u64
        public static final int OP_ID = 56;        // u64, the dropped cargo's cross-store transfer
        public static final int SIZE = 64;
    }

    /** TRANSFER (journaled), aux = transfer kind. A cross-store transfer seen on the Minecraft side. */
    public static final class Transfer {
        public static final int OP_ID = 24;  // u64
        public static final int FROM = 32;   // u64, a store reference (kind-defined)
        public static final int TO = 40;     // u64
        public static final int ITEM = 48;   // u32 dictionary id
        // 52: u32 reserved
        public static final int AMOUNT = 56; // i64
        public static final int SIZE = 64;

        public static final int GENERIC = 1;
    }

    /**
     * JOURNAL_GEN (journaled), aux 0. The session's generation: a random nonzero nonce, journaled right
     * after the journal moved past the engine's pages, so before anything else the session journals. A
     * holder keeps each record it carries with the nonce of the session that journaled it.
     */
    public static final class JournalGen {
        public static final int GEN = 24; // u64, never 0
        public static final int SIZE = 32;
    }

    /**
     * REASSERT (journaled), aux 0, game time the inner record's. A copy of a record a chunk or entity
     * carried, journaled again when its holder loads: the generation it was first journaled in, then the
     * record whole as first journaled (its own sequence and op id). The engine applies the inner record,
     * under this record's sequence, only when its state lacks it: it judges the copy by the generation's
     * last sequence, never by an op id it may have forgotten.
     */
    public static final class Reassert {
        public static final int GEN = 24;   // u64, the inner record's generation (0: none, always new)
        public static final int INNER = 32; // the record as first journaled, to the end
        public static final int SIZE = 32;  // the smallest; the inner record makes it larger
    }

    /** COMMAND_RESULT (journaled), aux = status. The outcome of a command that carries an op id. */
    public static final class CommandResult {
        public static final int CMD_SEQ = 24; // u64
        public static final int OP_ID = 32;   // u64
        public static final int SIZE = 40;

        public static final int APPLIED = 1;
        public static final int DUPLICATE_OP = 2;
        public static final int FAILED = 3;
        /** PLACE_HOUSE: no free plot (player-placed blocks, block entities, other buildings, terrain). */
        public static final int NOT_FREE = 4;
        /** WRITE_GOSSIP refused: vanilla already records this act first-hand (PLAN.md §8.7). */
        public static final int VANILLA_KNOWS = 5;
        /**
         * TRANSFER_TO_CONTAINER: the container's chunk is not loaded, so whether it already holds the op
         * (its carried COMMAND_RESULT) cannot be read. Nothing was put; the engine keeps the op and sends it
         * again under the same id (RP-210).
         */
        public static final int NOT_LOADED = 6;
    }

    // ---- phase 1 --------------------------------------------------------------------------------

    /**
     * VILLAGE_ADOPTED (journaled), aux = flags. A village joined the sim (C22): its structure start
     * generated (or, flag LATE, a chunk holding a start the bridge had not seen loaded). The village id
     * is the packed chunk position of the start ({@link #villageId}), so a repeat after a crash names
     * the same village and the engine treats it as a no-op.
     */
    public static final class VillageAdopted {
        public static final int VILLAGE = 24;   // u64
        public static final int DIM = 32;       // u32 dictionary
        public static final int STRUCTURE = 36; // u32 dictionary (DICT_STRUCTURE)
        public static final int CULTURE = 40;   // u32 dictionary (DICT_CULTURE)
        public static final int PIECES = 44;    // u32
        public static final int MIN_X = 48;     // i32, the start's bounding box
        public static final int MIN_Y = 52;
        public static final int MIN_Z = 56;
        public static final int MAX_X = 60;
        public static final int MAX_Y = 64;
        public static final int MAX_Z = 68;
        public static final int MEET_X = 72;    // i32, the meeting point: the start piece's centre until a census finds a meeting POI
        public static final int MEET_Y = 76;
        public static final int MEET_Z = 80;
        // 84: u32 reserved
        public static final int SIZE = 88;

        public static final int FLAG_LATE = 1;
    }

    /**
     * A village's id: its dimension's Java dictionary id (16 bits) over the start chunk's x and z
     * (24 bits each, two's complement). The top two bits are always clear; base ids set bit 62.
     */
    public static long villageId(int dimId, int chunkX, int chunkZ) {
        return ((long) (dimId & 0x3FFF) << 48) | ((long) (chunkX & 0xFF_FFFF) << 24) | (chunkZ & 0xFF_FFFFL);
    }

    /** A player base's id: bit 62 set, then the dimension (14 bits) and the base cell's x and z (24 bits each). */
    public static long baseId(int dimId, int cellX, int cellZ) {
        return (1L << 62) | ((long) (dimId & 0x3FFF) << 48) | ((long) (cellX & 0xFF_FFFF) << 24) | (cellZ & 0xFF_FFFFL);
    }

    /**
     * TRADE (journaled), aux = side. A player traded with a merchant: an adopted village's resident (its
     * market's stock and treasury) or a puppet merchant (its cargo). The counts are what was really paid
     * and received, reputation and demand included. The op id makes the transfer exactly-once.
     */
    public static final class Trade {
        public static final int OP_ID = 24;            // u64
        public static final int NODE = 32;             // u64, the village or base it happened in; 0 = none
        public static final int AGENT = 40;            // u64, the puppet merchant's agent; 0 = a resident
        public static final int EPOCH = 48;            // u32
        public static final int MEMBER = 52;           // u32
        public static final int PLAYER_MOST = 56;      // u64
        public static final int PLAYER_LEAST = 64;     // u64
        public static final int MERCHANT_MOST = 72;    // u64, the merchant entity's UUID
        public static final int MERCHANT_LEAST = 80;   // u64
        public static final int COST_A_ITEM = 88;      // u32 dictionary
        public static final int COST_A_COUNT = 92;     // i32
        public static final int COST_B_ITEM = 96;      // u32 dictionary, 0 = none
        public static final int COST_B_COUNT = 100;    // i32
        public static final int RESULT_ITEM = 104;     // u32 dictionary
        public static final int RESULT_COUNT = 108;    // i32
        public static final int MERCHANT_BODY = 112;   // u32 dictionary (DICT_BODY): entity type, or puppet body kind
        public static final int PROFESSION = 116;      // u32 dictionary (DICT_PROFESSION), 0 = none
        public static final int SIZE = 120;

        /** The merchant sold goods for its currency: the player bought. */
        public static final int MERCHANT_SELLS = 1;
        /** The merchant bought goods for its currency: the player sold. */
        public static final int MERCHANT_BUYS = 2;
        /** Any other shape (two costs, enchanted results, barter). */
        public static final int OTHER = 3;
    }

    /**
     * HURT (journaled), aux = what. Damage to a puppet member or to a resident of an adopted village,
     * coalesced per (attacker, victim) over a window (choice G9): the amounts and hits are summed.
     */
    public static final class Hurt {
        public static final int AGENT = 24;          // u64, 0 for a resident
        public static final int EPOCH = 32;          // u32
        public static final int MEMBER = 36;         // u32
        public static final int VICTIM_MOST = 40;    // u64
        public static final int VICTIM_LEAST = 48;   // u64
        public static final int ATTACKER_MOST = 56;  // u64, 0 = none
        public static final int ATTACKER_LEAST = 64; // u64
        public static final int ATTACKER_KIND = 72;  // u32, below
        public static final int AMOUNT = 76;         // f32
        public static final int NODE = 80;           // u64, the resident's village; 0 = none
        public static final int VICTIM_BODY = 88;    // u32 dictionary (DICT_BODY)
        public static final int FLAGS = 92;          // u32, below
        public static final int HITS = 96;           // u32
        public static final int PROFESSION = 100;    // u32 dictionary, 0 = none
        public static final int SIZE = 104;

        public static final int PUPPET_HURT = 1;
        public static final int RESIDENT_HURT = 2;
        public static final int RESIDENT_KILLED = 3;

        public static final int ATTACKER_NONE = 0;
        public static final int ATTACKER_PLAYER = 1;
        public static final int ATTACKER_MOB = 2;
        public static final int ATTACKER_OTHER = 3;

        /** Vanilla already wrote this as gossip (a villager hurt or killed by a player): the sim must not echo it. */
        public static final int FLAG_VANILLA_GOSSIP = 1;
        public static final int FLAG_LETHAL = 2;
    }

    /**
     * RESCUE (journaled), aux = 1 puppet, 2 resident. A player killed a mob whose target was a puppet
     * member or a village resident (C24). Journaled at the end of the tick, once the mob is really dead.
     */
    public static final class Rescue {
        public static final int AGENT = 24;          // u64, 0 for a resident
        public static final int EPOCH = 32;          // u32
        public static final int MEMBER = 36;         // u32
        public static final int RESCUER_MOST = 40;   // u64
        public static final int RESCUER_LEAST = 48;  // u64
        public static final int VICTIM_MOST = 56;    // u64, the entity the mob was targeting
        public static final int VICTIM_LEAST = 64;   // u64
        public static final int THREAT = 72;         // u32 dictionary (DICT_BODY): the mob's entity type
        // 76: u32 reserved
        public static final int NODE = 80;           // u64, 0 = none
        public static final int SIZE = 88;

        public static final int PUPPET = 1;
        public static final int RESIDENT = 2;
    }

    /**
     * CARGO_CHANGED (journaled), aux = member index. A puppet's cargo changed by something other than
     * the bridge (robbery, C24 and PLAN.md §8.7): signed deltas per item, negative = taken out.
     */
    public static final class CargoChanged {
        public static final int OP_ID = 24;        // u64
        public static final int AGENT = 32;        // u64
        public static final int EPOCH = 40;        // u32
        public static final int FLAGS = 44;        // u32, below
        public static final int PLAYER_MOST = 48;  // u64, 0 = nobody known
        public static final int PLAYER_LEAST = 56; // u64
        public static final int ITEM_COUNT = 64;   // u32
        // 68: u32 reserved
        public static final int ITEMS = 72;        // ITEM_COUNT x Item (slot 0xFFFFFFFF, count = signed delta)

        public static int size(int items) {
            return ITEMS + items * Item.SIZE;
        }

        /** The player is the nearest one, not one seen with the inventory open. */
        public static final int FLAG_BY_PROXIMITY = 1;
    }

    /**
     * BASE (journaled), aux = MARKED or UNMARKED. A player base as a market node (C26): a cell where a
     * player has spent much time (P30's recommendation, choice G14).
     */
    public static final class Base {
        public static final int BASE = 24;         // u64, {@link #baseId}
        public static final int DIM = 32;          // u32 dictionary
        // 36: u32 reserved
        public static final int X = 40;            // f64, where the player spent the time (mean position)
        public static final int Y = 48;
        public static final int Z = 56;
        public static final int PLAYER_MOST = 64;  // u64
        public static final int PLAYER_LEAST = 72; // u64
        public static final int DWELL = 80;        // f32, the decayed dwell counter, in samples
        // 84: u32 reserved
        public static final int SIZE = 88;

        public static final int MARKED = 1;
        public static final int UNMARKED = 2;
    }

    /** HOUSE_PLACED (journaled). A PLACE_HOUSE finished: where the house stands. COMMAND_RESULT follows it. */
    public static final class HousePlaced {
        public static final int OP_ID = 24;    // u64, the command's op id
        public static final int VILLAGE = 32;  // u64
        public static final int DIM = 40;      // u32 dictionary
        public static final int TEMPLATE = 44; // u32 dictionary (DICT_TEMPLATE)
        public static final int MIN_X = 48;    // i32, the house's bounding box
        public static final int MIN_Y = 52;
        public static final int MIN_Z = 56;
        public static final int MAX_X = 60;
        public static final int MAX_Y = 64;
        public static final int MAX_Z = 68;
        public static final int SLOT_X = 72;   // i32, where a resident of the house stands
        public static final int SLOT_Y = 76;
        public static final int SLOT_Z = 80;
        public static final int ROTATION = 84; // u8
        public static final int MIRROR = 85;   // u8
        public static final int SIZE = 88;
    }

    /** VILLAGE_CENSUS (latest-wins). A loaded village counted (PLAN.md §6.3), staggered. */
    public static final class VillageCensus {
        public static final int VILLAGE = 8;       // u64
        public static final int DIM = 16;          // u32 dictionary
        public static final int FLAGS = 20;        // u32, below
        public static final int MEET_X = 24;       // i32
        public static final int MEET_Y = 28;
        public static final int MEET_Z = 32;
        public static final int RESIDENTS = 36;    // u32
        public static final int BEDS = 40;         // u32
        public static final int WORKSTATIONS = 44; // u32
        public static final int GOLEMS = 48;       // u32
        public static final int PROFESSIONS = 52;  // u32
        public static final int ENTRIES = 56;      // PROFESSIONS x (u32 profession dictionary id, u32 count)
        public static final int ENTRY = 8;

        public static int size(int professions) {
            return ENTRIES + professions * ENTRY;
        }

        /** The meeting point is a meeting POI (a bell) the census found, not the start piece's centre. */
        public static final int FLAG_MEETING_POI = 1;
    }

    /**
     * WORK_CATALOG (latest-wins, aux 0; sent as a preamble once a session and after a data reload). The works catalogue (SINK.md §4.2) in one record, each
     * template's items read from its palette. A kind is {@link #DEF} bytes, then its variants; a
     * variant is {@link #VARIANT} bytes, then its items.
     */
    public static final class WorkCatalog {
        public static final int N = 8;            // u32 kinds
        public static final int DEFS = 16;
        public static final int DEF = 32;
        public static final int D_ID = 0;         // u32 dictionary (DICT_WORK)
        public static final int D_GROUP = 4;      // u32: 0 lamp, 1 decor, 2 golem
        public static final int D_CULTURE = 8;    // u32 dictionary (DICT_CULTURE)
        public static final int D_STYLE = 12;     // u32 dictionary (DICT_STRUCTURE), 0 = every village of the culture
        public static final int D_CAP = 16;       // u32: CAP_*
        public static final int D_CAP_N = 20;     // u32
        public static final int D_VARIANTS = 24;  // u32
        public static final int VARIANT = 24;
        public static final int V_PLACE = 0;      // u32: PLACE_*
        public static final int V_WHAT = 4;       // u32 dictionary: DICT_TEMPLATE or DICT_BODY
        public static final int V_PROCESSORS = 8; // u32 dictionary (DICT_PROCESSORS), 0 = none
        public static final int V_BLOCKS = 12;    // u32 blocks placed (bodies spawned)
        public static final int V_ITEMS = 16;     // u32 items n, then n x (u32 item dictionary id, u32 count)
        public static final int ITEM = 8;

        public static final int GROUP_LAMP = 0;
        public static final int GROUP_DECOR = 1;
        public static final int GROUP_GOLEM = 2;
        public static final int CAP_PER_RESIDENTS = 1;
        public static final int CAP_FIXED = 2;
        public static final int PLACE_TEMPLATE = 1;
        public static final int PLACE_BODY = 2;
    }

    /** VILLAGE_GOSSIP (latest-wins). Vanilla's own gossip about players, summed over the census's residents. */
    public static final class VillageGossip {
        public static final int VILLAGE = 8;    // u64
        public static final int ENTRIES = 16;   // u32
        public static final int RESIDENTS = 20; // u32
        public static final int FIRST = 24;     // ENTRIES x Entry
        public static final int ENTRY = 40;     // u64 most, u64 least, i32 x 5 (major_negative, minor_negative, minor_positive, major_positive, trading), u32 reserved

        public static int size(int entries) {
            return FIRST + entries * ENTRY;
        }
    }

    /**
     * Op ids made on the Java side have the top bit set; the engine's have it clear. A Java op id is
     * this bit OR the obsSeq of the record that carries it. The kernel's {@code journal::OpId} uses
     * the same convention ({@code JAVA_OP_BIT}, decision log O10).
     */
    public static final long JAVA_OP_BIT = 0x8000_0000_0000_0000L;
}
