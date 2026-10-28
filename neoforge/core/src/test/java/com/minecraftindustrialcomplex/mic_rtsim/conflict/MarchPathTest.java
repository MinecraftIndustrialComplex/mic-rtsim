package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin.EngineNames;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology;
import org.junit.jupiter.api.Test;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.HashMap;
import java.util.HexFormat;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * BAND_MARCH's record and the position rule it carries (MZB-454, MZB-468, MZB-477). The record's hex is pinned on
 * both sides: {@code boundary.rs}'s {@code SAMPLE_MARCH_HEX} asserts the same bytes from the engine's encoder.
 * The vectors are the ones the Rust tests run through {@code chart_pos} and {@code party_pos}.
 */
class MarchPathTest {
    /** Rust's sample march, with seq 1: agent 5, revision 4, out with the chief, units (1, 8) (2, 4), 3 waypoints. */
    static final String SAMPLE_MARCH =
            "0b888800030000000100000000000000050000000000000004000000010000004d0000000000000002000000030201"
            + "000000000000000000600900000000000002000000000000000100000008000000020000000400000084030000e8030000"
            + "600900000000000052030000c00300001c0c000000000000bc0200000c0300002823000000000000";

    static final EngineNames NAMES = (kind, id) -> switch (kind) {
        case Obs.DICT_DIMENSION -> id == 1 ? "minecraft:overworld" : null;
        case Obs.DICT_BODY -> id == 1 ? "mowziesmobs:foliaath" : id == 2 ? "mowziesmobs:umvuthana_raptor" : null;
        case Obs.DICT_CULTURE -> id == 2 ? "mic_rtsim:conflict/goblin" : null;
        default -> null;
    };

    private static MarchPath sample() {
        byte[] raw = HexFormat.of().parseHex(SAMPLE_MARCH);
        ByteBuffer b = ByteBuffer.wrap(raw).order(ByteOrder.LITTLE_ENDIAN);
        assertEquals(ConflictRecords.BAND_MARCH, b.getShort(0) & 0xFFFF);
        assertEquals(raw.length, b.getShort(2) & 0xFFFF);
        assertEquals(ConflictRecords.March.size(2, 3), raw.length);
        return MarchPath.decode(b, 0, raw.length, b.getInt(4), NAMES);
    }

    private static MarchPath on(int phase, int flags, long[]... waypoints) {
        int n = waypoints.length;
        int[] xs = new int[n];
        int[] zs = new int[n];
        long[] ticks = new long[n];
        for (int i = 0; i < n; i++) {
            xs[i] = (int) waypoints[i][0];
            zs[i] = (int) waypoints[i][1];
            ticks[i] = waypoints[i][2];
        }
        return new MarchPath(5, 1, "minecraft:overworld", 77, "mic_rtsim:conflict/goblin", 3, phase, flags, 0, 0,
                new HashMap<>(), xs, zs, ticks);
    }

    private static long[] w(long x, long z, long t) {
        return new long[] {x, z, t};
    }

    @Test
    void theEnginesRecordDecodesToItsFields() {
        MarchPath m = sample();
        assertNotNull(m);
        assertEquals(5, m.agent());
        assertEquals(4, m.revision());
        assertEquals("minecraft:overworld", m.dimension());
        assertEquals(77, m.faction());
        assertEquals("mic_rtsim:conflict/goblin", m.culture());
        assertEquals(3, m.kind());
        assertEquals(ConflictRecords.March.OUT, m.phase());
        assertTrue(m.chief());
        assertFalse(m.ended());
        assertFalse(m.joint());
        assertFalse(m.stopped());
        assertEquals(0, m.partner());
        assertEquals(2_400, m.musterEnd());
        assertEquals(8, m.count("mowziesmobs:foliaath"));
        assertEquals(4, m.count("mowziesmobs:umvuthana_raptor"));
        assertEquals(0, m.count("mowziesmobs:grottol"));
        assertEquals(3, m.waypoints());
        assertEquals(900, m.waypointX(0));
        assertEquals(1000, m.waypointZ(0));
        assertEquals(2_400, m.waypointTick(0));
        assertEquals(850, m.waypointX(1));
        assertEquals(960, m.waypointZ(1));
        assertEquals(3_100, m.waypointTick(1));
        assertEquals(700, m.waypointX(2));
        assertEquals(780, m.waypointZ(2));
        assertEquals(9_000, m.waypointTick(2));
        // The first tick is the first waypoint; the last is the last.
        assertArrayEquals(new int[] {900, 1000}, m.chartAt(2_400));
        assertArrayEquals(new int[] {700, 780}, m.chartAt(9_000));
    }

