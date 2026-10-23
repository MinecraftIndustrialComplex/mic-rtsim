package com.minecraftindustrialcomplex.mic_rtsim.far;

import com.minecraftindustrialcomplex.mic_rtsim.topology.Iso;
import com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The far feed's plain-Java parts (docs/boundary.md §12.2, §12.3): the wire codec, the subscriber filter, the
 * per-subscriber diff, and the seam motion a leg is drawn through.
 */
class FarFeedTest {
    static final int ALL = FarKind.DRAGON.bit() | FarKind.CARRIAGE.bit();

    static FarItem carriage(long id, double x, double z) {
        return new FarItem(id, FarKind.CARRIAGE, 0, 0, 0, 0x3A3B3F, FarState.MOVING,
                new double[]{x + 3, 64, z, x - 3, 64, z}, null, -1, 0);
    }

    static FarItem dragon(long id, int x0, int z0, int x1, int z1, long t0, long t1) {
        return new FarItem(id, FarKind.DRAGON, 1, 2, 4, 0x4060C0, FarState.FLY, null,
                new FarItem.Leg(180, x0, z0, x1, z1, t0, t1, 0), -1, 0);
    }

    @Test
    void aMessageRoundTripsAndStaysSmall() {
        List<FarWire.Rec> recs = List.of(
                new FarWire.Gone(7),
                new FarWire.Intro(7, 0x4400_0000_0000_0123L, 1, 2, 3, 4, 0xC04040),
                new FarWire.Leg(7, 1, 190, -2000, 3000, 2500, -1800, 99_000L, 101_400L, 0),
                FarWire.Sample.of(8, 5, new double[]{1000.25, 70.5, -1000.125, 994.0, 70.5, -1000.125}),
                new FarWire.Real(8, 12345),
                new FarWire.Real(9, -1));
        FarWire.Message m = new FarWire.Message(FarWire.RESET, 100_000L, "minecraft:overworld", 10, 64, -20, recs);
        byte[] b = FarWire.encode(m);
        assertEquals(m, FarWire.decode(b));
        FarWire.Sample s = (FarWire.Sample) FarWire.decode(b).recs().get(3);
        assertArrayEquals(new double[]{1000.25, 70.5, -1000.125, 994.0, 70.5, -1000.125}, s.segment(), 1e-9);
        // A sample near the subscriber is a few bytes: type, handle, state, 3 x 3 bytes, 3 x 1 byte
        FarWire.Message one = new FarWire.Message(0, 100_000L, "d", 0, 64, 0,
                List.of(FarWire.Sample.of(3, 5, new double[]{900, 64, 900, 894, 64, 900})));
        int header = FarWire.encode(new FarWire.Message(0, 100_000L, "d", 0, 64, 0, List.of())).length;
        assertTrue(FarWire.encode(one).length - header <= 15, "sample bytes " + (FarWire.encode(one).length - header));
    }

    @Test
    void aBrokenMessageIsRejected() {
        byte[] b = FarWire.encode(new FarWire.Message(0, 5L, "d", 0, 0, 0, List.of(new FarWire.Gone(1))));
        assertThrows(IllegalArgumentException.class, () -> FarWire.decode(java.util.Arrays.copyOf(b, b.length - 1)));
        byte[] extra = java.util.Arrays.copyOf(b, b.length + 1);
        assertThrows(IllegalArgumentException.class, () -> FarWire.decode(extra));
        byte[] v2 = b.clone(); // another version
        v2[0] = FarWire.VERSION + 1;
        assertThrows(IllegalArgumentException.class, () -> FarWire.decode(v2));
    }

