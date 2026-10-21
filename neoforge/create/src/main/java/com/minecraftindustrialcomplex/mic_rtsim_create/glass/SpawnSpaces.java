package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

/**
 * The SPAWN_SPACE closed form (GLASSBOX.md §7.1, model 71): how often vanilla's natural spawner puts a
 * monster on a dark spawning floor, from {@code NaturalSpawner.spawnCategoryForChunk} and
 * {@code spawnCategoryForPosition} (Minecraft 1.21.1). Each tick, for each chunk a player keeps spawning
 * and a category under its mob cap, one attempt starts at a column picked uniformly from the chunk's 256
 * and a y uniform in {@code [minY, surface + 1]} (WORLD_SURFACE heightmap); it then makes three packs of
 * {@code ceil(nextFloat() * 4)} tries each, each try a step of {@code nextInt(6) - nextInt(6)} in x and z at
 * the same y, and stops at the mob's {@code getMaxSpawnClusterSize()} spawns (4 for most monsters).
 *
 * <p>UNVERIFIED and not used by the G1 compile: there is no detector of spawning floors yet, so a mob
 * farm stays in the residual black box (choice GB54). The form ignores the mob cap, the 24-block player
 * distance, light, the walk leaving the floor and the farm's own kill time: for a floor that fills its
 * chunk layer it is an upper bound of the spawn rate.
 */
public final class SpawnSpaces {
    private SpawnSpaces() {}

    /**
     * Attempts per tick that start on the floor's layer inside its footprint, for one chunk: {@code (n /
     * 256) / (surface + 2 - minY)} with n the floor's spawnable blocks in that chunk.
     */
    public static double startsPerTick(int floorBlocksInChunk, int surfaceY, int minY) {
        int span = surfaceY + 2 - minY;
        if (span <= 0 || floorBlocksInChunk <= 0)
            return 0;
        return Math.min(256, floorBlocksInChunk) / 256.0 / span;
    }

    /**
     * Spawns per started attempt when every try lands on a valid spot: {@code E[min(T, cap)]} with T the
     * sum of three {@code ceil(nextFloat() * 4)} ({@code nextFloat} is {@code k / 2^24}, so 0 has
     * probability {@code 2^-24} and 4 one less than a quarter).
     */
    public static double spawnsPerStart(int maxClusterSize) {
        double q = 1.0 / (1 << 24);
        double[] one = {q, 0.25, 0.25, 0.25, 0.25 - q};
        double[] t = {1};
        for (int pack = 0; pack < 3; pack++) {
            double[] n = new double[t.length + 4];
            for (int a = 0; a < t.length; a++)
                for (int b = 0; b < one.length; b++)
                    n[a + b] += t[a] * one[b];
            t = n;
        }
        double e = 0;
        for (int k = 0; k < t.length; k++)
            e += t[k] * Math.min(k, maxClusterSize);
        return e;
    }

    /** Ticks per spawn, the rate entry's period: {@code 1 / (startsPerTick × spawnsPerStart)}; infinite at 0. */
    public static double ticksPerSpawn(int floorBlocksInChunk, int surfaceY, int minY, int maxClusterSize) {
        double r = startsPerTick(floorBlocksInChunk, surfaceY, minY) * spawnsPerStart(maxClusterSize);
        return r <= 0 ? Double.POSITIVE_INFINITY : 1 / r;
    }
}
