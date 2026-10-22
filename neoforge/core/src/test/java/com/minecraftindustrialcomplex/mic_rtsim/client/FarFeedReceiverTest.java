package com.minecraftindustrialcomplex.mic_rtsim.client;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarWire;
import java.util.List;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

/**
 * The client end of the hand-over (research/distant-horizons-2026-10-21.md §5): one far mover from the engine's
 * leg to the body's samples and back, moving on from where it was drawn, carrying the body's entity id only
 * while it has one.
 */
class FarFeedReceiverTest {
    private static final String DIM = "minecraft:overworld";
    private static final long ID = 0x44L << 56 | 77;
    private final FarFeedReceiver receiver = new FarFeedReceiver();

    @AfterEach
    void reset() {
        FarObjects.clearAll();
    }

    private void send(int flags, long gameTime, FarWire.Rec... recs) {
        receiver.accept(FarWire.encode(new FarWire.Message(flags, gameTime, DIM, 0, 64, 0, List.of(recs))));
    }

    private static FarMover mover() {
        FarMover found = null;
        for (FarMover m : FarObjects.movers())
            if (m.key().source().equals(FarFeedClient.SOURCE)) {
                assertNull(found, "one mover");
                found = m;
            }
        return found;
    }

    private static double centreX(FarMover m, double at) {
        double[] s = new double[6];
        m.segmentAt(at, s);
        return (s[0] + s[3]) / 2.0;
    }

    private static void advance(int ticks) {
        for (int i = 0; i < ticks; i++)
            FarClock.advance();
    }

    @Test
    void aDragonMovesOnFromWhereItWasDrawnAcrossTheHandOver() {
        long g0 = 1_000;
        int fly = FarState.FLY.wire;
        // The engine's leg: 400 blocks east in 400 ticks
        send(FarWire.RESET, g0, new FarWire.Intro(0, ID, FarKind.DRAGON.wire, 2, 1, 3, 0x445566),
                new FarWire.Leg(0, fly, 100, 0, 0, 400, 0, g0, g0 + 400, 0));
        FarMover leg = mover();
        assertNotNull(leg);
        assertEquals(-1, leg.entity());
        assertEquals(FarState.FLY, leg.state());
        double t1 = FarClock.ticks() + 100;
        assertEquals(100.5, centreX(leg, t1), 1e-6);

        // Embodied at x 103: the mover starts from where it was drawn and reaches the sample in a second
        advance(100);
        double now = FarClock.ticks();
        double drawn = centreX(leg, now);
        send(0, g0 + 100, FarWire.Sample.of(0, fly, new double[]{103.5, 100, 0.5, 102.5, 100, 0.5}),
                new FarWire.Real(0, 42));
        FarMover body = mover();
        assertEquals(leg.key(), body.key(), "the same mover");
        assertEquals(42, body.entity(), "it carries the body's entity id");
        assertEquals(drawn, centreX(body, now), 1e-6, "no jump at the hand-over");
        assertEquals(103.0, centreX(body, now + 20), 1e-6, "a second later it is at the sample");

        // Back to the engine: its leg starts at x 120, with no entity
        advance(20);
        send(0, g0 + 120, new FarWire.Leg(0, fly, 100, 120, 0, 400, 0, g0 + 120, g0 + 400, 0),
                new FarWire.Real(0, -1));
        FarMover back = mover();
        assertEquals(leg.key(), back.key(), "still the same mover");
        assertEquals(-1, back.entity(), "no entity once the body is gone");
        assertEquals(120.5, centreX(back, FarClock.ticks()), 1e-6);
        assertEquals(400.5, centreX(back, FarClock.ticks() + 280), 1e-6);

        send(0, g0 + 121, new FarWire.Gone(0));
        assertNull(mover(), "gone");
    }

    @Test
    void aBrokenMessageForgetsEverything() {
        send(FarWire.RESET, 1, new FarWire.Intro(0, ID, FarKind.DRAGON.wire, 0, 0, 1, 0),
                new FarWire.Leg(0, FarState.ROOST.wire, 70, 5, 5, 5, 5, 0, 0, 0));
        assertNotNull(mover());
        assertEquals(1, receiver.held());
        byte[] broken = FarWire.encode(new FarWire.Message(0, 2, DIM, 0, 64, 0, List.of(new FarWire.Gone(0))));
        assertThrows(IllegalArgumentException.class,
                () -> receiver.accept(java.util.Arrays.copyOf(broken, broken.length - 1)));
        assertNull(mover());
        assertEquals(0, receiver.held());
    }

    @Test
    void aRecordForAHandleNeverIntroducedIsIgnored() {
        send(FarWire.RESET, 1, new FarWire.Leg(3, FarState.FLY.wire, 70, 0, 0, 10, 0, 0, 20, 0),
                new FarWire.Real(3, 9));
        assertNull(mover());
    }

    @Test
    void aLegKeepsToTheServersClockWhenItRunsSlow() {
        long g0 = 5_000;
        send(FarWire.RESET, g0, new FarWire.Intro(0, ID, FarKind.DRAGON.wire, 0, 0, 2, 0x808080),
                new FarWire.Leg(0, FarState.FLY.wire, 100, 0, 0, 200, 0, g0, g0 + 200, 0));
        // 100 client ticks pass while the server's clock, as vanilla syncs it, moves 80
        advance(100);
        receiver.clock(FarClock.ticks() - (g0 + 80));
        assertEquals(1, receiver.retimes);
        assertEquals(80.5, centreX(mover(), FarClock.ticks()), 1e-6, "where the server's clock has it");
        // A wobble under two ticks does not time it again
        receiver.clock(FarClock.ticks() - (g0 + 81));
        assertEquals(1, receiver.retimes);
    }

    @Test
    void aHuntingDragonSpiralsOntoItsCircleTheWayItWasFlying() {
        long g0 = 5_000;
        // North (+z) for 50 ticks to x 90, z 0; then a hunt around x 100, z 0, radius 30
        send(FarWire.RESET, g0, new FarWire.Intro(0, ID, FarKind.DRAGON.wire, 2, 1, 3, 0x445566),
                new FarWire.Leg(0, FarState.FLY.wire, 100, 90, -50, 90, 0, g0, g0 + 50, 0));
        advance(50);
        double now = FarClock.ticks();
        double[] was = new double[6];
        mover().segmentAt(now, was);
        send(0, g0 + 50, new FarWire.Leg(0, FarState.HUNT.wire, 100, 100, 0, 100, 0, g0 + 50, g0 + 2000, 30));
        FarMover hunt = mover();
        assertEquals(FarState.HUNT, hunt.state());
        double[] s = new double[6];
        hunt.segmentAt(now, s);
        assertEquals((was[0] + was[3]) / 2.0, (s[0] + s[3]) / 2.0, 1e-6, "no jump in x");
        assertEquals((was[2] + was[5]) / 2.0, (s[2] + s[5]) / 2.0, 1e-6, "no jump in z");
        hunt.segmentAt(now + 1, s);
        assertTrue((s[2] + s[5]) / 2.0 > (was[2] + was[5]) / 2.0, "it turns on north, as it was flying");
        hunt.segmentAt(now + FarFeedReceiver.JOIN_TICKS, s);
        double dx = (s[0] + s[3]) / 2.0 - 100.5, dz = (s[2] + s[5]) / 2.0 - 0.5;
        assertEquals(30.0, Math.sqrt(dx * dx + dz * dz), 1e-6, "on the circle once joined");
    }
}
