package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

/**
 * The dragon records of docs/boundary.md §6, offsets from the start of the record. Mirrors
 * {@code crates/rtsim-ecology/src/boundary.rs} (the reference codec): the three change together.
 * Journaled observations carry {@code obsSeq} at 8 and {@code gameTime} at 16 (the core's journal
 * fills both); commands carry {@code cmdSeq} at 8.
 */
public final class DragonRecords {
    private DragonRecords() {}

    // ---- kinds (docs/boundary.md §6.1) ----------------------------------------------------------

    public static final int DRAGON_STATE = 0x0060;
    public static final int LAIR_CHEST = 0x0061;
    public static final int DRAGON_MATERIALIZED = 0x0500;
    public static final int DRAGON_SPAWN_REJECTED = 0x0501;
    public static final int DRAGON_DEMATERIALIZED = 0x0502;
    public static final int DRAGON_HURT = 0x0503;
    public static final int DRAGON_DIED = 0x0504;
    public static final int LAIR_FOUND = 0x0505;
    public static final int HOARD_ROBBED = 0x0506;
    public static final int DRAGON_UNTAGGED = 0x0507;
    public static final int DRAGON_ATE = 0x0508;
    public static final int OIL_CHUNKS = 0x0509;
    public static final int DRAGON_BREATHS = 0x050A;
    public static final int OIL_CONFIG = 0x050B;
    public static final int NEST_FOUND = 0x050C;
    public static final int NEST_CENSUS = 0x050D;
    public static final int NEST_LOSS = 0x050E;
    public static final int NEST_HEART = 0x050F;

    public static final int DRAGON_MATERIALIZE = 0x8500;
    public static final int DRAGON_STEER = 0x8501;
    public static final int DRAGON_DEPART = 0x8502;
    public static final int DRAGON_ADOPT = 0x8503;
    public static final int DRAGON_DISCARD = 0x8504;
    public static final int PLACE_LAIR = 0x8505;
    public static final int OIL_SURVEY = 0x8506;
    // The underground half (OILGATE.md §4.6, OG-9e): a Death Worm nest under an oil field.
    public static final int PLACE_NEST = 0x8507;
    public static final int NEST_RENEW = 0x8508;
    public static final int NEST_SORTIE = 0x8509;

    public static boolean isDragonCommand(int kind) {
        return kind >= 0x8500 && kind <= 0x85FF;
    }

    /** A dragon's agent id carries namespace byte 0x44 (choice R13). */
    public static final long AGENT_NAMESPACE = 0x44L;

    public static boolean isDragonAgent(long agent) {
        return (agent >>> 56) == AGENT_NAMESPACE;
    }

    // ---- observations ---------------------------------------------------------------------------

    /** DRAGON_STATE (latest-wins, aux = flags). */
    public static final class State {
        public static final int AGENT = 8, EPOCH = 16, DIM = 20, HEALTH = 24, MAX_HEALTH = 28, HUNGER = 32,
                AGE_TICKS = 36, X = 40, Y = 48, Z = 56, SIZE = 64;
        public static final int FLAG_SLEEPING = 1, FLAG_FLYING = 2, FLAG_HAS_TARGET = 4, FLAG_TICKING = 8;
    }

    /** LAIR_CHEST (latest-wins, aux 0): the chest a lair's tribute goes into (SINK.md §4.6). */
    public static final class LairChest {
        public static final int LAIR = 8, DIM = 16, X = 20, Y = 24, Z = 28, SIZE = 32;
    }

    /** DRAGON_MATERIALIZED (aux 0). */
    public static final class Materialized {
        public static final int AGENT = 24, EPOCH = 32, DIM = 36, ENTITY_MOST = 40, ENTITY_LEAST = 48, SIZE = 56;
    }

    /** DRAGON_SPAWN_REJECTED (aux = reason: docs/boundary.md §2.3's codes, plus 9 unknown species). */
    public static final class SpawnRejected {
        public static final int AGENT = 24, EPOCH = 32, CMD_SEQ = 40, SIZE = 48;
        public static final int NOT_TICKING = 1, UNKNOWN_DIMENSION = 3, ALREADY_EMBODIED = 4, SPAWN_FAILED = 5,
                BAD_RECORD = 6, DISCARDED_BY_SERVER = 7, UNKNOWN_SPECIES = 9;
    }

