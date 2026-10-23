package com.minecraftindustrialcomplex.mic_rtsim.conflict;

/**
 * The conflict records of docs/boundary.md §9, offsets from the start of the record. Mirrors
 * {@code crates/rtsim-conflict/src/boundary.rs} (the reference codec): the three change together.
 * Journaled observations carry {@code obsSeq} at 8 and {@code gameTime} at 16 (the core's journal fills
 * both); commands carry {@code cmdSeq} at 8.
 */
public final class ConflictRecords {
    private ConflictRecords() {}

    // ---- kinds (docs/boundary.md §9.1) ----------------------------------------------------------

    public static final int BAND_STATE = 0x0090;
    public static final int FACTION_FOUND = 0x0800;
    public static final int TERRITORY_SIGNAL = 0x0801;
    public static final int POLLUTION = 0x0802;
    public static final int BAND_MATERIALIZED = 0x0803;
    public static final int BAND_SPAWN_REJECTED = 0x0804;
    public static final int BAND_DEMATERIALIZED = 0x0805;
    public static final int BAND_CASUALTIES = 0x0806;
    public static final int CHIEF_KILLED = 0x0807;
    public static final int ENVOY_ANSWER = 0x0808;
    public static final int TRIBUTE_PAID = 0x0809;
    public static final int GIFT_GIVEN = 0x080A;
    public static final int BAND_LOOTED = 0x080B;
    /** M1 (feat/mowzie-m1): the world's difficulty, latest-wins and sticky, sent as a preamble. */
    public static final int DIFFICULTY = 0x0093;
    /** M1: a kept FACTION_NOTICE was played (aux = its step). */
    public static final int NOTICE_SHOWN = 0x0811;

    public static final int TERRITORY = 0x8800;
    public static final int FACTION_NOTICE = 0x8801;
    public static final int ENVOY_MATERIALIZE = 0x8802;
    public static final int BAND_MATERIALIZE = 0x8803;
    public static final int BAND_STEER = 0x8804;
    public static final int BAND_RECALL = 0x8805;
    public static final int DEMAND_SETTLED = 0x8806;

    public static boolean isConflictCommand(int kind) {
        return kind >= 0x8800 && kind <= 0x88FF;
    }

    /** A party's agent id carries namespace byte 0x43 ('C', docs/boundary.md §9.2). */
    public static final long AGENT_NAMESPACE = 0x43L;

    public static boolean isConflictAgent(long agent) {
        return (agent >>> 56) == AGENT_NAMESPACE;
    }

    // ---- shared codes ---------------------------------------------------------------------------

    /** Territory signals (TERRITORY_SIGNAL's aux, the bit numbers of TERRITORY's mask). */
    public static final int SIGNAL_SMOG = 0, SIGNAL_RADIATION = 1, SIGNAL_LOGGING = 2, SIGNAL_MINING = 3, SIGNAL_KILLING = 4,
            SIGNAL_BUILDING = 5;

    /** Who killed a member (BAND_CASUALTIES' and CHIEF_KILLED's aux). */
    public static final int BY_NONE = 0, BY_PLAYER = 1, BY_MOB = 2, BY_OTHER = 3;

    /** BAND_MATERIALIZE's purpose. */
    public static final int PURPOSE_ENVOY = 1, PURPOSE_SCOUTS = 2, PURPOSE_RAID = 3, PURPOSE_INVASION = 4, PURPOSE_SIEGE = 5,
            PURPOSE_PLUNDER = 6;

    /** ENVOY_ANSWER's aux. */
    public static final int ANSWER_ACCEPTED = 1, ANSWER_REFUSED = 2, ANSWER_ATTACKED = 3;

    /** DEMAND_SETTLED's aux. */
    public static final int SETTLED_PAID = 1, SETTLED_CUT = 2, SETTLED_EXPIRED = 3, SETTLED_REFUSED = 4;
    /** M1: the envoy party was gone with no player to blame (it goes home; BAND_RECALL ROUTED follows). */
    public static final int SETTLED_WITHDRAWN = 5;

    /** BAND_RECALL's aux. */
    public static final int RECALL_BORED = 1, RECALL_ROUTED = 2, RECALL_DONE = 3, RECALL_PASSING = 4, RECALL_PLAYER_LEFT = 5,
            RECALL_SETTLED = 6;

    // ---- observations ---------------------------------------------------------------------------

    /** BAND_STATE (latest-wins, aux = flags). Every {@link ConflictBridge#STATE_INTERVAL} ticks per party. */
    public static final class BandState {
        public static final int AGENT = 8, EPOCH = 16, DIM = 20, PRESENT = 24, X = 32, Y = 40, Z = 48, SIZE = 56;
        public static final int FLAG_TICKING = 1, FLAG_FIGHTING = 2;
    }

