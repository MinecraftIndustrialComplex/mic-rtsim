package com.minecraftindustrialcomplex.mic_rtsim.topology;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.core.Direction;
import net.minecraft.world.level.Level;

import java.lang.reflect.Method;
import java.util.List;
import java.util.Optional;

/**
 * Toroidal World's shape of a level, read through its public API by reflection (the core does not
 * compile against it): {@code ToroidalWorldApi.shapeOf(level)} and the shape's {@code loops(axis)},
 * {@code minBlock(axis)} and {@code widthBlocks(axis)} (TW 0.19.3, Modrinth project CdjG7l5z). x alone
 * loops: an x wrap; x and z: a torus; z alone: a glued domain with z slides (TP18).
 */
public interface ToroidalWorldProbe {

    /** One axis of a shape: whether it loops, its first block and its width. */
    record Axis(boolean loops, long minBlock, long widthBlocks) {}

    /** A level's two axes, or null when Toroidal World does not shape it (or is absent). */
    Axis[] axes(Level level);

    /** The topology the shape makes, or null when nothing loops. */
    default LevelTopology topologyOf(Level level) {
        Axis[] a = axes(level);
        if (a == null)
            return null;
        return of(a[0], a[1]);
    }

    /** The topology of a shape's x and z axes, or null when neither loops. */
    static LevelTopology of(Axis x, Axis z) {
        if (x.loops() && !z.loops())
            return LevelTopology.xWrap(x.minBlock(), x.widthBlocks(), LevelTopology.Source.TOROIDAL_WORLD);
        if (!x.loops() && !z.loops())
            return null;
        java.util.ArrayList<LevelTopology.Edge> edges = new java.util.ArrayList<>();
        long x0 = x.loops() ? x.minBlock() : Integer.MIN_VALUE / 2, w = x.loops() ? x.widthBlocks() : 0;
        long z0 = z.minBlock(), h = z.widthBlocks();
        if (x.loops()) {
            edges.add(new LevelTopology.Edge(LevelTopology.WEST, Iso.translation(w, 0), false));
            edges.add(new LevelTopology.Edge(LevelTopology.EAST, Iso.translation(-w, 0), false));
        }
        edges.add(new LevelTopology.Edge(LevelTopology.NORTH, Iso.translation(0, h), false));
        edges.add(new LevelTopology.Edge(LevelTopology.SOUTH, Iso.translation(0, -h), false));
        long x1 = x.loops() ? x0 + w : Integer.MAX_VALUE / 2;
        return LevelTopology.glued(x0, z0, x1, z0 + h, List.copyOf(edges), LevelTopology.Source.TOROIDAL_WORLD);
    }

    /** The probe that reads the real mod, when it is loaded. */
    static ToroidalWorldProbe reflective() {
        return new ToroidalWorldProbe() {
            private boolean failed;
            private Method shapeOf, loops, minBlock, widthBlocks;

            private boolean bind() {
                if (shapeOf != null)
                    return true;
                if (failed)
                    return false;
                try {
                    Class<?> api = Class.forName("com.toroidalworld.api.v1.ToroidalWorldApi");
                    Class<?> shape = Class.forName("com.toroidalworld.api.v1.ToroidalShape");
                    loops = shape.getMethod("loops", Direction.Axis.class);
                    minBlock = shape.getMethod("minBlock", Direction.Axis.class);
                    widthBlocks = shape.getMethod("widthBlocks", Direction.Axis.class);
                    shapeOf = api.getMethod("shapeOf", Level.class);
                    return true;
                } catch (ReflectiveOperationException | LinkageError e) {
                    failed = true; // not installed: every level is unshaped
                    return false;
                }
            }

            @Override
            public Axis[] axes(Level level) {
                if (!bind())
                    return null;
                try {
                    Object o = shapeOf.invoke(null, level);
                    if (!(o instanceof Optional<?> opt) || opt.isEmpty())
                        return null;
                    Object s = opt.get();
                    return new Axis[]{axis(s, Direction.Axis.X), axis(s, Direction.Axis.Z)};
                } catch (ReflectiveOperationException | RuntimeException e) {
                    MicRtsim.LOGGER.warn("[mic_rtsim] reading Toroidal World's shape of {} failed: {}", level.dimension().location(), e.toString());
                    return null;
                }
            }

            private Axis axis(Object s, Direction.Axis a) throws ReflectiveOperationException {
                boolean l = (Boolean) loops.invoke(s, a);
                return new Axis(l, l ? ((Number) minBlock.invoke(s, a)).longValue() : 0,
                        l ? ((Number) widthBlocks.invoke(s, a)).longValue() : 0);
            }
        };
    }
}
