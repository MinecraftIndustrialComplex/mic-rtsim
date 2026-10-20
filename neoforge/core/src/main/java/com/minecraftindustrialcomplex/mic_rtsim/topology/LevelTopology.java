package com.minecraftindustrialcomplex.mic_rtsim.topology;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Objects;
import java.util.Set;

/**
 * How a level's edges are joined (docs/choices-topology.md; the engine's
 * {@code rtsim_space::topology::Topology}): flat (vanilla, with the world border), an x wrap
 * (Toroidal World alone), or a glued domain (Deep Time's Petroff–Guyou layout). The bridge sends it to
 * the engine as a TOPOLOGY record and uses it itself to steer puppets: Toroidal World and Deep Time's
 * wrap engine fold an entity that crosses an edge back into the domain without an event, so every
 * target a body walks to is taken as its image nearest the body ({@link #nearestImage}), and a jump
 * across a seam is a short step, never a teleport (TP11).
 *
 * <p>Immutable. Positions are entity points (doubles): x in [x0, x1), z in [z0, z1] for a glued
 * domain.
 */
public final class LevelTopology {

    public enum Kind { FLAT, X_WRAP, GLUED }

    /** Who described it (the record's source byte). */
    public enum Source { DEFAULT, TOROIDAL_WORLD, DEEP_TIME, PLUGIN }

    public static final int WEST = 0, EAST = 1, NORTH = 2, SOUTH = 3;

    /** One glued edge: the motion beyond to inside, and whether edge_stop holds there. */
    public record Edge(int side, Iso iso, boolean stopped) {}

    public final Kind kind;
    public final Source source;
    /** Flat: the world border; x wrap: the lap and the border's z; glued: the domain. */
    public final boolean hasRect;
    public final long x0, z0, x1, z1;
    public final List<Edge> edges;
    /** Images of the domain touching it (canonical to tile), identity first. */
    private final List<Iso> adjacent;
    /** The same with stopped seams as walls: what bodies may cross. */
    private final List<Iso> adjacentEmbodied;

    private LevelTopology(Kind kind, Source source, boolean hasRect, long x0, long z0, long x1, long z1, List<Edge> edges) {
        this.kind = kind;
        this.source = source;
        this.hasRect = hasRect;
        this.x0 = x0;
        this.z0 = z0;
        this.x1 = x1;
        this.z1 = z1;
        this.edges = List.copyOf(edges);
        this.adjacent = kind == Kind.GLUED ? tiles(false) : xImages();
        this.adjacentEmbodied = kind == Kind.GLUED ? tiles(true) : this.adjacent;
    }

    public static LevelTopology flat() {
        return new LevelTopology(Kind.FLAT, Source.DEFAULT, false, 0, 0, 0, 0, List.of());
    }

    public static LevelTopology flatWithBorder(long x0, long z0, long x1, long z1) {
        return new LevelTopology(Kind.FLAT, Source.DEFAULT, true, x0, z0, x1, z1, List.of());
    }

    /** x wraps over [x0, x0 + period); z unbounded. */
    public static LevelTopology xWrap(long x0, long period, Source source) {
        return new LevelTopology(Kind.X_WRAP, source, true, x0, Integer.MIN_VALUE, x0 + period, Integer.MAX_VALUE, List.of());
    }

    /** A glued domain [x0, x1) x [z0, z1] with its edges. */
    public static LevelTopology glued(long x0, long z0, long x1, long z1, List<Edge> edges, Source source) {
        return new LevelTopology(Kind.GLUED, source, true, x0, z0, x1, z1, edges);
    }

    /** Petroff–Guyou of hemisphere side b (FORMAT.md "The world's layout"), the engine's {@code layout::guyou}. */
    public static LevelTopology guyou(long b, boolean edgeStop, Source source) {
        long h = b / 2;
        return glued(-b, -h, b, h, List.of(
                new Edge(WEST, Iso.translation(2 * b, 0), false),
                new Edge(EAST, Iso.translation(-2 * b, 0), false),
                new Edge(NORTH, Iso.halfTurnAbout(0, -h), edgeStop),
                new Edge(SOUTH, Iso.halfTurnAbout(0, h), edgeStop)), source);
    }

    public boolean isFlat() {
        return kind == Kind.FLAT;
    }

    public boolean anyStopped() {
        for (Edge e : edges)
            if (e.stopped)
                return true;
        return false;
    }

    /** The same topology from another source (the record says who). */
    public LevelTopology withSource(Source s) {
        return new LevelTopology(kind, s, hasRect, x0, z0, x1, z1, edges);
    }

    // ---- geometry ---------------------------------------------------------------------------------------

    /** A point brought into the domain: {x, z} and the motion that did it. */
    public record Folded(double x, double z, Iso iso) {}

    private Edge edge(int side) {
        for (Edge e : edges)
            if (e.side == side)
                return e;
        return null;
    }

