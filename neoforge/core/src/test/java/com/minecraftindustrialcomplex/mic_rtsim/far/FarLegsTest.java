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
            + "2c01000000100000b3ffffff701700000000000034210000000000000000000000000000";

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
}
