package com.minecraftindustrialcomplex.mic_rtsim.herd;

/**
 * The herd records of docs/boundary.md §8, offsets from the start of the record. Mirrors
 * {@code crates/rtsim-ecology/src/herds/boundary.rs} (the reference codec): the three change together.
 * Journaled observations carry {@code obsSeq} at 8 and {@code gameTime} at 16 (the core's journal fills
 * both); commands carry {@code cmdSeq} at 8.
 */
public final class HerdRecords {
    private HerdRecords() {}

    // ---- kinds (docs/boundary.md §8.1) ----------------------------------------------------------

    public static final int HERD_GROUP_STATE = 0x0080;
    public static final int HERD_MATERIALIZED = 0x0700;
    public static final int HERD_SPAWN_REJECTED = 0x0701;
    public static final int HERD_DEMATERIALIZED = 0x0702;
    public static final int HERD_KILLED = 0x0703;
    public static final int HERD_SHEARED = 0x0704;
    public static final int HERD_TAKEN = 0x0705;
    public static final int HERD_OBSTACLE = 0x0706;

    public static final int HERD_MATERIALIZE = 0x8700;
    public static final int HERD_STEER = 0x8701;
    public static final int HERD_DESPAWN = 0x8702;

    public static boolean isHerdCommand(int kind) {
        return kind >= 0x8700 && kind <= 0x87FF;
    }

    /** A herd group's agent id carries namespace byte 0x48 ('H', docs/boundary.md §8.2). */
    public static final long AGENT_NAMESPACE = 0x48L;

    public static boolean isHerdAgent(long agent) {
        return (agent >>> 56) == AGENT_NAMESPACE;
    }

    /** Member flags (HERD_MATERIALIZE member blocks and HERD_DEMATERIALIZED captures). */
    public static final int MEMBER_SHEARED = 1;

    // ---- observations ---------------------------------------------------------------------------

    /** HERD_GROUP_STATE (latest-wins, aux = flags). Every {@link HerdBridge#STATE_INTERVAL} ticks per group. */
    public static final class GroupState {
        public static final int AGENT = 8, EPOCH = 16, DIM = 20, PRESENT = 24, X = 32, Y = 40, Z = 48, SIZE = 56;
        public static final int FLAG_TICKING = 1;
    }

    /** HERD_MATERIALIZED (aux = members spawned). */
    public static final class Materialized {
        public static final int AGENT = 24, EPOCH = 32, DIM = 36, SIZE = 40;
    }

    /** HERD_SPAWN_REJECTED (aux = reason: docs/boundary.md §2.3's codes, plus 9 unknown species). */
    public static final class SpawnRejected {
        public static final int AGENT = 24, EPOCH = 32, CMD_SEQ = 40, SIZE = 48;
        public static final int NOT_TICKING = 1, UNKNOWN_DIMENSION = 3, ALREADY_EMBODIED = 4, SPAWN_FAILED = 5,
                BAD_RECORD = 6, DISCARDED_BY_SERVER = 7, UNKNOWN_SPECIES = 9;
    }

    /** HERD_DEMATERIALIZED (aux = reason, docs/boundary.md §2.3's codes), then n captures of 24 bytes. */
    public static final class Dematerialized {
        public static final int AGENT = 24, EPOCH = 32, COUNT = 36, CAPTURES = 40;
        public static final int CAPTURE = 24;
        /** Within a capture. */
        public static final int C_MEMBER = 0, C_FLAGS = 8, C_HEALTH = 12, C_X = 16, C_Z = 20;
        public static final int COMMAND = 1, NOT_TICKING = 2, UNLOADED = 3, SESSION_END = 4, MEMBERS_GONE = 5, NOT_EMBODIED = 6;

        public static int size(int n) {
            return CAPTURES + CAPTURE * n;
        }
    }

    /** HERD_KILLED (aux = killer kind). Journaled at the end of the tick, once the death is certain. */
    public static final class Killed {
        public static final int AGENT = 24, EPOCH = 32, MEMBER = 40, KILLER_MOST = 48, KILLER_LEAST = 56, OP_ID = 64, SIZE = 72;
    }

    /** Who killed an animal (aux of HERD_KILLED). */
    public static final int BY_NONE = 0, BY_PLAYER = 1, BY_MOB = 2, BY_OTHER = 3;

    /** HERD_SHEARED (aux 0). */
    public static final class Sheared {
        public static final int AGENT = 24, EPOCH = 32, MEMBER = 40, PLAYER_MOST = 48, PLAYER_LEAST = 56, SIZE = 64;
    }

    /** HERD_TAKEN (aux = how: {@code Obs.Taken}'s codes). */
    public static final class Taken {
        public static final int AGENT = 24, EPOCH = 32, MEMBER = 40, PLAYER_MOST = 48, PLAYER_LEAST = 56, OP_ID = 64, SIZE = 72;
    }

    /** HERD_OBSTACLE (aux = members that failed to pass). */
    public static final class Obstacle {
        public static final int AGENT = 24, EPOCH = 32, PRESENT = 36, STUCK_X = 40, STUCK_Z = 44, TARGET_X = 48, TARGET_Z = 52, SIZE = 56;
    }

    // ---- commands -------------------------------------------------------------------------------

    /** HERD_MATERIALIZE (aux = members m), then m member blocks of 40 bytes. */
    public static final class Materialize {
        public static final int AGENT = 16, EPOCH = 24, SPECIES = 28, DIM = 32, FLAGS = 36, CENTRE_X = 40, CENTRE_Z = 44,
                TARGET_X = 48, TARGET_Z = 52, SPEED = 56, RADIUS = 60, MEMBERS = 64;
        public static final int MEMBER = 40;
        /** Within a member block. */
        public static final int M_MEMBER = 0, M_SEED = 8, M_VARIANT = 16, M_VANILLA_AGE = 20, M_AGE_DAYS = 24, M_COHORT = 28,
                M_FLAGS = 29, M_X = 32, M_Z = 36;
        /** Each member stands on the surface at its own x, z. */
        public static final int FLAG_SURFACE = 1;

        public static int size(int m) {
            return MEMBERS + MEMBER * m;
        }
    }

    /** HERD_STEER (aux 0). */
    public static final class Steer {
        public static final int AGENT = 16, EPOCH = 24, SPEED = 28, TARGET_X = 32, TARGET_Z = 36, ARRIVE = 40, SIZE = 48;
    }

    /** HERD_DESPAWN (aux = reason: 1 left view, 2 the player left, 3 recalled). */
    public static final class Despawn {
        public static final int AGENT = 16, EPOCH = 24, SIZE = 32;
    }

    /** The smallest a command of this kind can be (a shorter one is malformed). */
    public static int minSize(int kind) {
        return switch (kind) {
            case HERD_MATERIALIZE -> Materialize.MEMBERS;
            case HERD_STEER -> Steer.SIZE;
            case HERD_DESPAWN -> Despawn.SIZE;
            default -> 16;
        };
    }
}
