package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

/**
 * The glass box's records (GLASSBOX.md §1.4, Ben's G-B19; {@code docs/industry-boundary.md} §9): the Java
 * mirror of {@code crates/rtsim-industry/src/glass/codec.rs} and {@code records.rs}, pinned by
 * {@code GlassWireContractTest} against fixtures the Rust codec prints. Offsets are from the record start,
 * header included ({@code u16 kind, u16 size, u32 aux}).
 *
 * <p>A compile is sent as latest-wins records with a compile number and a count, as stock sweeps are
 * (F5): IND_GRAPH_NODE, EDGE, RECIPE and DOMAIN records, then IND_GRAPH_END saying how many there were.
 * The engine uses a compile only if it received exactly that many. Every graph record starts
 * {@code 8 i64 gameTime, 16 u64 district, 24 u32 compile, 28 u32 index}.
 */
public final class GlassRecords {
    private GlassRecords() {}

    // ---- kinds ------------------------------------------------------------------------------------
    public static final int IND_GRAPH_NODE = 0x0048;
    public static final int IND_GRAPH_EDGE = 0x0049;
    public static final int IND_GRAPH_RECIPE = 0x004A;
    public static final int IND_GRAPH_DOMAIN = 0x004B;
    public static final int IND_GRAPH_END = 0x004C;
    /** A probed buffer's contents in a sweep; not counted in the sweep's records. */
    public static final int IND_PROBE_SAMPLE = 0x004D;
    /** What was counted at a count set's (mover, store) pairs in a sweep; not counted in the sweep's records. */
    public static final int IND_COUNT_SAMPLE = 0x004F;
    /** Journaled: a district chunk started or stopped block-ticking. */
    public static final int IND_CHUNK_TICKING = 0x040B;
    /** Command: buffers to add to a district's sweeps. */
    public static final int IND_PROBE_SET = 0x8406;
    /** Command: the (mover, store) pairs whose moves Java counts for a district. */
    public static final int IND_COUNT_SET = 0x8408;

    /** {@code Option<u32>} index on the wire: none. */
    public static final int NONE = 0xFFFF_FFFF;

    /** The prefix every graph record (NODE, EDGE, RECIPE, DOMAIN) shares. */
    public static final int GAME_TIME = 8;   // i64
    public static final int DISTRICT = 16;   // u64
    public static final int COMPILE = 24;    // u32
    public static final int INDEX = 28;      // u32

    /** IND_GRAPH_NODE, aux = node kind. */
    public static final class Node {
        public static final int MODEL = 32;      // u32, a Model code
        public static final int DOMAIN = 36;     // u32, NONE
        public static final int INVENTORY = 40;  // u32, NONE
        public static final int FLAGS = 44;      // u32
        public static final int X = 48, Y = 52, Z = 56; // i32, in the compile's frame
        // 60: u32 reserved
        public static final int RULE = 64;       // u32, a Rule code
        public static final int N_RATES = 68;    // u32
        public static final int CAPACITY = 72;   // i64
        public static final int PARAM0 = 80, PARAM1 = 88, PARAM2 = 96, PARAM3 = 104; // i64
        public static final int RATES = 112;     // n x Rate24
        /** Rate24: u32 level, u32 recipe (NONE), u32 batch, u32 ticksNum, u32 ticksDen, u32 flags. */
        public static final int RATE = 24;
        public static final int R_LEVEL = 0, R_RECIPE = 4, R_BATCH = 8, R_TICKS_NUM = 12, R_TICKS_DEN = 16, R_FLAGS = 20;

        public static int size(int rates) {
            return pad8(RATES + RATE * rates);
        }

        // Kinds (aux).
        public static final int STORE = 1, BUFFER = 2, MACHINE = 3, MOVER = 4, JUNCTION = 5, HEATER = 6, SOURCE = 7,
                SINK = 8, BLACK_BOX = 9, CONTROLLER = 10, PORT = 11, BOILER = 12;

        // Flags.
        public static final int BOUNDED = 1, ANY_OUTPUT_BLOCKS = 2, STOCHASTIC = 4, FLUID = 8, CALIBRATED = 16, FRAME = 32, PASS_THROUGH = 64;
        /** A package store's frogport (SECTIONS.md §2.5, slice S4): a store whose chain side is a port. */
        public static final int CHAIN_PORT = 128;

        // Rate flags.
        public static final int RATE_SHARED = 1, RATE_FIRST_CYCLE = 2;
    }

    /** IND_GRAPH_EDGE, aux = edge kind. */
    public static final class Edge {
        public static final int FROM = 32;      // u32
        public static final int TO = 36;        // u32
        public static final int ORDER = 40;     // u32
        public static final int FLAGS = 44;     // u32
        public static final int CAP_NUM = 48;   // u32 units per tick, numerator (0/0: unlimited)
        public static final int CAP_DEN = 52;   // u32
        public static final int LATENCY = 56;   // u32 ticks
        public static final int N_FILTER = 60;  // u32
        public static final int PARAM = 64;     // i64
        public static final int FILTER = 72;    // n x u32 GOOD

        public static int size(int filter) {
            return pad8(FILTER + 4 * filter);
        }

