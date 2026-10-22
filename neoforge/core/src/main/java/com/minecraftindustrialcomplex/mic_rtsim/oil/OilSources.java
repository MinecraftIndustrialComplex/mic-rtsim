package com.minecraftindustrialcomplex.mic_rtsim.oil;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.OilSource;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.structure.BoundingBox;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;

/**
 * The registered {@link OilSource}s (OILGATE.md §4.1, OG-2), first registered first asked, and Deep
 * Time's oil traps read by reflection (the trap chunks of a Deep Time level, known before anything
 * generates).
 */
public final class OilSources {
    private OilSources() {}

    private static final List<OilSource> SOURCES = new CopyOnWriteArrayList<>();

    public static void register(OilSource source) {
        SOURCES.add(source);
    }

    public static List<OilSource> all() {
        return List.copyOf(SOURCES);
    }

    /** The first source that answers for {@code level}, or null. */
    public static OilSource forLevel(ServerLevel level) {
        for (OilSource s : SOURCES)
            if (s.covers(level))
                return s;
        return null;
    }

    // ---- Deep Time (source 1), by reflection ----------------------------------------------------

    static final String SITES = "com.minecraftindustrialcomplex.deeptime.api.DeepTimeSites";
    private static boolean deepTimeFailed;

    /**
     * The chunks Deep Time's petroleum traps cover in {@code level} ({@code DeepTimeSites.of(level).all()},
     * the sites whose extraction carries a CDG charge, every chunk of each site's bounds), or an empty
     * list for a level that is not Deep Time's or without Deep Time. UNTESTED: no GameTest server loads
     * Deep Time; any reflective failure is logged once and gives an empty list.
     */
    public static List<ChunkPos> deepTimeTrapChunks(ServerLevel level) {
        List<ChunkPos> out = new ArrayList<>();
        if (deepTimeFailed)
            return out;
        try {
            Class<?> sitesClass;
            try {
                sitesClass = Class.forName(SITES);
            } catch (ClassNotFoundException e) {
                return out; // Deep Time is not installed
            }
            Object sites = sitesClass.getMethod("of", ServerLevel.class).invoke(null, level);
            if (!(Boolean) sitesClass.getMethod("isDeepTimeLevel").invoke(sites))
                return out;
            List<?> all = (List<?>) sitesClass.getMethod("all").invoke(sites);
            for (Object site : all) {
                Method extraction = site.getClass().getMethod("extraction");
                Object ex = extraction.invoke(site);
                Object charge = ex.getClass().getMethod("cdgChargeMb").invoke(ex);
                if (!(Boolean) charge.getClass().getMethod("isPresent").invoke(charge))
                    continue;
                BoundingBox b = (BoundingBox) site.getClass().getMethod("bounds").invoke(site);
                for (int cx = b.minX() >> 4; cx <= b.maxX() >> 4; cx++)
                    for (int cz = b.minZ() >> 4; cz <= b.maxZ() >> 4; cz++)
                        out.add(new ChunkPos(cx, cz));
            }
        } catch (ReflectiveOperationException | RuntimeException e) {
            deepTimeFailed = true;
            MicRtsim.LOGGER.warn("[mic_rtsim] Deep Time's oil traps could not be read ({}); the oil gate falls back to the chunk sampler", e.toString());
            out.clear();
        }
        return out;
    }
}
