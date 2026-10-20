package com.minecraftindustrialcomplex.mic_rtsim.boundary;

/**
 * Command records: the engine writes them, Java reads and applies them. Kinds are 0x8000 and up.
 *
 * <p>Every command starts with a command sequence number at offset 8: unique and increasing per
 * world. Java remembers the last one it applied in the journal and skips any command at or below it
 * (PLAN.md §4.9). Ids of dimensions, items and body kinds in commands refer to the engine's own
 * dictionary, defined by {@link #DICT} commands; it is per session, so the engine sends it again
 * after every session start.
 */
public final class Cmd {
    private Cmd() {}

    public static final int DICT = 0x8001;
    public static final int SPAWN_GROUP = 0x8002;
    public static final int DESPAWN_GROUP = 0x8003;
    public static final int STEER_GROUP = 0x8004;
    public static final int TRANSFER_TO_CONTAINER = 0x8005;

    /** u64. Command sequence number. */
    public static final int CMD_SEQ = 8;
    /** First byte of a command's body. */
    public static final int BODY = 16;

    /** DICT, aux = dictionary kind ({@link Obs#DICT_DIMENSION} and so on). */
    public static final class Dict {
        public static final int ID = 16;       // u32, from 1
        public static final int NAME_LEN = 20; // u32
        public static final int NAME = 24;     // UTF-8, zero-padded to 8

        public static int size(int nameBytes) {
            return Layout.align(NAME + nameBytes);
        }
    }

    /**
     * SPAWN_GROUP, aux = member count. Materialize a group as one unit (C25): every member in the same
     * tick, or none. Answered by MATERIALIZED or SPAWN_REJECTED.
     */
    public static final class SpawnGroup {
        public static final int AGENT = 16;      // u64
        public static final int EPOCH = 24;      // u32, the new materialization epoch
        public static final int GROUP_KIND = 28; // u32 engine dictionary id (DICT_BODY)
        public static final int DIM = 32;        // u32 engine dictionary id (DICT_DIMENSION)
        public static final int FLAGS = 36;      // u32 reserved, zero
        public static final int X = 40;          // f64, the group's anchor
        public static final int Y = 48;          // f64
        public static final int Z = 56;          // f64
        public static final int MEMBERS = 64;    // aux x Member, each followed by its items

        /** A member block: 24 bytes, then ITEM_COUNT x {@link Obs.Item} (items use engine ids). */
        public static final class Member {
            public static final int INDEX = 0;      // u32, position in the group kind
            public static final int ITEM_COUNT = 4; // u32
            public static final int HEALTH = 8;     // f32, 0 = the body's full health
            public static final int BODY_A = 12;    // i32, -1 = let the body roll it
            public static final int BODY_B = 16;    // i32, -1 = let the body roll it
            // 20: u32 reserved
            public static final int ITEMS = 24;

            public static int size(int items) {
                return ITEMS + items * Obs.Item.SIZE;
            }
        }
    }

    /** DESPAWN_GROUP. Capture every member and remove the group. Answered by DEMATERIALIZED. */
    public static final class DespawnGroup {
        public static final int AGENT = 16;  // u64
        public static final int EPOCH = 24;  // u32
        public static final int REASON = 28; // u32, engine-defined, echoed nowhere
        public static final int SIZE = 32;
    }

    /** STEER_GROUP. Point the group's leader at a position; the members follow it. */
    public static final class SteerGroup {
        public static final int AGENT = 16; // u64
        public static final int EPOCH = 24; // u32
        public static final int SPEED = 28; // f32, a multiplier; 0 = the body's own speed
        public static final int X = 32;     // f64
        public static final int Y = 40;     // f64
        public static final int Z = 48;     // f64
        public static final int SIZE = 56;
    }

    /**
     * TRANSFER_TO_CONTAINER. Put items into the container at a block position: a cross-store transfer
     * from the engine into Minecraft. Answered by COMMAND_RESULT. Java applies an op id at most once.
     */
    public static final class TransferToContainer {
        public static final int OP_ID = 16; // u64
        public static final int DIM = 24;   // u32 engine dictionary id
        public static final int ITEM = 28;  // u32 engine dictionary id
        public static final int X = 32;     // i32
        public static final int Y = 36;     // i32
        public static final int Z = 40;     // i32
        public static final int COUNT = 44; // i32
        public static final int SIZE = 48;
    }
}
