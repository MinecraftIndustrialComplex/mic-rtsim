package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The far dragon's boxes and flipbook (research/distant-horizons-2026-10-21.md §4.2). */
class DragonModelTest {

    static double[] bounds(List<double[]> boxes) {
        double[] b = {Double.MAX_VALUE, Double.MAX_VALUE, Double.MAX_VALUE, -Double.MAX_VALUE, -Double.MAX_VALUE, -Double.MAX_VALUE};
        for (double[] x : boxes)
            for (int k = 0; k < 3; k++) {
                b[k] = Math.min(b[k], x[k]);
                b[k + 3] = Math.max(b[k + 3], x[k + 3]);
            }
        return b;
    }

    @Test
    void aDragonIsAsLongAsItsStageAndAsWideAsItsWings() {
        for (int stage = 1; stage <= 5; stage++) {
            double l = DragonModel.length(stage);
            assertTrue(stage == 1 || l > DragonModel.length(stage - 1));
            // Facing +x on an axis: the body runs nose to tail along x
            double[] body = bounds(DragonModel.body(stage, 0, 1.0));
            assertEquals(l, body[3] - body[0], 0.05 * l, "stage " + stage);
            double[] wings = bounds(DragonModel.wings(stage, 0, DragonModel.Pose.LEVEL, 1.0));
            assertEquals(DragonModel.WINGSPAN * l, wings[5] - wings[2], 0.1 * l, "stage " + stage);
            // The wings are symmetric about the body's line
            assertEquals(-wings[2], wings[5], 1e-6);
        }
    }

    @Test
    void thePosesRaiseAndLowerTheTips() {
        double up = bounds(DragonModel.wings(4, 3, DragonModel.Pose.UP, 1.0))[4];
        double level = bounds(DragonModel.wings(4, 3, DragonModel.Pose.LEVEL, 1.0))[4];
        double down = bounds(DragonModel.wings(4, 3, DragonModel.Pose.DOWN, 1.0))[1];
        double levelLow = bounds(DragonModel.wings(4, 3, DragonModel.Pose.LEVEL, 1.0))[1];
        assertTrue(up > level + 3, up + " vs " + level);
        assertTrue(down < levelLow - 3, down + " vs " + levelLow);
    }

    @Test
    void aDiagonalIsAStaircaseThatStaysOnTheLine() {
        // Heading step 2 is 45 degrees: the torso is cut into steps, every box near the body's line
        List<double[]> body = DragonModel.body(5, 2, 1.0);
        assertTrue(body.size() > 3, "a staircase, got " + body.size());
        double l = DragonModel.length(5);
        for (double[] b : body) {
            double cx = (b[0] + b[3]) / 2.0, cz = (b[2] + b[5]) / 2.0;
            double off = Math.abs(cx - cz) / Math.sqrt(2.0);
            assertTrue(off < 0.2 * l, "box off the line by " + off);
        }
        // Fewer boxes with LOW's longer steps
        assertTrue(DragonModel.body(5, 2, 2.0).size() < body.size());
    }

    @Test
    void headingsRoundToSixteenSteps() {
        assertEquals(0, DragonModel.headingStep(1, 0));
        assertEquals(4, DragonModel.headingStep(0, 1));
        assertEquals(8, DragonModel.headingStep(-1, 0.01));
        assertEquals(12, DragonModel.headingStep(0, -1));
        assertEquals(1, DragonModel.headingStep(Math.cos(Math.toRadians(20)), Math.sin(Math.toRadians(20))));
        assertEquals(0, DragonModel.headingStep(0, 0));
    }

    @Test
    void theFlipbookCyclesAndARoostingDragonFoldsItsWings() {
        Set<DragonModel.Pose> seen = new HashSet<>();
        for (int t = 0; t < 40; t++)
            seen.add(DragonModel.poseAt(t, 3, 7L, FarState.FLY));
        assertEquals(3, seen.size());
        seen.clear();
        for (int t = 0; t < 40; t++)
            seen.add(DragonModel.poseAt(t, 2, 7L, FarState.HUNT));
        assertEquals(Set.of(DragonModel.Pose.UP, DragonModel.Pose.DOWN), seen);
        assertNull(DragonModel.poseAt(10, 3, 7L, FarState.ROOST));
        // Two dragons do not beat as one
        assertTrue(DragonModel.poseAt(0, 3, 0L, FarState.FLY) != DragonModel.poseAt(0, 3, 1L, FarState.FLY));
    }
}