    /** DRAGON_DEMATERIALIZED (aux = reason, docs/boundary.md §2.3's DEMATERIALIZED codes). */
    public static final class Dematerialized {
        public static final int AGENT = 24, EPOCH = 32, DIM = 36, HEALTH = 40, MAX_HEALTH = 44, HUNGER = 48,
                AGE_TICKS = 52, X = 56, Y = 64, Z = 72, SIZE = 80;
        public static final int COMMAND = 1, NOT_TICKING = 2, UNLOADED = 3, SESSION_END = 4, NOT_EMBODIED = 6;
    }

    /** DRAGON_HURT (aux = attacker kind). */
    public static final class Hurt {
        public static final int AGENT = 24, EPOCH = 32, HITS = 36, AMOUNT = 40, HEALTH_AFTER = 44,
                ATTACKER_MOST = 48, ATTACKER_LEAST = 56, SIZE = 64;
    }

    /** Who hurt or killed a dragon (aux of DRAGON_HURT and DRAGON_DIED). */
    public static final int ATTACKER_NONE = 0, ATTACKER_PLAYER = 1, ATTACKER_MOB = 2, ATTACKER_OTHER = 3;

    /** DRAGON_DIED (aux = killer kind). */
    public static final class Died {
        public static final int AGENT = 24, EPOCH = 32, DIM = 36, KILLER_MOST = 40, KILLER_LEAST = 48,
                X = 56, Y = 64, Z = 72, SIZE = 80;
    }

    /** LAIR_FOUND (aux = flags, 1 LATE). */
    public static final class LairFound {
        public static final int LAIR = 24, DIM = 32, STRUCTURE = 36, MIN_X = 40, MIN_Y = 44, MIN_Z = 48,
                MAX_X = 52, MAX_Y = 56, MAX_Z = 60, HOME_X = 64, HOME_Y = 68, HOME_Z = 72, SIZE = 80;
        public static final int FLAG_LATE = 1;
    }

    /** HOARD_ROBBED (aux = item count n). */
    public static final class HoardRobbed {
        public static final int LAIR = 24, OP_ID = 32, PLAYER_MOST = 40, PLAYER_LEAST = 48, ITEMS = 56;
        /** One item: u32 item, i32 count, u64 reserved. */
        public static final int ITEM = 16;
        /** Most items one record holds (well below the core's largest record). */
        public static final int MAX_ITEMS = 1024;

        public static int size(int n) {
            return ITEMS + ITEM * n;
        }
    }

    /** DRAGON_UNTAGGED (aux = spawn kind). */
    public static final class Untagged {
        public static final int ENTITY_MOST = 24, ENTITY_LEAST = 32, SPECIES = 40, DIM = 44, FLAGS = 48,
                AGE_TICKS = 52, HUNGER = 56, GENDER = 60, VARIANT = 64, HOME_X = 68, HOME_Y = 72, HOME_Z = 76,
                LAIR = 80, X = 88, Y = 96, Z = 104, SIZE = 112;
        public static final int SPAWN_STRUCTURE = 1, SPAWN_NATURAL = 2, SPAWN_SPAWNER = 3, SPAWN_EGG = 4,
                SPAWN_COMMAND = 5, SPAWN_HATCHED = 6, SPAWN_OTHER = 7;
        public static final int FLAG_TAMED = 1, FLAG_HAS_HOME = 2, FLAG_SLEEPING = 4, FLAG_AGING_DISABLED = 8;
    }

    /** DRAGON_ATE (aux = count). */
    public static final class Ate {
        public static final int AGENT = 24, EPOCH = 32, PREY = 36, VILLAGE = 40, X = 48, Y = 56, Z = 64, SIZE = 72;
    }

