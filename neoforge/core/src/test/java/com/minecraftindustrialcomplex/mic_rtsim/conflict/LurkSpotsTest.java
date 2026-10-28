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

    /** The Frostmaw: a stand of 0.5, a box 4 across, and the rail clearance and strike reach of the create sampler (MZB-374). */
    private static final double STAND = 0.5, HALF = 2.0, CLEARANCE = 1.5, REACH = 6.0;

    @Test
    void aDiagonalRailStandsOnTheNearSideOfFiveWhateverTheFractionAlongTheRail() {
        // The track point is a bogey's, on the rail's line: its fraction along the line varies, the line does not. The
        // block centres of a diagonal's side lie on lines 0.707 apart (4.95 and 5.66 from this one), and the window
        // [5 - OBLIQUE_NEAR_SLACK, 5 + LATERAL_SLACK] finds the 4.95 one every time, on the den's side.
        for (double[] axis : new double[][] {DIAG, ANTI})
            for (int i = 0; i < 40; i++) {
                double tx = 100.5 + i * 0.05, tz = 200.5 + i * 0.05 * Math.signum(axis[0] * axis[1]);
                double len = Math.hypot(axis[0], axis[1]);
                double nx = -axis[1] / len, nz = axis[0] / len;
                for (int side : new int[] {1, -1}) {
                    List<double[]> c = cands(tx, tz, tx + side * nx * 5.0, tz + side * nz * 5.0, axis);
                    String at = "axis " + axis[0] + "," + axis[1] + " track " + tx + "," + tz + " side " + side;
                    assertFalse(c.isEmpty(), "a point is found: " + at);
                    double[] p = c.get(0);
                    double l = lat(tx, tz, axis, p);
                    assertEquals(4.9497, l, 1.0e-3, "lateral " + l + " for " + at);
                    double signed = (p[0] - tx) * nx + (p[1] - tz) * nz;
                    assertTrue(signed * side > 0.0, "on the den's side: " + at);
                    double along = ((p[0] - tx) * axis[0] + (p[1] - tz) * axis[1]) / len;
                    assertTrue(Math.abs(along) <= LurkGeometry.ALONG_MAX + 1.0e-9, "along " + along + " for " + at);
                }
            }
    }

    @Test
    void aDiagonalRailsBodyIsNeverNearerThanItsClearanceWhateverTheTrackPoint() {
        // The contract holds by itself (the second re-check's P3): every point found, for any track point on or off the
        // diagonal's lattice line, has its box's corner, half * sqrt(2) from the centre, at least the clearance off the
        // line at the near side of the stand, and its far side inside the reach. Off the lattice the den's side may
        // have no point in the window (the block centres lie k / sqrt(2) from the line, offset by the track point);
        // the other side, the mirror lattice, has one, so a diagonal is never without a point.
        int denSide = 0, otherSide = 0;
        for (double[] axis : new double[][] {DIAG, ANTI})
            for (int i = 0; i < 20; i++)
                for (int j = 0; j < 20; j++) {
                    double tx = 100.5 + i * 0.05, tz = 200.5 + j * 0.05;
                    double len = Math.hypot(axis[0], axis[1]);
                    double nx = -axis[1] / len, nz = axis[0] / len;
                    for (int side : new int[] {1, -1}) {
                        List<double[]> c = cands(tx, tz, tx + side * nx * 5.0, tz + side * nz * 5.0, axis);
                        String where = "axis " + axis[0] + "," + axis[1] + " track " + tx + "," + tz + " side " + side;
                        assertFalse(c.isEmpty(), "one side or the other has a point: " + where);
                        double[] first = c.get(0);
                        if (((first[0] - tx) * nx + (first[1] - tz) * nz) * side > 0.0)
                            denSide++;
                        else
                            otherSide++;
                        for (double[] p : c) {
                            double l = lat(tx, tz, axis, p);
                            String at = where + " lateral " + l;
                            assertTrue(l - STAND - HALF * Math.sqrt(2.0) >= CLEARANCE - 1.0e-9, "the corner clears the rail: " + at);
                            assertTrue(l + STAND <= REACH + 1.0e-9, "the far side is in reach: " + at);
                        }
                    }
                }
        assertTrue(denSide > 0, "some track points find one on the den's side");
        assertTrue(otherSide > 0, "and the window is narrow enough that others find none there and go to the other side (it is not the old 0.4 either side): " + otherSide);
    }

    @Test
    void aRailAlongAnAxisKeepsTheWiderWindowOnTheNearSide() {
        // The side of a box meets an axis rail: 5 - 0.4 - 0.5 - 2.0 = 2.1 clear, so the near slack is the far slack
        assertEquals(LurkGeometry.LATERAL_SLACK, LurkGeometry.nearSlack(1, 0), 0.0);
        assertEquals(LurkGeometry.LATERAL_SLACK, LurkGeometry.nearSlack(0, -3), 0.0);
        assertEquals(LurkGeometry.OBLIQUE_NEAR_SLACK, LurkGeometry.nearSlack(1, 1), 0.0);
        assertEquals(LurkGeometry.OBLIQUE_NEAR_SLACK, LurkGeometry.nearSlack(-2, 1), 0.0, "any axis not along x or z is held to the tight window");
        assertTrue(5.0 - LurkGeometry.LATERAL_SLACK - STAND - HALF >= CLEARANCE);
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