    /** FACTION_FOUND (aux = flags). */
    public static final class FactionFound {
        public static final int FACTION = 24, DIM = 32, STRUCTURE = 36, X = 40, Y = 44, Z = 48, SIZE = 56;
        /** M1: u16 bodies at home and u8 census flags at discovery (main's decoder ignores 52 to 55). */
        public static final int HOME_BODIES = 52, CENSUS = 54;
        public static final int FLAG_LATE = 1;
        /** CENSUS flags: the chief is present; it holds its crystal. */
        public static final int CENSUS_CHIEF = 1, CENSUS_CRYSTAL = 2;
    }

    /** DIFFICULTY (M1, latest-wins, aux 0): Minecraft's difficulty id, u32. */
    public static final class DifficultyRecord {
        public static final int DIFFICULTY = 8, SIZE = 16;
    }

    /** NOTICE_SHOWN (M1, journaled, aux = the notice's step): the faction and the notice's player ((0,0): the settlers). */
    public static final class NoticeShown {
        public static final int FACTION = 24, PLAYER_MOST = 32, PLAYER_LEAST = 40, SIZE = 48;
    }

    /** TERRITORY_SIGNAL (aux = signal). */
    public static final class TerritorySignal {
        public static final int FACTION = 24, DIM = 32, COUNT = 36, PLAYER_MOST = 40, PLAYER_LEAST = 48, X = 56, Y = 60, Z = 64,
                SIZE = 72;
    }

    /** BAND_MATERIALIZED (aux = members spawned). */
    public static final class Materialized {
        public static final int AGENT = 24, EPOCH = 32, DIM = 36, WAVE = 40, SIZE = 48;
    }

    /** BAND_SPAWN_REJECTED (aux = reason: docs/boundary.md §2.3's codes, plus 9 unknown body kind). */
    public static final class SpawnRejected {
        public static final int AGENT = 24, EPOCH = 32, WAVE = 36, CMD_SEQ = 40, SIZE = 48;
        public static final int NOT_TICKING = 1, UNKNOWN_DIMENSION = 3, ALREADY_EMBODIED = 4, SPAWN_FAILED = 5, BAD_RECORD = 6,
                DISCARDED_BY_SERVER = 7, UNKNOWN_BODY = 9;
    }

    /** BAND_DEMATERIALIZED (aux = reason, docs/boundary.md §2.3's codes), then n × (u32 body, u32 survivors). */
    public static final class Dematerialized {
        public static final int AGENT = 24, EPOCH = 32, COUNT = 36, SURVIVORS = 40;
        public static final int ENTRY = 8;
        /** Within an entry. */
        public static final int E_BODY = 0, E_COUNT = 4;
        public static final int COMMAND = 1, NOT_TICKING = 2, UNLOADED = 3, SESSION_END = 4, MEMBERS_GONE = 5, NOT_EMBODIED = 6;

        public static int size(int n) {
            return SURVIVORS + ENTRY * n;
        }
    }

    /** BAND_CASUALTIES (aux = killer kind). */
    public static final class Casualties {
        public static final int AGENT = 24, EPOCH = 32, BODY = 36, COUNT = 40, KILLER_MOST = 48, KILLER_LEAST = 56, SIZE = 64;
    }

    /** CHIEF_KILLED (aux = killer kind), with a Java op id. */
    public static final class ChiefKilled {
        public static final int AGENT = 24, EPOCH = 32, KILLER_MOST = 40, KILLER_LEAST = 48, OP_ID = 56, SIZE = 64;
    }

    /** ENVOY_ANSWER (aux = answer). */
    public static final class EnvoyAnswer {
        public static final int AGENT = 24, EPOCH = 32, PLAYER_MOST = 40, PLAYER_LEAST = 48, SIZE = 56;
    }

    /** One item of TRIBUTE_PAID, GIFT_GIVEN and BAND_LOOTED: u32 item (dictionary 2), i32 count, u64 reserved. */
    public static final int ITEM = 16;
    public static final int I_ITEM = 0, I_COUNT = 4;

    /** TRIBUTE_PAID (aux = n items), with a Java op id. */
    public static final class TributePaid {
        public static final int FACTION = 24, AGENT = 32, EPOCH = 40, PLAYER_MOST = 48, PLAYER_LEAST = 56, OP_ID = 64, ITEMS = 72;

        public static int size(int n) {
            return ITEMS + ITEM * n;
        }
    }

    /** GIFT_GIVEN (aux = n items), with a Java op id. */
    public static final class GiftGiven {
        public static final int FACTION = 24, PLAYER_MOST = 32, PLAYER_LEAST = 40, OP_ID = 48, ITEMS = 56;

        public static int size(int n) {
            return ITEMS + ITEM * n;
        }
    }

