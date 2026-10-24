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
    /** M6: a chief's health and crystal at home (latest-wins, aux = flags; the Frostmaw's steal, MZB-273). */
    public static final int CHIEF_STATE = 0x0091;
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
    /**
     * M1: the world's difficulty, journaled at session start and on each change (MZB-17). 0x0093 is retired:
     * it was latest-wins, and journaling a latest-wins id throws.
     */
    public static final int DIFFICULTY = 0x0815;
    /** M1: a kept FACTION_NOTICE was played (aux = its notice serial, {@link Notice#SERIAL}). */
    public static final int NOTICE_SHOWN = 0x0811;

    /** M5: the camp reconciler's records (MOWZIE.md §4.9; docs/boundary.md §9, with M4's CAMP_BODIES action). */
    public static final int CAMP_DEATH = 0x080E;
    public static final int CAMP_CENSUS = 0x080F;
    public static final int CAMP_BODIES = 0x0812;
    public static final int CAMP_OUT = 0x0813;
    public static final int CAMP_RECONCILE = 0x8808;

    /**
     * M9: the Umvuthana's records. TRADED and CHIEF_PAID carry Java op ids and are journaled; HAVEN_CHEST is
     * journaled too (0x0092 is retired: it was latest-wins there before MZB-446). HUNT_APPLIED answers the
     * engine's OWED_HUNT command ({@link OwedHunts}, MZB-425, MZB-426) and is journaled.
     */
    public static final int TRADED = 0x0810;
    public static final int CHIEF_PAID = 0x0814;
    public static final int HUNT_APPLIED = 0x0816;
    public static final int HAVEN_CHEST = 0x0817;
    /** M9: the hunt a raid on an unwatched district owes (a command, aux 0). */
    public static final int OWED_HUNT = 0x880C;
    /** M9: the chief's trade memory at home (a command, aux = traded players t). */
    public static final int CHIEF_BODY = 0x880A;

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

    /** ENVOY_ANSWER's aux. The bridge sends only ATTACKED: no click refuses (MZB-110); REFUSED stays in the codec. */
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

    /** CHIEF_STATE (M6, latest-wins, aux = flags: {@link #CRYSTAL} when the chief holds its crystal). */
    public static final class ChiefState {
        public static final int FACTION = 8, HEALTH = 16, SIZE = 24;
        public static final int CRYSTAL = 1;
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

    /** DIFFICULTY (M1, journaled, aux 0): Minecraft's difficulty id, u32. */
    public static final class DifficultyRecord {
        public static final int DIFFICULTY = 24, SIZE = 32;
    }

    /**
     * NOTICE_SHOWN (M1, journaled, aux = the notice's serial): the faction and the notice's player ((0,0): the
     * settlers).
     */
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

    /**
     * FACTION_NOTICE (aux = rung). SERIAL (M1): the notice's u32 serial, one engine-wide counter (MZB-28),
     * which NOTICE_SHOWN echoes in its aux.
     */
    public static final class Notice {
        public static final int FACTION = 16, PLAYER_MOST = 24, PLAYER_LEAST = 32, DIM = 40, SIGNAL = 44, X = 48, Z = 52,
                GRIEVANCE = 56, SERIAL = 60, SIZE = 64;
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

    // ---- the camp reconciler (M5; docs/boundary.md §9) -------------------------------------------

    /** CAMP_DEATH's killer kind (aux): BY_* above, and a body carried off (Carry On). */
    public static final int BY_TAKEN = 4;

    /** CAMP_DEATH (aux = killer kind): once per body UUID, carried on a marker where it died. */
    public static final class CampDeath {
        public static final int FACTION = 24, BODY = 32, FLAGS = 36, SLOT = 40, GENERATION = 44, KILLER_MOST = 48,
                KILLER_LEAST = 56, OP_ID = 64, SIZE = 72;
        public static final int FLAG_CHIEF = 1, FLAG_STRAY = 2;
    }

    /** CAMP_CENSUS (aux = bodies n): the camp came in at a new epoch. */
    public static final class CampCensus {
        public static final int FACTION = 24, EPOCH = 32, FLAGS = 40, HEALTH = 44, THRONE_X = 48, THRONE_Y = 56, THRONE_Z = 64,
                DIRECTION = 72, TRADED = 76, TRADED_SLOT = 80, TRADED_GENERATION = 84, BODIES = 88, BODY = 32, TRADER = 16;
        /** Per body. */
        public static final int B_BODY = 0, B_FLAGS = 4, B_SLOT = 8, B_GENERATION = 12, B_UUID_MOST = 16, B_UUID_LEAST = 24;
        public static final int CHIEF = 1, CRYSTAL = 2, THRONE = 4;
        public static final int BODY_STRAY = 1, BODY_ELSEWHERE = 2, BODY_CHIEF = 4;

        public static int size(int n, int t) {
            return BODIES + BODY * n + TRADER * t;
        }
    }

    /** CAMP_BODIES (aux = lines n): the answer to a CAMP_RECONCILE, each line with the action it answers. */
    public static final class CampBodies {
        public static final int FACTION = 24, EPOCH = 32, OP_ID = 40, LINES = 48, LINE = 16;
        public static final int L_LINE = 0, L_SLOT = 4, L_GENERATION = 8, L_ACTION = 12, L_OUTCOME = 13;
        public static final int DONE = 1, NOTHING = 2, STALE = 3;

        public static int size(int n) {
            return LINES + LINE * n;
        }
    }

    /** CAMP_OUT (aux 0): a footprint chunk dropped below FULL and the epoch ended. */
    public static final class CampOut {
        public static final int FACTION = 24, EPOCH = 32, SIZE = 40;
    }

    /** CAMP_RECONCILE (command, aux = lines n). */
    public static final class CampReconcile {
        public static final int FACTION = 16, EPOCH = 24, OP_ID = 32, THRONE_X = 40, THRONE_Y = 48, THRONE_Z = 56, DIRECTION = 64,
                FLAGS = 68, LINES = 72, LINE = 32;
        public static final int L_SLOT = 0, L_GENERATION = 4, L_BODY = 8, L_ACTION = 12, L_FLAGS = 13, L_A = 16, L_B = 24;
        public static final int SPAWN = 1, ADOPT = 2, TAKE = 3, SET_OUT = 4, HAND_BACK = 5, REMOVE = 6, SEAT_HEIR = 7;
        /**
         * M9 (MZB-408): the warrior the heir becomes. Always the first line of a pair whose second is a SEAT_HEIR;
         * it names no slot (slot 0, generation 0), and its answer names the warrior taken.
         */
        public static final int MASK = 8;
        /** The chief's slot (the Umvuthi's, MZB-401): {@code u32::MAX} in the engine, -1 as a Java int; outside the pool. */
        public static final int CHIEF_SLOT = -1;
        /** A line's flags: the slot is the camp's chief; the body holds its crystal. */
        public static final int LINE_CHIEF = 1, LINE_CRYSTAL = 2;
        public static final int FLAG_THRONE = 1;

        public static int size(int n) {
            return LINES + LINE * n;
        }

        public static String actionName(int action) {
            return switch (action) {
                case SPAWN -> "spawn";
                case ADOPT -> "adopt";
                case TAKE -> "take";
                case SET_OUT -> "set out";
                case HAND_BACK -> "hand back";
                case REMOVE -> "remove";
                case SEAT_HEIR -> "seat heir";
                case MASK -> "mask";
                default -> "action " + action;
            };
        }
    }

    // ---- the Umvuthana (M9; docs/boundary.md §9.2 to 9.6) ----------------------------------------

    /**
     * TRADED (journaled, a Java op id): a masked trade with a grove's minion, {@code nuggets} the gold nuggets on
     * either side of it (MZB-417).
     */
    public static final class Traded {
        public static final int FACTION = 24, PLAYER_MOST = 32, PLAYER_LEAST = 40, NUGGETS = 48, OP_ID = 56, SIZE = 64;
    }

    /**
     * CHIEF_PAID (aux = n items, journaled, a Java op id): a payment to a grove's Umvuthi (MZB-418). The slot and
     * generation are the body's CampMember tag, both 0 with PAID_STRAY.
     */
    public static final class ChiefPaid {
        public static final int FACTION = 24, SLOT = 32, GENERATION = 36, FLAGS = 40, PLAYER_MOST = 48, PLAYER_LEAST = 56,
                OP_ID = 64, ITEMS = 72;
        /** A payment to an untagged Umvuthi standing in a grove's footprint. */
        public static final int PAID_STRAY = 1;

        public static int size(int n) {
            return ITEMS + ITEM * n;
        }
    }

    /**
     * OWED_HUNT (command, aux 0; docs/boundary.md §9.4): the animals a raid on an unwatched district owes
     * (MZB-424, MZB-425). {@code REPLACED} is the op of the hunt this one replaced in flight (0: none) and
     * {@code REST} the animals to kill instead of {@code COUNT} when that op is the last one applied for the
     * faction and district (MZB-449).
     */
    public static final class OwedHunt {
        public static final int FACTION = 16, DISTRICT = 24, OP_ID = 32, REPLACED = 40, DIM = 48, COUNT = 52, CENTRE_X = 56,
                CENTRE_Z = 60, RADIUS = 64, REST = 68, SIZE = 72;
    }

    /** HUNT_APPLIED (journaled, aux 0): an OWED_HUNT was applied, with the animals it killed (MZB-426). */
    public static final class HuntApplied {
        public static final int FACTION = 24, DISTRICT = 32, OP_ID = 40, KILLED = 48, SIZE = 56;
    }

    /** HAVEN_CHEST (journaled; M9c sends it): the chest a grove's tribute goes into, and its free slots (MZB-422). */
    public static final class HavenChest {
        public static final int FACTION = 24, DIM = 32, X = 36, Y = 40, Z = 44, FREE = 48, SIZE = 56;
    }

    /**
     * CHIEF_BODY (command, aux = traded players t): the chief's trade memory at home (MZB-421). Agent 0 and flags 0
     * at home; {@code flags} 1 is the crystal, M11's. The players follow from {@link #TRADED}, 16 bytes each.
     */
    public static final class ChiefBody {
        public static final int FACTION = 16, AGENT = 24, EPOCH = 32, HEALTH = 40, FLAGS = 44, TRADED = 48, PLAYER = 16;
        public static final int CRYSTAL = 1;

        public static int size(int t) {
            return TRADED + PLAYER * t;
        }
    }

    /** The smallest a command of this kind can be (a shorter one is malformed). */
    public static int minSize(int kind) {
        return switch (kind) {
            case CAMP_RECONCILE -> CampReconcile.LINES;
            case CHIEF_BODY -> ChiefBody.TRADED;
            case TERRITORY -> Territory.SIZE;
            case FACTION_NOTICE -> Notice.SIZE;
            case ENVOY_MATERIALIZE -> Envoy.MEMBERS;
            case BAND_MATERIALIZE -> Materialize.STACKS;
            case BAND_STEER -> Steer.SIZE;
            case BAND_RECALL -> Recall.SIZE;
            case DEMAND_SETTLED -> Settled.SIZE;
            case OWED_HUNT -> OwedHunt.SIZE;
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
