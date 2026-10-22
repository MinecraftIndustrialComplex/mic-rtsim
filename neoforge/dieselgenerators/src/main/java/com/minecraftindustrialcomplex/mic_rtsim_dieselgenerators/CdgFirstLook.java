package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators;

import com.jesz.createdieselgenerators.CDGConfig;
import com.jesz.createdieselgenerators.CDGTags;
import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.oil.OilSources;
import net.minecraft.core.Holder;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeManager;
import net.neoforged.fml.ModList;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.LinkedHashSet;
import java.util.Set;

/**
 * A cheap first look before Create: Diesel Generators' {@code getBaseOilAmount} (OGC31; Ben, 2026-10-21,
 * DECISIONS.md OGB-5). CDG reads 12,800 biomes a chunk ({@code getBiomesInChunk}: every block column of the
 * chunk, y 60 to 109, through {@code ServerLevel.getBiome}), and under the pack's rule (CDG's normal
 * multiplier 0: only a chunk with a biome in {@code #createdieselgenerators:oil_biomes} holds oil) most chunks
 * read all 12,800 to answer 0.
 *
 * <p>{@code BiomeManager.getBiome} gives a block the biome of one of the 8 noise cells (quarts) around it:
 * cell {@code (x - 2) >> 2} or the next one, and the same for y and z. So the cells x
 * {@code (minX - 2) >> 2} to {@code ((maxX - 2) >> 2) + 1}, and likewise for z and for y 60 to 109, hold
 * every biome CDG's points can read: 6 by 14 by 6, 504 lookups through the level's own
 * {@code getNoiseBiome} (what {@code getBiome} reads, in memory or not). A chunk with no oil biome among them
 * holds no oil and is answered 0; any other gets the exact call.
 *
 * <p>The look stands aside, and every chunk gets the exact call, whenever its answer could differ: CDG's
 * config lets chunks without an oil biome hold oil, a KubeJS script answers CDG's oil-chunk event (which
 * replaces CDG's own rule), or Deep Time is installed (its mixin answers at the head of
 * {@code getBaseOilAmount}). Server thread.
 */
public final class CdgFirstLook {
    private CdgFirstLook() {}

    /** CDG's biome points: y 60 up to, not including, 110 ({@code getBiomesInChunk}). */
    public static final int MIN_Y = 60, MAX_Y = 109;
    /** CDG's base amount before its multipliers ({@code getBaseOilAmount}'s 7,000,000). */
    static final double BASE = 7_000_000.0;
    static final String KUBEJS_PLUGIN = "com.jesz.createdieselgenerators.compat.kubejs.CDGKubeJSPlugin";

    /** Chunks the first look answered for, and of those the ones it answered 0 without the exact call. */
    public static long looked, skipped;

    private static boolean kubeJsBroken;
    private static Field oilChunks;
    private static Method hasListeners;

    /** Whether the first look may answer now. */
    public static boolean applies() {
        return normalChunksHoldNone() && !kubeJsAnswers() && !OilSources.deepTimeInstalled();
    }

    /**
     * Whether CDG gives no oil to a chunk without an oil biome: normal chunks switched off, or their amount
     * ({@code 7,000,000 x} the normal multiplier, as an int, times a square) at most 0 under a threshold above
     * 0, which {@code getBaseOilAmount} then answers 0.
     */
    public static boolean normalChunksHoldNone() {
        if (CDGConfig.DISABLE_NORMAL_OIL_CHUNKS.get())
            return true;
        int normal = (int) (BASE * CDGConfig.OIL_MULTIPLIER.get());
        return normal <= 0 && CDGConfig.OIL_CHUNK_THRESHOLD.get() > 0;
    }

    /**
     * Whether a KubeJS script answers CDG's oil-chunk event ({@code CDGKubeJSPlugin.OIL_CHUNKS} has listeners),
     * read by reflection each time (a reload can add one). False without KubeJS; true when it cannot be read.
     */
    static boolean kubeJsAnswers() {
        if (!ModList.get().isLoaded("kubejs"))
            return false;
        if (kubeJsBroken)
            return true;
        try {
            if (hasListeners == null) {
                Field f = Class.forName(KUBEJS_PLUGIN).getField("OIL_CHUNKS");
                hasListeners = f.getType().getMethod("hasListeners");
                oilChunks = f;
            }
            Object handler = oilChunks.get(null);
            return handler == null || (Boolean) hasListeners.invoke(handler);
        } catch (ReflectiveOperationException | RuntimeException | LinkageError e) {
            kubeJsBroken = true;
            MicRtsim.LOGGER.warn("[mic_rtsim_dieselgenerators] CDG's KubeJS oil event could not be read ({}); every chunk gets getBaseOilAmount", e.toString());
            return true;
        }
    }

    /** Whether any noise cell CDG's points in {@code chunk} can read is an oil biome. */
    public static boolean mayHoldOil(BiomeManager.NoiseBiomeSource biomes, ChunkPos chunk) {
        int x0 = (chunk.getMinBlockX() - 2) >> 2, x1 = ((chunk.getMaxBlockX() - 2) >> 2) + 1;
        int y0 = (MIN_Y - 2) >> 2, y1 = ((MAX_Y - 2) >> 2) + 1;
        int z0 = (chunk.getMinBlockZ() - 2) >> 2, z1 = ((chunk.getMaxBlockZ() - 2) >> 2) + 1;
        for (int qx = x0; qx <= x1; qx++)
            for (int qz = z0; qz <= z1; qz++)
                for (int qy = y0; qy <= y1; qy++)
                    if (biomes.getNoiseBiome(qx, qy, qz).is(CDGTags.OIL_BIOMES))
                        return true;
        return false;
    }

    /** The biomes of those noise cells (for the GameTest's check that they hold every biome CDG reads). */
    public static Set<Holder<Biome>> cellBiomes(BiomeManager.NoiseBiomeSource biomes, ChunkPos chunk) {
        Set<Holder<Biome>> out = new LinkedHashSet<>();
        int x0 = (chunk.getMinBlockX() - 2) >> 2, x1 = ((chunk.getMaxBlockX() - 2) >> 2) + 1;
        int y0 = (MIN_Y - 2) >> 2, y1 = ((MAX_Y - 2) >> 2) + 1;
        int z0 = (chunk.getMinBlockZ() - 2) >> 2, z1 = ((chunk.getMaxBlockZ() - 2) >> 2) + 1;
        for (int qx = x0; qx <= x1; qx++)
            for (int qz = z0; qz <= z1; qz++)
                for (int qy = y0; qy <= y1; qy++)
                    out.add(biomes.getNoiseBiome(qx, qy, qz));
        return out;
    }
}
