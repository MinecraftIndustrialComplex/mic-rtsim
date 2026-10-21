package com.minecraftindustrialcomplex.mic_rtsim.topology;

import java.nio.ByteBuffer;

/**
 * The TOPOLOGY record (docs/boundary.md §11), offsets from the start of the record. Mirrors
 * {@code crates/rtsim-space/src/topology/record.rs} (the reference codec): the three change together.
 *
 * <p>Latest wins, aux = the dimension (Java's dictionary id). Sent for every level at a session's
 * first hand-over, before any journaled record, and again when a level's topology changes.
 */
public final class TopologyRecords {
    private TopologyRecords() {}

    public static final int TOPOLOGY = 0x00B0;

    /** u8 kind: 0 flat, 1 x wrap, 2 glued. */
    public static final int KIND = 8;
    /** u8 edge count n (at most 4). */
    public static final int EDGES_N = 9;
    /** u8 flags: {@link #FLAG_RECT}. */
    public static final int FLAGS = 10;
    /** u8 source: 0 the bridge's default, 1 Toroidal World, 2 Deep Time's layout, 3 a plugin. */
    public static final int SOURCE = 11;
    /** i64 x0, z0, x1, z1: flat, the world border; x wrap, the lap and the border's z; glued, the domain. */
    public static final int X0 = 16, Z0 = 24, X1 = 32, Z1 = 40;
    public static final int EDGES = 48;
    /** Per edge: u8 side (0 west, 1 east, 2 north, 3 south), u8 quarter turns, u8 flags, 5 reserved, i64 tx, i64 tz. */
    public static final int EDGE = 24, E_SIDE = 0, E_TURNS = 1, E_FLAGS = 2, E_TX = 8, E_TZ = 16;

    public static final int FLAG_RECT = 1;
    public static final int EDGE_STOPPED = 1;

    public static int size(int n) {
        return EDGES + EDGE * n;
    }

    /** The record's size for {@code t}. */
    public static int size(LevelTopology t) {
        return size(Math.min(4, t.edges.size()));
    }

    /** Writes {@code t}'s fields into a reserved record at {@code at} (the header is the writer's). */
    public static void write(ByteBuffer b, int at, LevelTopology t) {
        int n = Math.min(4, t.edges.size());
        b.put(at + KIND, (byte) t.kind.ordinal());
        b.put(at + EDGES_N, (byte) n);
        b.put(at + FLAGS, (byte) (t.hasRect ? FLAG_RECT : 0));
        b.put(at + SOURCE, (byte) t.source.ordinal());
        b.putInt(at + 12, 0);
        if (t.hasRect) {
            b.putLong(at + X0, t.x0);
            b.putLong(at + Z0, t.z0);
            b.putLong(at + X1, t.x1);
            b.putLong(at + Z1, t.z1);
        } else {
            for (int o = X0; o < EDGES; o += 8)
                b.putLong(at + o, 0);
        }
        for (int i = 0; i < n; i++) {
            LevelTopology.Edge e = t.edges.get(i);
            int o = at + EDGES + EDGE * i;
            b.put(o + E_SIDE, (byte) e.side());
            b.put(o + E_TURNS, (byte) e.iso().k());
            b.put(o + E_FLAGS, (byte) (e.stopped() ? EDGE_STOPPED : 0));
            for (int r = 3; r < 8; r++)
                b.put(o + r, (byte) 0);
            b.putLong(o + E_TX, e.iso().tx());
            b.putLong(o + E_TZ, e.iso().tz());
        }
    }
}