    @Test
    void theFilterTakesDragonsFirstThenTheNearestWithinTheRadius() {
        List<FarItem> items = new ArrayList<>();
        for (int k = 0; k < 10; k++)
            items.add(carriage(100 + k, 100.0 * (k + 1), 0));
        items.add(dragon(1, 900, 0, 900, 0, 0, 0));
        items.add(dragon(2, 5000, 0, 5000, 0, 0, 0)); // outside
        List<FarItem> got = FarFilter.select(items, 0, 0, 1000, 4, ALL, 0);
        assertEquals(List.of(1L, 100L, 101L, 102L), got.stream().map(FarItem::id).toList());
        assertEquals(List.of(100L), FarFilter.select(items, 0, 0, 1000, 1, FarKind.CARRIAGE.bit(), 0)
                .stream().map(FarItem::id).toList());
        // A leg's position is where it is now
        FarItem flying = dragon(3, -5000, 0, 5000, 0, 0, 1000);
        assertEquals(1, FarFilter.select(List.of(flying), 0, 0, 100, 4, ALL, 500).size());
        assertEquals(0, FarFilter.select(List.of(flying), 0, 0, 100, 4, ALL, 100).size());
    }

    @Test
    void theBodyWinsOverTheEnginesLeg() {
        FarItem leg = dragon(5, 0, 0, 100, 0, 0, 100);
        FarItem body = new FarItem(5, FarKind.DRAGON, 1, 2, 4, 0x4060C0, FarState.BODY,
                new double[]{1, 70, 0, 0, 70, 0}, null, 77, 10);
        assertEquals(List.of(body), FarFilter.dedupe(List.of(leg, body)));
        assertEquals(List.of(body), FarFilter.dedupe(List.of(body, leg)));
    }

    @Test
    void theViewSendsOnlyWhatChanged() {
        FarView v = new FarView();
        FarItem a = carriage(100, 500, 0), b = dragon(1, 0, 0, 1000, 0, 0, 1000);
        List<FarWire.Rec> first = v.step(List.of(a, b));
        assertEquals(4, first.size(), first.toString()); // two intros, a sample and a leg
        assertTrue(v.step(List.of(a, b)).isEmpty(), "nothing moved");
        // Less than half an eighth of a block is not a move; a block is
        assertTrue(v.step(List.of(carriage(100, 500.04, 0), b)).isEmpty());
        List<FarWire.Rec> moved = v.step(List.of(carriage(100, 501, 0), b));
        assertEquals(1, moved.size());
        assertInstanceOf(FarWire.Sample.class, moved.get(0));
        // The engine sends a new leg; the body appears with an entity id (the hand-over)
        FarItem body = new FarItem(1, FarKind.DRAGON, 1, 2, 4, 0x4060C0, FarState.BODY,
                new double[]{1001, 180, 0, 1000, 180, 0}, null, 4242, 10);
        List<FarWire.Rec> hand = v.step(List.of(carriage(100, 501, 0), body));
        assertEquals(3, hand.size(), hand.toString());
        assertEquals(moved.get(0), hand.get(0), "the carriage stood: its sample once more, to stop it");
        assertInstanceOf(FarWire.Sample.class, hand.get(1));
        assertEquals(new FarWire.Real(1, 4242), hand.get(2));
        // The carriage leaves: held for the grace (a newcomer takes a new handle), then GONE first, and the next
        // newcomer reuses its handle
        FarItem c200 = carriage(200, 0, 0);
        List<FarWire.Rec> swap = v.step(List.of(body, c200));
        assertTrue(swap.stream().noneMatch(r -> r instanceof FarWire.Gone), swap.toString());
        assertEquals(hand.get(1), swap.get(0), "the body stood: its sample once more");
        assertEquals(2, ((FarWire.Intro) swap.get(1)).handle());
        for (int i = 1; i < FarView.GRACE_SWEEPS; i++)
            assertTrue(v.step(List.of(body, c200)).isEmpty(), "held");
        assertEquals(3, v.size());
        List<FarWire.Rec> gone = v.step(List.of(body, c200, carriage(300, 9, 0)));
        assertEquals(new FarWire.Gone(0), gone.get(0));
        assertEquals(0, ((FarWire.Intro) gone.get(1)).handle());
        assertEquals(3, v.size());
    }