    /** OIL_CHUNKS (aux = entry count n): oil chunks found in one dimension (OILGATE.md §4.1, OG-2). */
    public static final class OilChunks {
        public static final int DIM = 24, SOURCE = 28, ENTRIES = 32;
        /** One entry: i32 cx, i32 cz, i32 amount (mB), u32 flags. */
        public static final int ENTRY = 16, E_CX = 0, E_CZ = 4, E_AMOUNT = 8, E_FLAGS = 12;
        /** Most entries one record holds (boundary.rs OIL_CHUNKS_MAX). */
        public static final int MAX = 1024;
        public static final int SOURCE_DEEP_TIME = 1, SOURCE_CDG = 2, SOURCE_DEPOSIT = 3;
        /** The chunk never runs dry (CDG's infinite deposits). */
        public static final int FLAG_BOTTOMLESS = 1;

        public static int size(int n) {
            return ENTRIES + ENTRY * n;
        }
    }

    /** DRAGON_BREATHS (aux = breaths since the last report): an embodied dragon's fire (OG-4). */
    public static final class Breaths {
        public static final int AGENT = 24, EPOCH = 32, SIZE = 40;
    }

    /** OIL_CONFIG (aux = flags): the oil gate's switches, sent at session start (OILGATE.md §4.2, §4.8). */
    public static final class OilConfig {
        public static final int SIZE = 24;
        public static final int FLAG_FIELD_FLIGHTS = 1, FLAG_PLACE_LAIRS = 2;
        /** Oil chunks found nests (OG-9e): the engine sends PLACE_NEST, re-sent after each OIL_CONFIG. */
        public static final int FLAG_PLACE_NESTS = 4;
    }

    /**
     * NEST_FOUND (aux: 1 placed, 2 failed): a PLACE_NEST answered (OG-9e). A failed one carries zeros for
     * the heart and the engine gives up the cell. An already placed nest answers again with its heart.
     */
    public static final class NestFound {
        public static final int NEST = 24, DIM = 32, HEART_X = 36, HEART_Y = 40, HEART_Z = 44, SIZE = 48;
        public static final int PLACED = 1, FAILED = 2;
    }

    /** NEST_CENSUS (aux = flags): a nest's worms, live (loaded or not) plus still owed. */
    public static final class NestCensus {
        public static final int NEST = 24, WORMS = 32, RESERVED = 36, SIZE = 40;
        /** The heart is no longer a spawner: the engine counts it as broken. */
        public static final int FLAG_HEART_MISSING = 1;
    }

    /** NEST_LOSS (aux = cause): nest worms died. The boss rule: any death not credited to a wild mob is a player's. */
    public static final class NestLoss {
        public static final int NEST = 24, COUNT = 32, RESERVED = 36, PLAYER_MOST = 40, PLAYER_LEAST = 48, SIZE = 56;
        public static final int CAUSE_PLAYER = 0, CAUSE_AGE = 1, CAUSE_STARVED = 2, CAUSE_FACTION = 3,
                CAUSE_PREDATOR = 4, CAUSE_OTHER = 5;
    }

    /** NEST_HEART (aux 0): the spawner heart was broken (by the player, zeros for none). */
    public static final class NestHeart {
        public static final int NEST = 24, PLAYER_MOST = 32, PLAYER_LEAST = 40, SIZE = 48;
    }

    // ---- commands -------------------------------------------------------------------------------

    /** DRAGON_MATERIALIZE (aux = flags). */
    public static final class Materialize {
        public static final int AGENT = 16, EPOCH = 24, SPECIES = 28, DIM = 32, AGE_TICKS = 36, X = 40, Y = 48,
                Z = 56, HUNGER = 64, GENDER = 68, VARIANT = 72, HEALTH = 76, HOME_X = 80, HOME_Y = 84, HOME_Z = 88,
                OIL = 92, TARGET_X = 96, TARGET_Y = 104, TARGET_Z = 112, TARGET_PLAYER_MOST = 120,
                TARGET_PLAYER_LEAST = 128, SIZE = 136;
        public static final int FLAG_CLAMP_ABOVE_SURFACE = 1, FLAG_HAS_HOME = 2, FLAG_SLEEPING = 4, FLAG_HOSTILE = 8;
        /** The body carries an oil reserve ({@link #OIL}): its breaths are counted, its fire stops at zero (OG-4, OG-13). */
        public static final int FLAG_OIL = 16;
    }

