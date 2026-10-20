package com.minecraftindustrialcomplex.mic_rtsim.habitat;

/**
 * The world model's records (docs/boundary.md §10), offsets from the start of the record. Mirrors
 * {@code crates/rtsim-space/src/habitat.rs} (the reference codec): the three change together.
 */
public final class HabitatRecords {
    private HabitatRecords() {}

    // ---- kinds (docs/boundary.md §10.1) ---------------------------------------------------------

    /** One sampled 64-block cell (latest wins). */
    public static final int HABITAT = 0x00A0;
    /** What the engine holds of one region (engine to Java, per session). */
    public static final int HABITAT_KNOWN = 0x8900;

    public static boolean isHabitatCommand(int kind) {
        return kind >= 0x8900 && kind <= 0x89FF;
    }

    /** Java's dictionary kind of biome names (docs/boundary.md §10.2). */
    public static final int DICT_BIOME = 0x30;

    // ---- cells (docs/boundary.md §10.3) ---------------------------------------------------------

    /** Cell edge, blocks: 4 x 4 chunks, one sample point at each chunk's centre column. */
    public static final int CELL_BLOCKS = 64;
    public static final int CELL_CHUNKS = 4;
    public static final int POINTS = 16;
    /** Cells per region edge (a 512-block region). */
    public static final int REGION_CELLS = 8;

    /** HABITAT (aux = creature entries n), then n entries of 8 bytes. */
    public static final class Habitat {
        public static final int DIM = 8, FINGERPRINT = 12, CELL_X = 16, CELL_Z = 20, KNOWN = 24, WATER = 26, ICE = 28,
                FLUID = 30, BIOME = 32, LAND_BIOME = 36, TAGS = 40, LAND_TAGS = 44, SURFACE_MIN = 48, SURFACE_MAX = 50,
                SURFACE_MEAN = 52, WATER_MEAN = 54, FLOOR_MEAN = 56, TEMPERATURE = 60, DOWNFALL = 64, CELSIUS = 68,
                SEASON = 72, SEASON_COUNT = 73, CREATURES = 80;
        /** Within a creature entry: u32 body kind (DICT_BODY), u16 share per mille, u16 generated. */
        public static final int CREATURE = 8, C_BODY = 0, C_SHARE = 4, C_GENERATED = 6;

        public static int size(int n) {
            return CREATURES + CREATURE * n;
        }
    }

    /** HABITAT_KNOWN (aux = flags), then n entries of 8 bytes: u8 cell index in the region, 3 reserved, u32 fingerprint. */
    public static final class Known {
        public static final int DIM = 16, REGION_X = 20, REGION_Z = 24, COUNT = 28, ENTRIES = 32, ENTRY = 8;
        /** The last record of this session's list. */
        public static final int FLAG_END = 1;
    }

    /**
     * Biome tag ids by bit (docs/boundary.md §10.3): vanilla's {@code minecraft:} tags and NeoForge's common
     * {@code c:} tags. The Rust side's {@code habitat::tag::NAMES} is the same list.
     */
    public static final String[] TAGS = {
            "minecraft:is_ocean", "minecraft:is_deep_ocean", "minecraft:is_river", "minecraft:is_beach",
            "minecraft:is_mountain", "minecraft:is_hill", "minecraft:is_forest", "minecraft:is_taiga",
            "minecraft:is_jungle", "minecraft:is_savanna", "minecraft:is_badlands", "c:is_swamp", "c:is_desert",
            "c:is_plains", "c:is_snowy", "c:is_cold", "c:is_hot", "c:is_wet", "c:is_dry", "c:is_mushroom",
            "c:is_underground", "minecraft:is_overworld", "minecraft:is_nether", "minecraft:is_end",
            "c:is_sparse_vegetation", "c:is_dense_vegetation", "c:is_mountain/peak", "c:is_mountain/slope",
            "c:is_plateau", "c:is_windswept", "c:is_stony_shores",
    };

    /** Packs a cell into a map key. */
    public static long key(int cx, int cz) {
        return ((long) cx << 32) | (cz & 0xFFFF_FFFFL);
    }

    public static int keyX(long key) {
        return (int) (key >> 32);
    }

    public static int keyZ(long key) {
        return (int) key;
    }
}
