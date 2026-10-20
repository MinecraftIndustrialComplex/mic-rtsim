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

    public static final int MATERIALIZED = 0x0100;
    public static final int SPAWN_REJECTED = 0x0101;
    public static final int DEMATERIALIZED = 0x0102;
    public static final int MEMBER_STATE = 0x0103;
    public static final int TAKEN = 0x0104;
    public static final int DIED = 0x0105;
    public static final int TRANSFER = 0x0110;
    public static final int COMMAND_RESULT = 0x0111;

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

    /** COMMAND_RESULT (journaled), aux = status. The outcome of a command that carries an op id. */
    public static final class CommandResult {
        public static final int CMD_SEQ = 24; // u64
        public static final int OP_ID = 32;   // u64
        public static final int SIZE = 40;

        public static final int APPLIED = 1;
        public static final int DUPLICATE_OP = 2;
        public static final int FAILED = 3;
    }

    /**
     * Op ids the engine allocates have the top bit set; the bridge's have it clear (the kernel's
     * {@code journal::OpId}). A bridge op id is the obsSeq of the record that carries it.
     */
    public static final long ENGINE_OP_BIT = 0x8000_0000_0000_0000L;
}