    /** BAND_LOOTED (aux = n items), with a Java op id. */
    public static final class Looted {
        public static final int AGENT = 24, EPOCH = 32, OP_ID = 40, ITEMS = 48;

        public static int size(int n) {
            return ITEMS + ITEM * n;
        }
    }

    // ---- commands -------------------------------------------------------------------------------

    /** TERRITORY (aux = flags). */
    public static final class Territory {
        public static final int FACTION = 16, DIM = 24, MASK = 28, X = 32, Z = 36, RADIUS = 40, SIZE = 48;
        public static final int FLAG_REMOVED = 1;
    }

    /** FACTION_NOTICE (aux = rung). */
    public static final class Notice {
        public static final int FACTION = 16, PLAYER_MOST = 24, PLAYER_LEAST = 32, DIM = 40, SIGNAL = 44, X = 48, Z = 52,
                GRIEVANCE = 56, SIZE = 64;
    }

    /** ENVOY_MATERIALIZE (aux = m), then m × (u32 body, u32 count). */
    public static final class Envoy {
        public static final int AGENT = 16, EPOCH = 24, DIM = 28, FACTION = 32, X = 40, Y = 48, Z = 56, PLAYER_MOST = 64,
                PLAYER_LEAST = 72, DEMAND = 80, SIGNAL = 84, ITEM = 88, COUNT = 92, DEADLINE = 96, MEMBERS = 104;
        public static final int MEMBER = 8;
        /** Within a member entry. */
        public static final int M_BODY = 0, M_COUNT = 4;
        public static final int DEMAND_TRIBUTE = 1, DEMAND_STOP = 2;

        public static int size(int m) {
            return MEMBERS + MEMBER * m;
        }
    }

    /** BAND_MATERIALIZE (aux = m stacks), then m × (u32 body, u32 count, u32 flags, u32 reserved). */
    public static final class Materialize {
        public static final int AGENT = 16, EPOCH = 24, DIM = 28, FACTION = 32, PURPOSE = 40, FLAGS = 41, WAVE = 42, BEARING = 44,
                ARC_X = 48, ARC_Z = 56, ARC_RADIUS = 64, HALF_WIDTH = 68, TARGET_X = 72, TARGET_Y = 80, TARGET_Z = 88,
                TARGET_PLAYER_MOST = 96, TARGET_PLAYER_LEAST = 104, SPEED = 112, STACKS = 120;
        public static final int STACK = 16;
        /** Within a stack entry. */
        public static final int S_BODY = 0, S_COUNT = 4, S_FLAGS = 8;
        public static final int FLAG_HOSTILE = 1, FLAG_CHIEF = 2, FLAG_SURFACE = 4;
        /** A stack's flags: the war chief is in this stack. */
        public static final int STACK_CHIEF = 1;

        public static int size(int m) {
            return STACKS + STACK * m;
        }
    }

    /** BAND_STEER (aux = flags). */
    public static final class Steer {
        public static final int AGENT = 16, EPOCH = 24, SPEED = 28, X = 32, Y = 40, Z = 48, ARRIVE = 56, SIZE = 64;
        public static final int FLAG_ATTACK = 1;
    }

    /** BAND_RECALL (aux = reason). */
    public static final class Recall {
        public static final int AGENT = 16, EPOCH = 24, TIMEOUT = 28, TOWARD_X = 32, TOWARD_Z = 40, SIZE = 48;
    }

    /** DEMAND_SETTLED (aux = outcome). */
    public static final class Settled {
        public static final int FACTION = 16, AGENT = 24, PLAYER_MOST = 32, PLAYER_LEAST = 40, SIGNAL = 48, SIZE = 56;
    }

    /** The smallest a command of this kind can be (a shorter one is malformed). */
    public static int minSize(int kind) {
        return switch (kind) {
            case TERRITORY -> Territory.SIZE;
            case FACTION_NOTICE -> Notice.SIZE;
            case ENVOY_MATERIALIZE -> Envoy.MEMBERS;
            case BAND_MATERIALIZE -> Materialize.STACKS;
            case BAND_STEER -> Steer.SIZE;
            case BAND_RECALL -> Recall.SIZE;
            case DEMAND_SETTLED -> Settled.SIZE;
            default -> 16;
        };
    }

    /** A signal's name, for logs. */
    public static String signalName(int signal) {
        return switch (signal) {
            case SIGNAL_SMOG -> "smog";
            case SIGNAL_RADIATION -> "radiation";
            case SIGNAL_LOGGING -> "logging";
            case SIGNAL_MINING -> "mining";
            case SIGNAL_KILLING -> "killing";
            case SIGNAL_BUILDING -> "building";
            default -> "signal " + signal;
        };
    }
}