    /** DRAGON_STEER (aux = flags, 1 settle at the target). */
    public static final class Steer {
        public static final int AGENT = 16, EPOCH = 24, X = 32, Y = 40, Z = 48, SIZE = 56;
        public static final int FLAG_SETTLE = 1;
    }

    /** DRAGON_DEPART (aux = reason: 1 bored, 2 hungry, 3 wounded, 4 passing through, 5 recalled). */
    public static final class Depart {
        public static final int AGENT = 16, EPOCH = 24, TIMEOUT_TICKS = 28, X = 32, Y = 40, Z = 48, SIZE = 56;
    }

    /** DRAGON_ADOPT (aux 0). */
    public static final class Adopt {
        public static final int ENTITY_MOST = 16, ENTITY_LEAST = 24, AGENT = 32, EPOCH = 40, FLAGS = 44,
                HOME_X = 48, HOME_Y = 52, HOME_Z = 56, SIZE = 64;
        public static final int FLAG_HAS_HOME = 1;
    }

    /** DRAGON_DISCARD (aux = reason: 1 the sim owns the population, 2 its lair has its dragon). */
    public static final class Discard {
        public static final int ENTITY_MOST = 16, ENTITY_LEAST = 24, SIZE = 32;
    }

    /**
     * PLACE_LAIR (aux 0): a lair structure (structure dictionary) on the first load of a free site within
     * {@code radius} blocks of (x, z), every biome allowed; at most one per field (OILGATE.md §4.4, OG-7).
     */
    public static final class PlaceLair {
        public static final int FIELD = 16, DIM = 24, STRUCTURE = 28, X = 32, Z = 36, RADIUS = 40, SIZE = 48;
    }

    /** OIL_SURVEY (aux 0): sample the oil of the chunks within {@code radius} blocks, answered OIL_CHUNKS. */
    public static final class OilSurvey {
        public static final int DIM = 16, X = 20, Z = 24, RADIUS = 28, SIZE = 32;
    }

    /**
     * PLACE_NEST (aux 0): a nest whose chamber floor lies {@code depth} blocks (20 to 60) below the surface
     * heightmap in chunk (cx, cz), carved when that chunk is loaded (OG-9e). Idempotent.
     */
    public static final class PlaceNest {
        public static final int NEST = 16, DIM = 24, CX = 28, CZ = 32, DEPTH = 36, SIZE = 40;
    }

    /**
     * NEST_RENEW (aux = count): the nest owes {@code count} more worms of size {@code stage} (1 to 3), when
     * {@code serial} (the engine's count of the nest's renewals with this one in it, an unsigned 32 bits, first 1)
     * is above the last applied under the same engine life (docs/replay-gate.md §7.1, RP-14). Offset 36 is reserved.
     */
    public static final class NestRenew {
        public static final int NEST = 16, DIM = 24, STAGE = 28, SERIAL = 32, RESERVED = 36, SIZE = 40;
    }

    /**
     * NEST_SORTIE (aux = count): up to {@code count} loaded worms burrow to column (x, z) and fight there for
     * {@code timeout} ticks, stopping under an armoured floor.
     */
    public static final class NestSortie {
        public static final int NEST = 16, DIM = 24, X = 28, Z = 32, TIMEOUT_TICKS = 36, SIZE = 40;
    }

    /** The smallest valid size of each command kind; a shorter record is skipped as malformed. */
    public static int minSize(int kind) {
        return switch (kind) {
            case DRAGON_MATERIALIZE -> Materialize.SIZE;
            case DRAGON_STEER -> Steer.SIZE;
            case DRAGON_DEPART -> Depart.SIZE;
            case DRAGON_ADOPT -> Adopt.SIZE;
            case DRAGON_DISCARD -> Discard.SIZE;
            case PLACE_LAIR -> PlaceLair.SIZE;
            case OIL_SURVEY -> OilSurvey.SIZE;
            case PLACE_NEST -> PlaceNest.SIZE;
            case NEST_RENEW -> NestRenew.SIZE;
            case NEST_SORTIE -> NestSortie.SIZE;
            default -> 16;
        };
    }
}