        // Kinds (aux).
        public static final int ADJACENCY = 1, BELT = 2, CHUTE = 3, PIPE = 4, CHAIN = 5, AIR_ZONE = 6, ARM_REACH = 7,
                HEAT = 8, CONTROL = 9, OBSERVE = 10, LINE = 11, DROP = 12;

        // Flags.
        public static final int DENY = 1, FLUID = 2, BOUNDED = 4, FILTERED_SIDE = 8;
        /** A belt's own link to its next element (the through traffic); insertions onto the belt do not wait for it (GB72). */
        public static final int THROUGH = 16;

        // CONTROL edge params.
        public static final long CONTROL_STOP_WHEN_ON = 1, CONTROL_RUN_WHEN_ON = 2;
    }

    /** IND_GRAPH_RECIPE, aux = inputs + outputs. */
    public static final class Recipe {
        public static final int TYPE = 32;       // u32, a RecipeType code
        public static final int HEAT = 36;       // u32, a Heat level (at least)
        public static final int N_INPUTS = 40;   // u32
        public static final int N_OUTPUTS = 44;  // u32
        public static final int DURATION = 48;   // u32
        public static final int NAME = 52;       // u32 (NAME dictionary), 0 none
        public static final int TERMS = 56;      // n x Term24 (inputs, then outputs)
        /** Term24: u32 good, u32 flags, i64 num, i64 den. */
        public static final int TERM = 24;
        public static final int T_GOOD = 0, T_FLAGS = 4, T_NUM = 8, T_DEN = 16;
        public static final int TERM_CATALYST = 1, TERM_CHANCE = 2;

        public static int size(int terms) {
            return TERMS + TERM * terms;
        }
    }

    /** IND_GRAPH_DOMAIN, aux = members. */
    public static final class Domain {
        public static final int NETWORK = 32;       // u64
        public static final int N_LEVELS = 40;      // u32
        public static final int CURRENT = 44;       // u32
        public static final int DRIVER = 48;        // u32, NONE
        public static final int FLAGS = 52;         // u32
        public static final int LIVE_STRESS = 56;   // f64
        public static final int LIVE_CAPACITY = 64; // f64
        public static final int LEVELS = 72;        // n x Level24, then members x Member24
        /** Level24: u32 selector, i32 speedNum, u32 speedDen, u32 reserved, f64 capacity. */
        public static final int LEVEL = 24;
        public static final int L_SELECTOR = 0, L_SPEED_NUM = 4, L_SPEED_DEN = 8, L_CAPACITY = 16;
        /** Member24: u32 node (NONE), u32 count, i32 ratioNum, u32 ratioDen, f64 impact. */
        public static final int MEMBER = 24;
        public static final int M_NODE = 0, M_COUNT = 4, M_RATIO_NUM = 8, M_RATIO_DEN = 12, M_IMPACT = 16;
        public static final int OVERSTRESSED = 1, STRESS_DISABLED = 2;

        public static int size(int levels, int members) {
            return LEVELS + LEVEL * levels + MEMBER * members;
        }

        public static int members(int levels) {
            return LEVELS + LEVEL * levels;
        }
    }

    /** IND_GRAPH_END, aux = flags. */
    public static final class End {
        // 24 u32 compile
        public static final int WINDOW = 28;         // u32: checksum window ticks (0: the engine's default)
        public static final int RECORDS = 32;        // u32
        public static final int NODES = 36;          // u32
        public static final int EDGES = 40;          // u32
        public static final int RECIPES = 44;        // u32
        public static final int DOMAINS = 48;        // u32
        public static final int REASON = 52;         // u32
        public static final int FINGERPRINT = 56;    // u64
        public static final int FRAME = 64;          // u64: 0 the level; else a Sable sub-level's id
        public static final int REVISION = 72;       // u32
        public static final int COMPILE_TICKS = 76;  // u32
        public static final int SIZE = 80;

        public static final int COMPLETE = 1, NOT_COMPILABLE = 2, BLACK_BOX = 4, RESEND = 8;
        // Not-compilable reasons.
        public static final int REASON_NONE = 0, REASON_UNSTABLE = 1, REASON_TOO_BIG = 2, REASON_CONTRAPTION = 3, REASON_OTHER = 9;
    }

    /** IND_PROBE_SAMPLE, aux = n entries. */
    public static final class ProbeSample {
        // 8 i64 gameTime, 16 u64 district
        public static final int SWEEP = 24;     // u32
        public static final int NODE = 28;      // u32
        public static final int COMPILE = 32;   // u32
        // 36 reserved
        public static final int ENTRIES = 40;   // n x Entry16

        public static int size(int n) {
            return ENTRIES + 16 * n;
        }
    }

    /** IND_CHUNK_TICKING (journaled: 8 obsSeq, 16 gameTime), aux = 1 ticking, 0 not. */
    public static final class ChunkTicking {
        public static final int DISTRICT = 24;  // u64
        public static final int DIM = 32;       // u32 (the bridge's dimension dictionary)
        public static final int CHUNK_X = 36;   // i32
        public static final int CHUNK_Z = 40;   // i32
        public static final int SIZE = 48;
    }

