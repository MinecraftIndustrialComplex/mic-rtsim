package com.minecraftindustrialcomplex.mic_rtsim.far;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.HexFormat;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;

/** FAR_LEG as the engine writes it (docs/boundary.md §12.1): the bytes of rtsim-ecology's far.rs test. */
class FarLegsTest {
    /** rtsim-ecology far.rs, {@code the_bytes_java_reads}: encode(42, sample(), body 9, dimension 3, offset 4000). */
    static final String RUST = "008c5000010000002a000000000000000700000000000044090000000300000001020400b400000050fbffff"
            + "2c01000000100000b3ffffff701700000000000034210000000000000000000057000000";

    static final SessionPlugin.EngineNames NAMES = (dict, id) -> {
        if (dict == Obs.DICT_BODY && id == 9)
            return "iceandfire:ice_dragon";
        if (dict == Obs.DICT_DIMENSION && id == 3)
            return "minecraft:overworld";
        return null;
    };

    static ByteBuffer record(String hex) {
        return ByteBuffer.wrap(HexFormat.of().parseHex(hex)).order(ByteOrder.LITTLE_ENDIAN);
    }

    @Test
    void theEnginesBytesReadAsItWroteThem() {
        ByteBuffer b = record(RUST);
        assertEquals(FarLegs.SIZE, b.capacity());
        assertEquals(FarLegs.FAR_LEG, b.getShort(0) & 0xFFFF);
        FarLegs legs = new FarLegs();
        legs.accept(b, 0, FarLegs.FAR_LEG, b.getShort(2) & 0xFFFF, b.getInt(4), NAMES);
        FarLegs.Rec r = legs.get(0x44L << 56 | 7);
        assertNotNull(r, legs.describe());
        assertEquals(FarState.FLY, r.state());
        assertEquals("iceandfire:ice_dragon", r.body());
        assertEquals("minecraft:overworld", r.dimension());
        assertEquals(2, r.variant());
        assertEquals(4, r.stage());
        assertEquals(180, r.y());
        assertEquals(-1200, r.x0());
        assertEquals(300, r.z0());
        assertEquals(4096, r.x1());
        assertEquals(-77, r.z1());
        assertEquals(6_000, r.t0(), "game time: the engine tick less the link's offset");
        assertEquals(8_500, r.t1());
        assertEquals(0, r.radius());
        assertEquals(87, r.ageDays(), "the age in days (DH-7)");
    }

    @Test
    void goneForgetsAndAnUnknownDimensionIsNotKept() {
        FarLegs legs = new FarLegs();
        ByteBuffer b = record(RUST);
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, FarState.FLY.wire, NAMES);
        assertEquals(1, legs.size());
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, FarState.GONE.wire, NAMES);
        assertEquals(0, legs.size());
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, FarState.HUNT.wire, (d, id) -> null);
        assertNull(legs.get(0x44L << 56 | 7));
        // A short record or a state this side does not know is malformed, never kept
        legs.accept(b, 0, FarLegs.FAR_LEG, 40, FarState.FLY.wire, NAMES);
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, 77, NAMES);
        assertEquals(0, legs.size());
    }

    @Test
    void ecologySwitchedOffForgetsEveryLegAndDropsLateOnes() {
        // The engine drops its queue, GONE records included, and resends every dragon when ecology is back on
        FarLegs legs = new FarLegs();
        ByteBuffer b = record(RUST);
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, FarState.FLY.wire, NAMES);
        legs.setEnabled(false);
        assertEquals(0, legs.size());
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, FarState.FLY.wire, NAMES);
        assertEquals(0, legs.size(), "a record still on its way when ecology went off");
        legs.setEnabled(true);
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, FarState.FLY.wire, NAMES);
        assertEquals(1, legs.size());
    }

    @Test
    void aDragonThatStopsFlyingKeepsTheWayItFlew() {
        // An embodied dragon spawns facing its last flown leg's way, whatever its record now is (DHT41)
        FarLegs legs = new FarLegs();
        ByteBuffer b = record(RUST);
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, FarState.FLY.wire, NAMES);
        FarLegs.Rec fly = legs.get(0x44L << 56 | 7);
        double n = Math.hypot(4096 + 1200, -77 - 300);
        assertEquals((4096 + 1200) / n, fly.hx(), 1e-9);
        assertEquals((-77 - 300) / n, fly.hz(), 1e-9);
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, FarState.HUNT.wire, NAMES);
        legs.accept(b, 0, FarLegs.FAR_LEG, FarLegs.SIZE, FarState.ROOST.wire, NAMES);
        FarLegs.Rec roost = legs.get(0x44L << 56 | 7);
        assertEquals(FarState.ROOST, roost.state());
        assertEquals(fly.hx(), roost.hx(), 0.0, "a hunt or a roost does not say which way it flew");
        assertEquals(fly.hz(), roost.hz(), 0.0);
    }

    @Test
    void aStyleSaysHowFarAboveItsFeetABodysMiddleIs() {
        // The engine's y is the feet; the body source samples the middle, so the leg is drawn that much higher
        FarLegs.Style s = new FarLegs.Style(1, new int[]{0x101010}, new double[]{0.4, 1.0, 2.0});
        assertEquals(0.4, s.centre(1), 0.0);
        assertEquals(2.0, s.centre(3), 0.0);
        assertEquals(2.0, s.centre(5), 0.0, "past the last stage known: the last");
        assertEquals(0.4, s.centre(0), 0.0);
        assertEquals(0.0, new FarLegs.Style(1, new int[]{0x101010}).centre(3), 0.0, "none registered");
    }
}
