package com.minecraftindustrialcomplex.mic_rtsim.topology;

import org.junit.jupiter.api.Test;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.HexFormat;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The TOPOLOGY record against the Rust reference codec, and the bridge's own seam geometry
 * (docs/choices-topology.md). The hex is pinned on both sides: {@code record.rs}'s
 * {@code contract::GUYOU_16K_STOP} asserts the same bytes from the engine's encoder.
 */
class TopologyRecordsTest {

    static final String GUYOU_16K_STOP =
            "b000900001000000020401020000000000e6ffffffffffff00f3ffffffffffff001a000000000000"
            + "000d000000000000000000000000000000340000000000000000000000000000010000000000000000"
            + "ccffffffffffff00000000000000000202010000000000000000000000000000e6ffffffffffff0302"
            + "0100000000000000000000000000001a000000000000";

    @Test
    void guyouRecordIsTheEnginesBytes() {
        LevelTopology t = LevelTopology.guyou(6656, true, LevelTopology.Source.DEEP_TIME);
        int size = TopologyRecords.size(t);
        ByteBuffer b = ByteBuffer.allocate(size).order(ByteOrder.LITTLE_ENDIAN);
        // The header the writer puts in front (kind, size, aux = dimension 1).
        b.putShort(0, (short) TopologyRecords.TOPOLOGY);
        b.putShort(2, (short) size);
        b.putInt(4, 1);
        TopologyRecords.write(b, 0, t);
        assertEquals(GUYOU_16K_STOP, HexFormat.of().formatHex(b.array()));
        // The tracker's preamble body is the same bytes after the header.
        byte[] body = TopologyTracker.body(t);
        byte[] tail = new byte[size - 8];
        System.arraycopy(b.array(), 8, tail, 0, tail.length);
        assertArrayEquals(tail, body);
    }

    @Test
    void anXWrapFoldsAndFindsTheShortWay() {
        LevelTopology t = LevelTopology.xWrap(-6656, 13312, LevelTopology.Source.TOROIDAL_WORLD);
        LevelTopology.Folded f = t.fold(6700.5, 10);
        assertEquals(6700.5 - 13312, f.x(), 1e-9);
        // A body at x = 6650 heads for x = -6650 the short way: 12 blocks east, past the edge.
        double[] i = t.nearestImage(6650, 0, -6650, 0, true);
        assertEquals(6662, i[0], 1e-9);
        assertEquals(12, t.distance(6650, 0, -6650, 0), 1e-9);
    }

    @Test
    void overThePoleAHeadingTurnsAndEdgeStopWallsIt() {
        LevelTopology open = LevelTopology.guyou(6656, false, LevelTopology.Source.DEEP_TIME);
        // Ten blocks past the north edge at x = 1000 is x = -1000, ten blocks inside, turned.
        LevelTopology.Folded f = open.fold(1000, -3328 - 10);
        assertEquals(-1000, f.x(), 1e-9);
        assertEquals(-3328 + 10, f.z(), 1e-9);
        assertEquals(2, f.iso().k());
        // Two points either side of the north seam are close through it...
        assertEquals(4, open.distance(100, -3326, -100, -3326), 1e-9);
        // ...but a body may not cross it while edge_stop holds: the long way round.
        LevelTopology stop = LevelTopology.guyou(6656, true, LevelTopology.Source.DEEP_TIME);
        assertTrue(stop.anyStopped());
        assertTrue(stop.distance(100, -3326, -100, -3326) > 150);
        // East and west still slide.
        assertEquals(12, stop.distance(6650, 0, -6650, 0), 1e-9);
    }

    @Test
    void toroidalWorldShapesBecomeTopologies() {
        ToroidalWorldProbe.Axis none = new ToroidalWorldProbe.Axis(false, 0, 0);
        ToroidalWorldProbe.Axis x = new ToroidalWorldProbe.Axis(true, -6656, 13312);
        assertNull(ToroidalWorldProbe.of(none, none));
        LevelTopology w = ToroidalWorldProbe.of(x, none);
        assertNotNull(w);
        assertEquals(LevelTopology.Kind.X_WRAP, w.kind);
        assertEquals(LevelTopology.Source.TOROIDAL_WORLD, w.source);
        LevelTopology torus = ToroidalWorldProbe.of(x, new ToroidalWorldProbe.Axis(true, -4096, 8192));
        assertEquals(LevelTopology.Kind.GLUED, torus.kind);
        assertEquals(4, torus.edges.size());
        assertEquals(10, torus.distance(0, 4090, 0, -4096 + 4), 1e-9);
    }

    @Test
    void deepTimeLayoutPartsBuildTheGuyouWorld() {
        List<LevelTopology.Edge> edges = LevelTopology.guyou(6656, true, LevelTopology.Source.DEEP_TIME).edges;
        LevelTopology t = DeepTimeLayoutSource.of(-6656, -3328, 6656, 3328, edges);
        assertEquals(LevelTopology.guyou(6656, true, LevelTopology.Source.DEEP_TIME), t);
        // Lambert: two slides alone are an x wrap.
        LevelTopology l = DeepTimeLayoutSource.of(-8192, -2607, 8192, 2608, edges.subList(0, 2).stream()
                .map(e -> new LevelTopology.Edge(e.side(), Iso.translation(e.side() == LevelTopology.WEST ? 16384 : -16384, 0), false))
                .toList());
        assertEquals(LevelTopology.Kind.X_WRAP, l.kind);
        assertFalse(l.anyStopped());
    }
}
