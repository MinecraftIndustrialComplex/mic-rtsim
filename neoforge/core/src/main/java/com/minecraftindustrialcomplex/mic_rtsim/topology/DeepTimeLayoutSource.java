package com.minecraftindustrialcomplex.mic_rtsim.topology;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.TopologySource;
import net.minecraft.server.level.ServerLevel;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;

/**
 * Deep Time's world layout as a topology (TP19): the plugin hook the core registers when Deep Time is
 * loaded. Deep Time has no public layout API (its coordinator, 2026-10-20: "use the internal route
 * behind our deeptime plugin hook"; both mods are unreleased), so this reads, by reflection,
 * {@code GeologicChunkGenerator.terrain()} (a {@code WorldDataTerrain}), its {@code data()} (a
 * {@code WorldData}) and that's {@code layout()} (a {@code WorldLayout} record, null on a Lambert world,
 * which Toroidal World's x wrap then describes): the domain and every edge's {@code [quarterTurns, tx,
 * tz]}, as the manifest's {@code projection} block holds them (FORMAT.md "The world's layout", stable
 * from format 0.5). Edge_stop holds while {@code EdgeBorder.stopZ(level)} is a number (Deep Time:
 * always, unless {@code -Ddeeptime.wrapengine=true} installed the topology); it then covers the edges
 * the layout's border names.
 *
 * <p>Any reflective failure (a renamed class, a changed record) is logged once and the level falls back
 * to Toroidal World's shape: the sim then sees the x wrap but not the turned seams.
 */
public final class DeepTimeLayoutSource implements TopologySource {

    private static final String GENERATOR = "com.minecraftindustrialcomplex.deeptime.worldgen.GeologicChunkGenerator";
    private static final String BORDER = "com.minecraftindustrialcomplex.deeptime.wrap.EdgeBorder";

    private boolean warned;

    @Override
    public String name() {
        return "Deep Time's layout";
    }

    @Override
    public LevelTopology topologyOf(ServerLevel level) {
        Object gen = level.getChunkSource().getGenerator();
        if (!gen.getClass().getName().equals(GENERATOR))
            return null;
        try {
            Object terrain = call(gen, "terrain");
            if (terrain == null || !terrain.getClass().getSimpleName().equals("WorldDataTerrain"))
                return null;
            Object data = call(terrain, "data");
            Object layout = data == null ? null : call(data, "layout");
            if (layout == null)
                return null; // Lambert: Toroidal World's x wrap is the whole topology
            boolean stopping = !Double.isNaN(stopZ(level));
            return fromLayout(layout, stopping);
        } catch (ReflectiveOperationException | RuntimeException | LinkageError e) {
            if (!warned) {
                warned = true;
                MicRtsim.LOGGER.warn("[mic_rtsim] Deep Time's layout of {} could not be read ({}); the sim sees only Toroidal World's x wrap there",
                        level.dimension().location(), e.toString());
            }
            return null;
        }
    }

    /** {@code EdgeBorder.stopZ(level)}: NaN when no edge_stop border holds. */
    private static double stopZ(ServerLevel level) throws ReflectiveOperationException {
        Class<?> b = Class.forName(BORDER);
        Method m = b.getMethod("stopZ", ServerLevel.class);
        return ((Number) m.invoke(null, level)).doubleValue();
    }

    private static Object call(Object o, String method) throws ReflectiveOperationException {
        Method m = o.getClass().getMethod(method);
        return m.invoke(o);
    }

    /**
     * The topology of a {@code WorldLayout} record, read by its accessors: {@code xMin()}, {@code xMax()},
     * {@code zMin()}, {@code zMax()}, {@code edges()} (records with {@code name()}, {@code axisX()},
     * {@code sideMin()}, {@code quarterTurns()}, {@code tx()}, {@code tz()}) and {@code border()} (with
     * {@code stops(name)}), stopped only while {@code stopping}.
     */
    static LevelTopology fromLayout(Object layout, boolean stopping) throws ReflectiveOperationException {
        long x0 = (long) ((Number) call(layout, "xMin")).doubleValue();
        long x1 = (long) ((Number) call(layout, "xMax")).doubleValue();
        long z0 = (long) ((Number) call(layout, "zMin")).doubleValue();
        long z1 = (long) ((Number) call(layout, "zMax")).doubleValue();
        Object border = call(layout, "border");
        List<LevelTopology.Edge> edges = new ArrayList<>();
        for (Object e : (List<?>) call(layout, "edges")) {
            String name = (String) call(e, "name");
            boolean axisX = (Boolean) call(e, "axisX");
            boolean min = (Boolean) call(e, "sideMin");
            int side = axisX ? (min ? LevelTopology.WEST : LevelTopology.EAST) : (min ? LevelTopology.NORTH : LevelTopology.SOUTH);
            int k = ((Number) call(e, "quarterTurns")).intValue();
            long tx = ((Number) call(e, "tx")).longValue(), tz = ((Number) call(e, "tz")).longValue();
            boolean stopped = stopping && border != null
                    && (Boolean) border.getClass().getMethod("stops", String.class).invoke(border, name);
            edges.add(new LevelTopology.Edge(side, new Iso(k, tx, tz), stopped));
        }
        return of(x0, z0, x1, z1, edges);
    }

    /** The topology of a layout's parts (a glued domain; two x slides alone are an x wrap). */
    public static LevelTopology of(long x0, long z0, long x1, long z1, List<LevelTopology.Edge> edges) {
        boolean slidesOnly = edges.size() == 2 && edges.stream().allMatch(e -> e.iso().k() == 0 && e.iso().tz() == 0 && !e.stopped()
                && (e.side() == LevelTopology.WEST || e.side() == LevelTopology.EAST));
        if (slidesOnly)
            return LevelTopology.xWrap(x0, x1 - x0, LevelTopology.Source.DEEP_TIME);
        return LevelTopology.glued(x0, z0, x1, z1, edges, LevelTopology.Source.DEEP_TIME);
    }
}