    @Test
    void aMoverThatStopsIsSentOnceMoreThenNotAgain() {
        FarView v = new FarView();
        v.step(List.of(carriage(100, 500, 0)));
        assertTrue(v.step(List.of(carriage(100, 500, 0))).isEmpty(), "never moved: nothing to stop");
        List<FarWire.Rec> moved = v.step(List.of(carriage(100, 520, 0)));
        assertEquals(1, moved.size());
        assertEquals(moved, v.step(List.of(carriage(100, 520, 0))), "stood: the same sample once more");
        assertTrue(v.step(List.of(carriage(100, 520, 0))).isEmpty(), "and then nothing");
        assertEquals(1, v.step(List.of(carriage(100, 540, 0))).size(), "off again");
    }

    @Test
    void anEntityJoiningIsSentAtOnceAndNotAgainAtTheSweep() {
        FarView v = new FarView();
        assertEquals(null, v.real(100, 42), "an object this view does not hold");
        v.step(List.of(carriage(100, 500, 0)));
        assertEquals(new FarWire.Real(0, 42), v.real(100, 42));
        assertEquals(null, v.real(100, 42), "already said");
        FarItem withEntity = new FarItem(100, FarKind.CARRIAGE, 0, 0, 0, 0x3A3B3F, FarState.MOVING,
                new double[]{503, 64, 0, 497, 64, 0}, null, 42, 0);
        assertTrue(v.step(List.of(withEntity)).isEmpty(), "the sweep's item says the same");
        assertEquals(List.of(new FarWire.Real(0, -1)), v.step(List.of(carriage(100, 500, 0))), "gone again");
    }

    @Test
    void aDragonMissingForASweepComesBackOnItsHandle() {
        // A captured body is in neither source until the engine's leg lands (the hand-over's gap)
        FarView v = new FarView();
        FarItem body = new FarItem(1, FarKind.DRAGON, 1, 2, 4, 0x4060C0, FarState.FLY,
                new double[]{1001, 180, 0, 1000, 180, 0}, null, 4242, 10);
        assertEquals(3, v.step(List.of(body)).size()); // intro, sample, entity
        assertTrue(v.step(List.of()).isEmpty(), "held, not GONE");
        List<FarWire.Rec> back = v.step(List.of(dragon(1, 1000, 0, 1400, 0, 0, 1000)));
        assertTrue(back.stream().noneMatch(r -> r instanceof FarWire.Intro || r instanceof FarWire.Gone),
                back.toString());
        assertTrue(back.stream().allMatch(r -> r.handle() == 0), back.toString());
        assertTrue(back.stream().anyMatch(r -> r instanceof FarWire.Leg), back.toString());
        assertTrue(back.contains(new FarWire.Real(0, -1)), back.toString());
    }

    @Test
    void aLegAcrossASeamIsDrawnThroughIt() {
        // Toroidal World's x lap of 13312: a subscriber at x = 6600 sees a dragon flying at x = -6600
        // 112 blocks east, past the edge, and the whole leg moves with it.
        LevelTopology t = LevelTopology.xWrap(-6656, 13312, LevelTopology.Source.TOROIDAL_WORLD);
        Iso iso = t.nearestImageIso(6600, 0, -6600, 0);
        assertEquals(Iso.translation(13312, 0), iso);
        FarItem moved = dragon(1, -6600, 0, -6500, 50, 0, 100).moved(iso);
        assertEquals(6712, moved.leg().x0());
        assertEquals(6812, moved.leg().x1());
        assertEquals(50, moved.leg().z1());
        // Over Deep Time's north pole, the image is turned: both ends turn with it
        LevelTopology g = LevelTopology.guyou(6656, false, LevelTopology.Source.DEEP_TIME);
        Iso pole = g.nearestImageIso(100, -3320, -100, -3320);
        double[] p = pole.point(-100, -3320);
        assertEquals(t.distance(0, 0, 0, 0), 0.0);
        double dx = p[0] - 100, dz = p[1] + 3320;
        assertTrue(Math.sqrt(dx * dx + dz * dz) < 20, "the near image, got " + p[0] + ", " + p[1]);
        double[] q = g.nearestImage(100, -3320, -100, -3320, false);
        assertArrayEquals(q, p, 1e-9);
        assertEquals(Iso.IDENTITY, LevelTopology.flat().nearestImageIso(0, 0, 1e6, 0));
    }
}