    @Test
    void aRecordShorterThanItsCountsIsMalformed() {
        byte[] raw = HexFormat.of().parseHex(SAMPLE_MARCH);
        ByteBuffer b = ByteBuffer.wrap(raw).order(ByteOrder.LITTLE_ENDIAN);
        // One waypoint short of what aux and the unit count say.
        assertNull(MarchPath.decode(b, 0, raw.length - 16, 3, NAMES));
        // Fewer than the fixed part.
        assertNull(MarchPath.decode(b, 0, ConflictRecords.March.UNIT_LIST - 8, 3, NAMES));
        // More waypoints than the record holds, and the most a u32 can say (no overflow).
        assertNull(MarchPath.decode(b, 0, raw.length, 4, NAMES));
        assertNull(MarchPath.decode(b, 0, raw.length, -1, NAMES));
        b.putInt(ConflictRecords.March.UNITS, -1);
        assertNull(MarchPath.decode(b, 0, raw.length, 3, NAMES));
        b.putInt(ConflictRecords.March.UNITS, 2);
        // A phase out of 1 to 5.
        b.put(ConflictRecords.March.PHASE, (byte) 0);
        assertNull(MarchPath.decode(b, 0, raw.length, 3, NAMES));
        b.put(ConflictRecords.March.PHASE, (byte) 6);
        assertNull(MarchPath.decode(b, 0, raw.length, 3, NAMES));
        b.put(ConflictRecords.March.PHASE, (byte) 2);
        assertNotNull(MarchPath.decode(b, 0, raw.length, 3, NAMES));
        // A record with no waypoints and no units is whole.
        MarchPath none = MarchPath.decode(b.putInt(ConflictRecords.March.UNITS, 0), 0, ConflictRecords.March.UNIT_LIST, 0, NAMES);
        assertNotNull(none);
        assertEquals(0, none.waypoints());
        assertNull(none.chartAt(100));
    }

    @Test
    void aNameTheDictionaryLacksLeavesThatEntryOut() {
        byte[] raw = HexFormat.of().parseHex(SAMPLE_MARCH);
        ByteBuffer b = ByteBuffer.wrap(raw).order(ByteOrder.LITTLE_ENDIAN);
        // The second unit's body is an id the dictionary has no name for; the culture is none.
        b.putInt(ConflictRecords.March.UNIT_LIST + ConflictRecords.March.UNIT, 9);
        b.putInt(ConflictRecords.March.CULTURE, 0);
        b.putInt(ConflictRecords.March.DIM, 7);
        MarchPath m = MarchPath.decode(b, 0, raw.length, 3, NAMES);
        assertNotNull(m);
        assertEquals(8, m.count("mowziesmobs:foliaath"));
        assertEquals(1, m.units().size());
        assertNull(m.culture());
        assertNull(m.dimension());
    }

    @Test
    void theFlagsAndPhasesAreTheStopAndTheEnd() {
        assertTrue(on(ConflictRecords.March.WAITING, 0).stopped());
        assertTrue(on(ConflictRecords.March.FIGHTING, 0).stopped());
        for (int phase : new int[] {ConflictRecords.March.MUSTERING, ConflictRecords.March.OUT, ConflictRecords.March.HOME})
            assertFalse(on(phase, 0).stopped());
        assertTrue(on(ConflictRecords.March.HOME, ConflictRecords.March.FLAG_ENDED).ended());
        assertFalse(on(ConflictRecords.March.HOME, ConflictRecords.March.FLAG_CHIEF).ended());
        assertTrue(on(ConflictRecords.March.OUT, ConflictRecords.March.FLAG_CHIEF | ConflictRecords.March.FLAG_JOINT).chief());
        assertTrue(on(ConflictRecords.March.OUT, ConflictRecords.March.FLAG_JOINT).joint());
    }

    @Test
    void aLaterRevisionIsNewerInUnsignedOrder() {
        MarchPath one = new MarchPath(5, 1, null, 0, null, 3, 2, 0, 0, 0, new HashMap<>(), new int[0], new int[0],
                new long[0]);
        MarchPath two = new MarchPath(5, 2, null, 0, null, 3, 2, 0, 0, 0, new HashMap<>(), new int[0], new int[0],
                new long[0]);
        MarchPath top = new MarchPath(5, -1, null, 0, null, 3, 2, 0, 0, 0, new HashMap<>(), new int[0], new int[0],
                new long[0]);
        assertTrue(two.newerThan(one));
        assertFalse(one.newerThan(two));
        assertFalse(one.newerThan(one));
        assertTrue(top.newerThan(two));
    }