    /** Brings a point into the domain through the edges (clamped past a wall). */
    public Folded fold(double x, double z) {
        switch (kind) {
            case FLAT:
                return new Folded(x, z, Iso.IDENTITY);
            case X_WRAP: {
                long p = x1 - x0;
                long n = (long) Math.floor((x - x0) / p);
                return new Folded(x - n * p, z, Iso.translation(-n * p, 0));
            }
            default:
                break;
        }
        Iso g = Iso.IDENTITY;
        for (int i = 0; i < 4096; i++) {
            int side;
            if (x < x0)
                side = WEST;
            else if (x >= x1)
                side = EAST;
            else if (z < z0)
                side = NORTH;
            else if (z > z1)
                side = SOUTH;
            else
                return new Folded(x, z, g);
            Edge e = edge(side);
            if (e == null) {
                x = Math.max(x0, Math.min(x, x1 - 1e-9));
                z = Math.max(z0, Math.min(z, z1));
                return new Folded(x, z, g);
            }
            double[] p = e.iso.point(x, z);
            if (p[0] == x && p[1] == z)
                return new Folded(x, z, g); // a point on a turned edge is its own image
            x = p[0];
            z = p[1];
            g = e.iso.compose(g);
        }
        return new Folded(Math.max(x0, Math.min(x, x1 - 1e-9)), Math.max(z0, Math.min(z, z1)), g);
    }

    /**
     * The image of {@code (toX, toZ)} nearest {@code (fromX, fromZ)}, in the frame of {@code from}: where
     * a body at {@code from} walks to reach {@code to} the short way. With {@code embodied}, never
     * through a seam under edge_stop (the long way round instead). Exact for targets nearer than the
     * domain's narrow side, which every steering target is.
     */
    public double[] nearestImage(double fromX, double fromZ, double toX, double toZ, boolean embodied) {
        if (kind == Kind.FLAT)
            return new double[]{toX, toZ};
        Folded f = fold(fromX, fromZ), t = fold(toX, toZ);
        double best = Double.POSITIVE_INFINITY;
        double[] img = null;
        for (Iso h : embodied ? adjacentEmbodied : adjacent) {
            double[] q = h.point(t.x, t.z);
            double dx = q[0] - f.x, dz = q[1] - f.z;
            double d2 = dx * dx + dz * dz;
            if (d2 < best) {
                best = d2;
                img = q;
            }
        }
        return f.iso.inverse().point(img[0], img[1]);
    }

    /** The shortest displacement from one point to another, through the seams bodies may cross. */
    public double[] displacement(double fromX, double fromZ, double toX, double toZ) {
        double[] i = nearestImage(fromX, fromZ, toX, toZ, true);
        return new double[]{i[0] - fromX, i[1] - fromZ};
    }

    public double distance(double fromX, double fromZ, double toX, double toZ) {
        double[] d = displacement(fromX, fromZ, toX, toZ);
        return Math.hypot(d[0], d[1]);
    }

    private List<Iso> xImages() {
        if (kind != Kind.X_WRAP)
            return List.of(Iso.IDENTITY);
        long p = x1 - x0;
        return List.of(Iso.IDENTITY, Iso.translation(-p, 0), Iso.translation(p, 0));
    }

    /** The motions of the tiles touching the domain, breadth first over the (open) edges. */
    private List<Iso> tiles(boolean embodied) {
        List<Iso> out = new ArrayList<>();
        Set<String> seen = new HashSet<>();
        ArrayDeque<Iso> queue = new ArrayDeque<>();
        out.add(Iso.IDENTITY);
        seen.add(key(Iso.IDENTITY));
        queue.add(Iso.IDENTITY);
        while (!queue.isEmpty()) {
            Iso here = queue.poll();
            for (Edge e : edges) {
                if (embodied && e.stopped)
                    continue;
                Iso next = here.compose(e.iso.inverse());
                if (!touches(next) || !seen.add(key(next)))
                    continue;
                out.add(next);
                queue.add(next);
            }
        }
        return List.copyOf(out);
    }

    private static String key(Iso i) {
        return i.k() + ":" + i.tx() + ":" + i.tz();
    }

    /** Whether the tile {@code h} makes of the domain touches it. */
    private boolean touches(Iso h) {
        double[] a = h.point(x0, z0), b = h.point(x1, z1);
        double ax = Math.min(a[0], b[0]), bx = Math.max(a[0], b[0]);
        double az = Math.min(a[1], b[1]), bz = Math.max(a[1], b[1]);
        return ax <= x1 && bx >= x0 && az <= z1 && bz >= z0;
    }

    // ---- identity -----------------------------------------------------------------------------------------

    @Override
    public boolean equals(Object o) {
        return o instanceof LevelTopology t && kind == t.kind && hasRect == t.hasRect && x0 == t.x0 && z0 == t.z0
                && x1 == t.x1 && z1 == t.z1 && edges.equals(t.edges);
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind, hasRect, x0, z0, x1, z1, edges);
    }

    @Override
    public String toString() {
        return switch (kind) {
            case FLAT -> hasRect ? "flat inside the border [" + x0 + ", " + x1 + ") x [" + z0 + ", " + z1 + ")" : "flat";
            case X_WRAP -> "x wraps every " + (x1 - x0) + " blocks from " + x0;
            case GLUED -> "glued over [" + x0 + ", " + x1 + ") x [" + z0 + ", " + z1 + "], " + edges.size() + " edges"
                    + (anyStopped() ? ", edge_stop" : "");
        };
    }
}