    /** IND_PROBE_SET (command: 8 cmdSeq), aux = n nodes. */
    public static final class ProbeSet {
        public static final int DISTRICT = 16;  // u64
        public static final int COMPILE = 24;   // u32
        // 28 reserved
        public static final int NODES = 32;     // n x u32

        public static int size(int n) {
            return pad8(NODES + 4 * n);
        }
    }

    /** IND_COUNT_SAMPLE, aux = n entries (one per pair and good; a pair that counted nothing has good NONE). */
    public static final class CountSample {
        // 8 i64 gameTime, 16 u64 district
        public static final int SWEEP = 24;     // u32
        public static final int COMPILE = 28;   // u32
        public static final int ENTRIES = 32;   // n x Count40
        /**
         * Count40: u32 mover node, u32 store node, u32 good (NONE), u32 entries (how many entries the entry's
         * (mover, store) pair has in all, over every record of the sweep), i64 since, i64 in, i64 out.
         */
        public static final int ENTRY = 40;
        public static final int E_MOVER = 0, E_STORE = 4, E_GOOD = 8, E_ENTRIES = 12, E_SINCE = 16, E_IN = 24, E_OUT = 32;

        public static int size(int n) {
            return ENTRIES + ENTRY * n;
        }
    }

    /** IND_COUNT_SET (command: 8 cmdSeq), aux = n pairs. */
    public static final class CountSet {
        public static final int DISTRICT = 16;  // u64
        public static final int COMPILE = 24;   // u32
        // 28 reserved
        public static final int PAIRS = 32;     // n x Pair8
        /** Pair8: u32 mover node, u32 store node (the named compile's node numbers). */
        public static final int PAIR = 8;
        public static final int P_MOVER = 0, P_STORE = 4;

        public static int size(int n) {
            return pad8(PAIRS + PAIR * n);
        }
    }

    // ---- codes ------------------------------------------------------------------------------------

    /** Model codes (IND_GRAPH_NODE at 32): the design's templates (§7.1) and Create's components. */
    public static final class Model {
        public static final int UNKNOWN = 0, STORE_ITEMS = 1, STORE_FLUID = 2, STORE_ENERGY = 3;
        public static final int MILLSTONE = 10, CRUSHER = 11, PRESS = 12, MIXER = 13, SAW = 14, DEPLOYER = 15,
                FAN_ZONE = 16, CRAFTER = 17, SPOUT = 18, DRAIN = 19, BASIN = 20, TIMED = 21, SEQUENCED = 22;
        public static final int BELT = 30, CHUTE = 31, FUNNEL_ANDESITE = 32, FUNNEL_BRASS = 33, ARM = 34, HOPPER = 35,
                PUMP = 36, TUNNEL_BRASS = 37, TUNNEL_ANDESITE = 38, DEPOT = 39, SMART_CHUTE = 40, PACKAGER = 41,
                CHAIN_CONVEYOR = 42, FROGPORT = 43, PIPE = 44;
        public static final int BLAZE_BURNER = 50, BOILER = 51;
        public static final int THRESHOLD_SWITCH = 60, FACTORY_GAUGE = 61;
        public static final int ANIMAL_SHEEP = 70, SPAWN_SPACE = 71, WATER_SOURCE = 72;
        public static final int BLACK_BOX = 80;
        public static final int SINK_BURN = 90, SINK_VOID = 91;
    }

    /** Routing rules (IND_GRAPH_NODE at 64). 1 to 7 are the brass tunnel's SelectionMode ordinal plus one. */
    public static final class Rule {
        public static final int NONE = 0, SPLIT = 1, FORCED_SPLIT = 2, ROUND_ROBIN = 3, FORCED_ROUND_ROBIN = 4,
                PREFER_NEAREST = 5, RANDOMIZE = 6, SYNCHRONIZE = 7, PREFER_FIRST = 8, MERGE = 9, ANDESITE_SPLIT = 10,
                SIDE_INSERT = 11;
    }

    /** Recipe types (IND_GRAPH_RECIPE at 32). */
    public static final class RecipeType {
        public static final int OTHER = 0, MILLING = 1, CRUSHING = 2, PRESSING = 3, MIXING = 4, COMPACTING = 5,
                CUTTING = 6, DEPLOYING = 7, SPLASHING = 8, SMELTING = 9, BLASTING = 10, SMOKING = 11, HAUNTING = 12,
                CRAFTING = 13, FILLING = 14, EMPTYING = 15, SEQUENCED = 16, FUEL = 17, SHEARING = 18,
                STONECUTTING = 19, ITEM_APPLICATION = 20, TRANSFER = 21, BOILING = 22;
    }

    /** Heat levels: Create's {@code BlazeBurnerBlock.HeatLevel} order; a recipe needs "at least". */
    public static final class Heat {
        public static final int NONE = 0, SMOULDERING = 1, FADING = 2, KINDLED = 3, SEETHING = 4;
        public static final int NEEDS_HEATED = FADING, NEEDS_SUPERHEATED = SEETHING;
    }

    public static int pad8(int n) {
        return (n + 7) & ~7;
    }
}