    // ---- MZB-454, MZB-477: the position rule ------------------------------------------------------

    @Test
    void theFlatVectorTruncatesTowardZero() {
        MarchPath m = on(ConflictRecords.March.OUT, 0, w(0, 0, 1_000), w(100, 37, 1_100), w(100, -50, 1_200));
        long[][] want = {{999, 0, 0}, {1_000, 0, 0}, {1_050, 50, 18}, {1_100, 100, 37}, {1_150, 100, -6}, {1_200, 100, -50},
                {1_300, 100, -50}};
        for (long[] v : want)
            assertArrayEquals(new int[] {(int) v[1], (int) v[2]}, m.chartAt(v[0]), "tick " + v[0]);
        // On a flat world the level frame is the chart, from the block centre floored back.
        LevelTopology flat = LevelTopology.flat();
        for (long[] v : want)
            assertArrayEquals(new int[] {(int) v[1], (int) v[2]}, m.at(v[0], flat), "flat tick " + v[0]);
        // Home walks the same way: the rule is the phase's two arms, not its number.
        MarchPath h = on(ConflictRecords.March.HOME, 0, w(0, 0, 1_000), w(100, 37, 1_100), w(100, -50, 1_200));
        assertArrayEquals(new int[] {100, -6}, h.chartAt(1_150));
    }

    @Test
    void musteringAndTheStopAreTheRoutesEnds() {
        long[][] route = {w(0, 0, 1_000), w(100, 37, 1_100), w(100, -50, 1_200)};
        assertArrayEquals(new int[] {0, 0}, on(ConflictRecords.March.MUSTERING, 0, route).chartAt(1_150));
        assertArrayEquals(new int[] {100, -50}, on(ConflictRecords.March.WAITING, 0, route).chartAt(0));
        assertArrayEquals(new int[] {100, -50}, on(ConflictRecords.March.FIGHTING, 0, route).chartAt(0));
        // No waypoint: no place. One waypoint out: that one.
        assertNull(on(ConflictRecords.March.OUT, 0).chartAt(5));
        assertNull(on(ConflictRecords.March.OUT, 0).at(5, LevelTopology.flat()));
        assertArrayEquals(new int[] {7, -9}, on(ConflictRecords.March.HOME, 0, w(7, -9, 40)).chartAt(500));
        // A segment of no duration holds its start, and the walk goes on after it.
        MarchPath z = on(ConflictRecords.March.OUT, 0, w(0, 0, 10), w(50, 50, 10), w(90, 10, 30));
        assertArrayEquals(new int[] {0, 0}, z.chartAt(9));
        assertArrayEquals(new int[] {50, 50}, z.chartAt(10));
        assertArrayEquals(new int[] {70, 30}, z.chartAt(20));
    }

    @Test
    void anInterpolationAcrossTheWholeIntRangeDoesNotOverflow() {
        MarchPath m = on(ConflictRecords.March.OUT, 0, w(Integer.MIN_VALUE, 0, 0), w(Integer.MAX_VALUE, 0, 100));
        int x = m.chartAt(50)[0];
        // Halfway is about zero, not an overflowed wrap.
        assertTrue(Math.abs(x) <= 1, "x = " + x);
    }

    @Test
    void anXWrapFoldsTheChartPointIntoThePeriod() {
        LevelTopology t = LevelTopology.xWrap(0, 64, LevelTopology.Source.TOROIDAL_WORLD);
        MarchPath m = on(ConflictRecords.March.OUT, 0, w(50, 0, 0), w(90, 10, 40));
        assertArrayEquals(new int[] {70, 5}, m.chartAt(20));
        assertArrayEquals(new int[] {6, 5}, m.at(20, t));
        assertArrayEquals(new int[] {26, 10}, m.at(40, t));
    }

    @Test
    void guyouTurnsTheBandAcrossTheSouthSeamAndFoldsTheBlockCentre() {
        LevelTopology t = LevelTopology.guyou(64, false, LevelTopology.Source.DEEP_TIME);
        MarchPath m = on(ConflictRecords.March.OUT, 0, w(10, 20, 0), w(10, 50, 30));
        assertArrayEquals(new int[] {10, 30}, m.at(10, t));
        assertArrayEquals(new int[] {-11, 23}, m.at(20, t));
        assertArrayEquals(new int[] {-11, 13}, m.at(30, t));
        // The corner's fold, (-10, 24), is the trap MZB-454 names.
        assertFalse(java.util.Arrays.equals(new int[] {-10, 24}, m.at(20, t)));
    }
}
