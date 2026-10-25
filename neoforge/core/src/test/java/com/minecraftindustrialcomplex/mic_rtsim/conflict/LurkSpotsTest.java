package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import org.junit.jupiter.api.Test;

import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Where a lurking body stands (M8, MZB-360, the re-check's P2), as geometry in pure Java: the hold point's distance to
 * the rail's centre line is {@code SPOT_BLOCKS} (within the slack) for a rail along an axis or a diagonal, a den
 * square to it, oblique, or on the line. The GameTests lay the same geometries in a world (FrostmawGameTests,
 * RailFrostmawGameTests, RailDirectionsGameTests).
 */
class LurkSpotsTest {
    /** Rail directions as (x, z). */
    private static final double[] EW = {1, 0}, NS = {0, 1}, DIAG = {1, 1}, ANTI = {-1, 1};

    private static List<double[]> cands(double tx, double tz, double sx, double sz, double[] axis) {
        return LurkGeometry.candidates(tx, tz, sx, sz, axis[0], axis[1]);
    }

    private static double lat(double tx, double tz, double[] axis, double[] p) {
        return LurkGeometry.lateral(tx, tz, axis[0], axis[1], p[0], p[1]);
    }

    private static void at(double[] got, double x, double z) {
        assertEquals(x, got[0], 1.0e-9, "x of " + got[0] + ", " + got[1]);
        assertEquals(z, got[1], 1.0e-9, "z of " + got[0] + ", " + got[1]);
    }

    @Test
    void aRailAlongXWithADenSquareToItStandsFiveBlocksOffOnTheDensSide() {
        // three block centres a side (the track point's own column and one each side along the rail), the den's side first
        List<double[]> north = cands(100.5, 200.5, 100.5, 195.5, EW);
        assertEquals(6, north.size());
        at(north.get(0), 100.5, 195.5);
        at(north.get(1), 99.5, 195.5);
        at(north.get(2), 101.5, 195.5);
        for (int i = 3; i < 6; i++)
            assertEquals(205.5, north.get(i)[1], 1.0e-9, "the other side comes second");
        List<double[]> south = cands(100.5, 200.5, 100.5, 205.5, EW);
        at(south.get(0), 100.5, 205.5);
        for (int i = 3; i < 6; i++)
            assertEquals(195.5, south.get(i)[1], 1.0e-9);
    }

    @Test
    void aRailAlongZStandsOffInX() {
        at(cands(-10.5, 33.5, -15.5, 33.5, NS).get(0), -15.5, 33.5);
        at(cands(-10.5, 33.5, -5.5, 33.5, NS).get(0), -5.5, 33.5);
    }

    @Test
    void anObliqueDenStillGivesThePointOnTheRailsNormal() {
        // The engine's spot for a den 4 east and 4 north of the point is (+4, -4) after its rounding: the sample's
        // line is 45 degrees off the rail. The normal through the track point puts the body straight north.
        List<double[]> c = cands(100.5, 200.5, 104.5, 196.5, EW);
        at(c.get(0), 100.5, 195.5);
        assertEquals(5.0, lat(100.5, 200.5, EW, c.get(0)), 1.0e-9);
    }

    @Test
    void aDenOnTheLineTakesTheLeftHandSideFirstAndTheOtherSecond() {
        // The engine put the spot 5 along +x, on an east-west rail. Left of +x looking along it (x east, z south) is
        // (-az, ax) = (0, +1), the south; looking along -x it is the north.
        List<double[]> c = cands(100.5, 200.5, 105.5, 200.5, EW);
        at(c.get(0), 100.5, 205.5);
        assertEquals(6, c.size());
        for (int i = 0; i < 3; i++)
            assertEquals(205.5, c.get(i)[1], 1.0e-9, "the first half is the left-hand side");
        for (int i = 3; i < 6; i++)
            assertEquals(195.5, c.get(i)[1], 1.0e-9, "the other side is offered after it");
        at(cands(100.5, 200.5, 105.5, 200.5, new double[] {-1, 0}).get(0), 100.5, 195.5);
    }

    @Test
    void aSpotJustOffTheLineStillNamesItsSide() {
        // 0.6 north of the line is past ON_LINE: north is the den's side
        assertTrue(cands(100.5, 200.5, 105.5, 199.9, EW).get(0)[1] < 200.5);
        // 0.4 north of the line is on it: the left-hand (south) side
        assertTrue(cands(100.5, 200.5, 105.5, 200.1, EW).get(0)[1] > 200.5);
    }

    @Test
    void aDiagonalRailStandsWithinTheSlackOfFiveWhateverTheFractionOfTheTrackPoint() {
        // Block centres on a diagonal's side lie on lines 0.707 apart: sweep the point's fraction along the rail and
        // across it, and the nearest centre must always be inside LATERAL_SLACK, on the den's side.
        for (double[] axis : new double[][] {DIAG, ANTI})
            for (int i = 0; i < 20; i++)
                for (int j = 0; j < 20; j++) {
                    double tx = 100.5 + i * 0.05, tz = 200.5 + j * 0.05;
                    double len = Math.hypot(axis[0], axis[1]);
                    double nx = -axis[1] / len, nz = axis[0] / len;
                    for (int side : new int[] {1, -1}) {
                        List<double[]> c = cands(tx, tz, tx + side * nx * 5.0, tz + side * nz * 5.0, axis);
                        String at = "axis " + axis[0] + "," + axis[1] + " track " + tx + "," + tz + " side " + side;
                        assertFalse(c.isEmpty(), "a point is found: " + at);
                        double[] p = c.get(0);
                        double l = lat(tx, tz, axis, p);
                        assertEquals(5.0, l, LurkGeometry.LATERAL_SLACK + 1.0e-9, "lateral " + l + " for " + at);
                        double signed = (p[0] - tx) * nx + (p[1] - tz) * nz;
                        assertTrue(signed * side > 0.0, "on the den's side: " + at);
                        double along = ((p[0] - tx) * axis[0] + (p[1] - tz) * axis[1]) / len;
                        assertTrue(Math.abs(along) <= LurkGeometry.ALONG_MAX + 1.0e-9, "along " + along + " for " + at);
                    }
                }
    }

    @Test
    void theEnginesSpotOnADiagonalIsNotOnTheNormal() {
        // Why Java places the body: the engine's spot for a den due north of a rail running south-east is 5 blocks
        // north, which is 3.5 blocks from the rail, not 5 (rail.rs spot_toward knows no rail).
        assertEquals(3.5355, LurkGeometry.lateral(100.5, 200.5, DIAG[0], DIAG[1], 100.5, 195.5), 1.0e-3);
        double[] p = cands(100.5, 200.5, 100.5, 195.5, DIAG).get(0);
        assertEquals(5.0, lat(100.5, 200.5, DIAG, p), LurkGeometry.LATERAL_SLACK + 1.0e-9);
        assertTrue(p[0] > 100.5 && p[1] < 200.5, "toward the den's side (north-east of the rail's line): " + p[0] + ", " + p[1]);
    }

    @Test
    void candidatesAreInADeterministicOrderAndAllBlockCentres() {
        List<double[]> a = cands(100.37, 200.81, 103.0, 196.0, EW);
        List<double[]> b = cands(100.37, 200.81, 103.0, 196.0, new double[] {2.0, 0});
        assertEquals(a.size(), b.size(), "the axis's length does not matter");
        for (int i = 0; i < a.size(); i++)
            at(b.get(i), a.get(i)[0], a.get(i)[1]);
        for (double[] v : a) {
            assertEquals(0.5, v[0] - Math.floor(v[0]), 1.0e-9);
            assertEquals(0.5, v[1] - Math.floor(v[1]), 1.0e-9);
        }
        // the track point's own column first (along 0.13), then the one behind it (-0.87), then the one ahead (1.13)
        at(a.get(0), 100.5, 195.5);
        at(a.get(1), 99.5, 195.5);
        at(a.get(2), 101.5, 195.5);
    }

    @Test
    void noAxisGivesNoCandidates() {
        assertTrue(cands(0.5, 0.5, 5.5, 0.5, new double[] {0, 0}).isEmpty());
        assertTrue(cands(0.5, 0.5, 5.5, 0.5, new double[] {Double.NaN, 0}).isEmpty());
    }
}
