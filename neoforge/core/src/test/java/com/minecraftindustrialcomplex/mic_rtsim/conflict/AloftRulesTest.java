package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.conflict.AloftRules.Cells;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.AloftRules.Landing;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.AloftRules.Repair;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.AloftRules.Stuck;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.AloftRules.Turn;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.AloftRules.Verdict;
import org.junit.jupiter.api.Test;

import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The aloft chief's rules at each threshold and one either side (MZB-808 to MZB-814, MZB-841). */
class AloftRulesTest {
    private static final double EPS = 1e-9;

    // ---- facing --------------------------------------------------------------------------------

    @Test
    void quarterInvertsTheJarsDirectionToYaw() {
        // EntityUmvuthi.tick sets yaw to (direction - 1) * 90 every tick, and its direction is 1 to 4 (the constructor
        // rolls nextInt(4) + 1; 0 is "unset", and checkBlocksByFeet matches no branch for it): MZB-806, MZB-870.
        for (int d = 1; d <= 4; d++)
            assertEquals(d, AloftRules.quarter(AloftRules.quarterYaw(d)), "direction " + d);
        assertEquals(1, AloftRules.quarter(0f));
        assertEquals(2, AloftRules.quarter(90f));
        assertEquals(3, AloftRules.quarter(180f));
        assertEquals(4, AloftRules.quarter(-90f));
        assertEquals(4, AloftRules.quarter(270f));
    }

    @Test
    void quarterIsNeverZero() {
        for (float yaw = -720f; yaw <= 720f; yaw += 7.5f) {
            int q = AloftRules.quarter(yaw);
            assertTrue(q >= 1 && q <= 4, "yaw " + yaw + " gave " + q);
        }
    }

    @Test
    void quarterRoundsToTheNearestQuarterAndWraps() {
        assertEquals(1, AloftRules.quarter(44f));
        assertEquals(2, AloftRules.quarter(46f));
        assertEquals(2, AloftRules.quarter(134f));
        assertEquals(3, AloftRules.quarter(136f));
        assertEquals(1, AloftRules.quarter(360f));
        assertEquals(3, AloftRules.quarter(-180f));
        assertEquals(2, AloftRules.quarter(450f));
        assertEquals(4, AloftRules.quarter(-46f));
        assertEquals(1, AloftRules.quarter(-44f));
    }

    @Test
    void theHalfwayYawsPickTheLaterQuarter() {
        assertEquals(2, AloftRules.quarter(45f));
        assertEquals(3, AloftRules.quarter(135f));
    }

    @Test
    void aWayFacesAsMinecraftYawDoes() {
        assertEquals(0f, AloftRules.wayYaw(0, 1), EPS); // +z is yaw 0
        assertEquals(-90f, AloftRules.wayYaw(1, 0), EPS); // +x is -90
        assertEquals(90f, AloftRules.wayYaw(-1, 0), EPS);
        assertEquals(180f, Math.abs(AloftRules.wayYaw(0, -1)), EPS);
    }

    @Test
    void theTurnIsTheWrappedAngleBetweenTwoYaws() {
        assertEquals(20f, AloftRules.turn(170f, -170f), EPS);
        assertEquals(180f, AloftRules.turn(0f, 180f), EPS);
        assertEquals(0f, AloftRules.turn(180f, -180f), EPS);
        assertEquals(90f, AloftRules.turn(-90f, 0f), EPS);
    }

    // ---- height --------------------------------------------------------------------------------

    @Test
    void theFootIsThreeAboveTheHighestTop() {
        // ChunkAccess.getHeight is the top block's y, one less than Level.getHeight: top + 1 + 2 (MZB-808).
        assertEquals(73.0, AloftRules.foot(new int[]{64, 70, 66}, 3, 0.0), EPS);
        assertEquals(67.0, AloftRules.foot(new int[]{64}, 1, 0.0), EPS);
    }

    @Test
    void anUnreadColumnIsSkipped() {
        int[] tops = {AloftRules.UNREAD, 64, AloftRules.UNREAD};
        assertEquals(67.0, AloftRules.foot(tops, 3, 0.0), EPS);
    }

    @Test
    void withNoColumnReadTheTargetStaysAsItWas() {
        int[] tops = {AloftRules.UNREAD, AloftRules.UNREAD};
        assertEquals(91.5, AloftRules.foot(tops, 2, 91.5), EPS);
        assertEquals(91.5, AloftRules.foot(tops, 0, 91.5), EPS);
    }

    @Test
    void onlyTheFirstCountColumnsAreRead() {
        assertEquals(67.0, AloftRules.foot(new int[]{64, 200, 200}, 1, 0.0), EPS);
    }

    @Test
    void theVerticalStepRisesAtMostATenthAndSinksAtMostTwentiethOfABlock() {
        assertEquals(0.1, AloftRules.verticalStep(0.0, 5.0), EPS);
        assertEquals(-0.05, AloftRules.verticalStep(5.0, 0.0), EPS);
        // Within a cap it arrives.
        assertEquals(0.09, AloftRules.verticalStep(0.0, 0.09), EPS);
        assertEquals(-0.04, AloftRules.verticalStep(1.0, 0.96), EPS);
        assertEquals(0.0, AloftRules.verticalStep(3.0, 3.0), EPS);
    }

    @Test
    void theVerticalStepJustAtAndBeyondItsCaps() {
        assertEquals(0.1, AloftRules.verticalStep(0.0, 0.1), EPS);
        assertEquals(0.1, AloftRules.verticalStep(0.0, 0.11), EPS);
        assertEquals(-0.05, AloftRules.verticalStep(0.0, -0.05), EPS);
        assertEquals(-0.05, AloftRules.verticalStep(0.0, -0.06), EPS);
    }

    @Test
    void itTakesNoStepAcrossWhileMoreThanOneBelowItsTarget() {
        assertTrue(AloftRules.mayStepAcross(10.0, 11.0)); // exactly 1 below
        assertFalse(AloftRules.mayStepAcross(10.0, 11.01));
        assertTrue(AloftRules.mayStepAcross(10.0, 10.0));
        assertTrue(AloftRules.mayStepAcross(12.0, 10.0)); // above its target: it sinks and goes on
    }

    @Test
    void theStepAcrossIsATenthOfABlockAtMost() {
        assertEquals(0.1, AloftRules.across(50.0), EPS);
        assertEquals(0.1, AloftRules.across(0.1), EPS);
        assertEquals(0.05, AloftRules.across(0.05), EPS);
        assertEquals(0.0, AloftRules.across(0.0), EPS);
    }

    // ---- distance ------------------------------------------------------------------------------

    @Test
    void distancesAreHorizontal() {
        // A player 150 across and 100 up is within 160 (3D it would be 180); sight is horizontal (MZB-753, MZB-831).
        assertTrue(AloftRules.near(0, 0, 0, 150, 100, 0, AloftRules.SIGHT));
        assertTrue(AloftRules.near(0, 0, 0, 160, -300, 0, AloftRules.SIGHT));
        assertFalse(AloftRules.near(0, 0, 0, 160.1, 0, 0, AloftRules.SIGHT));
        assertTrue(AloftRules.near(0, 0, 0, 100, 5, 120, AloftRules.SIGHT)); // 156.2 across
        assertFalse(AloftRules.near(0, 0, 0, 120, 0, 120, AloftRules.SIGHT)); // 169.7 across
    }

    // ---- settle and rise -----------------------------------------------------------------------

    @Test
    void withTheAttackFlagItSettlesWithin32BlocksOfTheTarget() {
        assertTrue(AloftRules.settles(true, 31.9, 0, false));
        assertTrue(AloftRules.settles(true, 32.0, 0, false));
        assertFalse(AloftRules.settles(true, 32.1, 0, false));
        assertFalse(AloftRules.settles(false, 5, 0, false)); // no flag, no player in sight: it stays up
        assertTrue(AloftRules.settles(true, 20, 20, false)); // 28.3 across
        assertFalse(AloftRules.settles(true, 25, 25, false)); // 35.4 across
    }

    @Test
    void aPlayerTargetInSightSettlesItAtAnyDistance() {
        assertTrue(AloftRules.settles(false, 500, 500, true));
        assertTrue(AloftRules.settles(true, 500, 0, true));
    }

    @Test
    void overNoFluidASettlingBodySinks() {
        assertEquals(Landing.SINK, AloftRules.landing(true, false, 3, 0));
        assertEquals(Landing.AIR, AloftRules.landing(false, false, 3, 0));
    }

    @Test
    void overFluidASettlingBodyFloatsOnAndHoversWithin8OfTheTarget() {
        assertEquals(Landing.AIR, AloftRules.landing(true, true, 8.1, 0)); // floats on toward its place
        assertEquals(Landing.HOVER, AloftRules.landing(true, true, 7.9, 0));
        assertEquals(Landing.HOVER, AloftRules.landing(true, true, 8.0, 0));
        assertEquals(Landing.AIR, AloftRules.landing(true, true, 6, 6)); // 8.5 across
        assertEquals(Landing.AIR, AloftRules.landing(false, true, 1, 0)); // not settling: nothing to hover for
    }

    @Test
    void itRisesAfter200TicksWithNoFlagAndNoPlayerTarget() {
        assertFalse(AloftRules.rises(false, false, 1199, 1000));
        assertTrue(AloftRules.rises(false, false, 1200, 1000));
        assertTrue(AloftRules.rises(false, false, 5000, 1000));
    }

    @Test
    void theFlagOrAPlayerTargetKeepsItDown() {
        assertFalse(AloftRules.rises(true, false, 5000, 1000));
        assertFalse(AloftRules.rises(false, true, 5000, 1000));
    }

    // ---- stuck ---------------------------------------------------------------------------------

    private static Stuck stuckAt(double distance) {
        Stuck s = new Stuck();
        assertEquals(Verdict.NONE, s.tick(0, distance));
        return s;
    }

    @Test
    void nothingHappensBeforeTheWindowEnds() {
        Stuck s = stuckAt(50);
        for (long t = 1; t < 100; t++)
            assertEquals(Verdict.NONE, s.tick(t, 50), "tick " + t);
        assertEquals(0, s.raised());
    }

    @Test
    void headwayOfHalfABlockIn100TicksIsNotStuck() {
        Stuck below = stuckAt(100);
        assertEquals(Verdict.RAISED, below.tick(100, 99.51)); // 0.49 shrunk
        Stuck at = stuckAt(100);
        assertEquals(Verdict.NONE, at.tick(100, 99.5)); // exactly 0.5
        Stuck above = stuckAt(100);
        assertEquals(Verdict.NONE, above.tick(100, 99.49)); // 0.51
    }

    @Test
    void withinEightBlocksOfItsPlaceItIsNeverStuck() {
        Stuck near = stuckAt(8.0);
        assertEquals(Verdict.NONE, near.tick(100, 8.0));
        Stuck far = stuckAt(8.1);
        assertEquals(Verdict.RAISED, far.tick(100, 8.1));
    }

    @Test
    void eachWindowWithoutHeadwayRaisesTheTargetByEightTo24() {
        Stuck s = stuckAt(50);
        assertEquals(Verdict.RAISED, s.tick(100, 50));
        assertEquals(8, s.raised());
        assertEquals(Verdict.RAISED, s.tick(200, 50));
        assertEquals(16, s.raised());
        assertEquals(Verdict.RAISED, s.tick(300, 50));
        assertEquals(24, s.raised());
        assertFalse(s.holding());
        assertEquals(Verdict.HELD, s.tick(400, 50)); // the 100 ticks after the 24
        assertEquals(24, s.raised());
        assertTrue(s.holding());
    }

    @Test
    void aWindowIsMeasuredFromWhereTheLastEnded() {
        Stuck s = stuckAt(50);
        assertEquals(Verdict.RAISED, s.tick(100, 50));
        // Not shrunk by 0.5 since tick 100, whatever it had shrunk before.
        assertEquals(Verdict.RAISED, s.tick(200, 49.6));
        assertEquals(16, s.raised());
    }

    @Test
    void headwayResetsTheRaises() {
        Stuck s = stuckAt(50);
        s.tick(100, 50);
        s.tick(200, 50);
        assertEquals(16, s.raised());
        assertEquals(Verdict.NONE, s.tick(300, 40));
        assertEquals(0, s.raised());
        assertEquals(Verdict.RAISED, s.tick(400, 40)); // stuck again: the raise starts over
        assertEquals(8, s.raised());
    }

    @Test
    void headwayEndsTheHold() {
        Stuck s = stuckAt(50);
        s.tick(100, 50);
        s.tick(200, 50);
        s.tick(300, 50);
        s.tick(400, 50);
        assertTrue(s.holding());
        assertEquals(Verdict.NONE, s.tick(500, 45));
        assertFalse(s.holding());
        assertEquals(0, s.raised());
    }

    private static Stuck heldAt400() {
        Stuck s = stuckAt(50);
        s.tick(100, 50);
        s.tick(200, 50);
        s.tick(300, 50);
        assertEquals(Verdict.HELD, s.tick(400, 50));
        return s;
    }

    @Test
    void heldWithNoPlayerSeeingItItIsMovedAtOnce() {
        Stuck s = heldAt400();
        assertTrue(s.moveDue(400, false));
    }

    @Test
    void heldUnderAPlayersEyesItIsMovedOnlyAt1200HeldTicks() {
        Stuck s = heldAt400();
        assertFalse(s.moveDue(400, true));
        assertFalse(s.moveDue(400 + 1199, true));
        assertTrue(s.moveDue(400 + 1200, true));
    }

    @Test
    void notHeldItIsNeverDueToMove() {
        Stuck s = stuckAt(50);
        assertFalse(s.moveDue(100, false));
        s.tick(100, 50);
        s.tick(200, 50);
        s.tick(300, 50); // 24 raised, not yet held
        assertFalse(s.moveDue(399, false));
    }

    @Test
    void theHoldLooksEveryTenTicks() {
        Stuck s = heldAt400();
        assertTrue(s.lookDue(400));
        s.looked(400);
        assertFalse(s.lookDue(409));
        assertTrue(s.lookDue(410));
        s.looked(410);
        assertFalse(s.lookDue(419));
    }

    @Test
    void notHeldItNeverLooks() {
        Stuck s = stuckAt(50);
        assertFalse(s.lookDue(10_000));
    }

    @Test
    void movedStartsEverythingOver() {
        Stuck s = heldAt400();
        s.moved(500, 3.0);
        assertFalse(s.holding());
        assertEquals(0, s.raised());
        assertFalse(s.moveDue(500, false));
        assertEquals(Verdict.NONE, s.tick(600, 3.0)); // a new window from 500, near its place
    }

    // ---- the re-pair gate ----------------------------------------------------------------------

    @Test
    void aTurnOfMoreThan55DegreesIsHeldTwoTicksThenGoes() {
        Repair r = new Repair();
        assertEquals(Turn.HOLD, r.gate(1000, 0f, 56f, false, false));
        assertEquals(Turn.HOLD, r.gate(1001, 0f, 56f, false, false));
        assertEquals(Turn.GO, r.gate(1002, 0f, 56f, false, false));
    }

    @Test
    void aTurnOf55DegreesOrLessWaitsForNothing() {
        Repair r = new Repair();
        assertEquals(Turn.NO, r.gate(1000, 0f, 54f, false, false));
        assertEquals(Turn.NO, r.gate(1001, 0f, 55f, false, false));
        assertEquals(Turn.NO, r.gate(1002, 0f, -55f, false, false));
        assertEquals(Turn.HOLD, r.gate(1003, 0f, -56f, false, false));
    }

    @Test
    void theTurnIsMeasuredAcrossTheSeam() {
        Repair r = new Repair();
        assertEquals(Turn.NO, r.gate(1000, 170f, -170f, false, false)); // 20 degrees
        assertEquals(Turn.HOLD, r.gate(1001, 0f, 180f, false, false));
    }

    @Test
    void thereIsNoSecondRepairWithin200Ticks() {
        Repair r = new Repair();
        r.gate(1000, 0f, 90f, false, false);
        r.gate(1001, 0f, 90f, false, false);
        assertEquals(Turn.GO, r.gate(1002, 0f, 90f, false, false));
        assertEquals(Turn.NO, r.gate(1003, 0f, 90f, false, false));
        assertEquals(Turn.NO, r.gate(1002 + 199, 0f, 90f, false, false));
        assertEquals(Turn.HOLD, r.gate(1002 + 200, 0f, 90f, false, false));
    }

    @Test
    void anActiveAbilityStopsTheTurnAndRestartsTheHold() {
        Repair r = new Repair();
        assertEquals(Turn.HOLD, r.gate(1000, 0f, 90f, false, false));
        assertEquals(Turn.HOLD, r.gate(1001, 0f, 90f, false, false));
        assertEquals(Turn.NO, r.gate(1002, 0f, 90f, true, false)); // a ROAR starts
        assertEquals(Turn.HOLD, r.gate(1003, 0f, 90f, false, false)); // the hold starts over
        assertEquals(Turn.HOLD, r.gate(1004, 0f, 90f, false, false));
        assertEquals(Turn.GO, r.gate(1005, 0f, 90f, false, false));
    }

    @Test
    void aDyingBodyIsNeverRepaired() {
        Repair r = new Repair();
        assertEquals(Turn.NO, r.gate(1000, 0f, 90f, false, true));
        assertEquals(Turn.NO, r.gate(1001, 0f, 90f, false, true));
        assertEquals(Turn.NO, r.gate(1002, 0f, 90f, false, true));
    }

    @Test
    void aWayThatStraightensDuringTheHoldCancelsIt() {
        Repair r = new Repair();
        assertEquals(Turn.HOLD, r.gate(1000, 0f, 90f, false, false));
        assertEquals(Turn.NO, r.gate(1001, 0f, 10f, false, false));
        assertEquals(Turn.HOLD, r.gate(1002, 0f, 90f, false, false)); // starts over
    }

    @Test
    void aFirstRepairIsNotHeldBackByTheCooldown() {
        Repair r = new Repair();
        assertEquals(Turn.HOLD, r.gate(0, 0f, 90f, false, false));
    }

    // ---- the line to a player ------------------------------------------------------------------

    private static Cells blocked(Set<Long> cells) {
        return (x, y, z) -> cells.contains(key(x, y, z)) ? Cells.BLOCKS : Cells.OPEN;
    }

    private static long key(int x, int y, int z) {
        return ((long) (x + 1000) << 40) | ((long) (y + 1000) << 20) | (z + 1000);
    }

    @Test
    void aClearLineIsClear() {
        assertTrue(AloftRules.clear(0.5, 0.5, 0.5, 10.5, 0.5, 0.5, blocked(Set.of())));
    }

    @Test
    void aBlockBetweenBlocksTheLine() {
        assertFalse(AloftRules.clear(0.5, 0.5, 0.5, 10.5, 0.5, 0.5, blocked(Set.of(key(5, 0, 0)))));
    }

    @Test
    void aBlockOffTheLineDoesNot() {
        assertTrue(AloftRules.clear(0.5, 0.5, 0.5, 10.5, 0.5, 0.5, blocked(Set.of(key(5, 1, 0), key(5, 0, 1)))));
    }

    @Test
    void aSlopingLineMeetsABlockOnIt() {
        assertFalse(AloftRules.clear(0.5, 0.5, 0.5, 8.5, 4.5, 0.5, blocked(Set.of(key(4, 2, 0)))));
        assertFalse(AloftRules.clear(0.5, 0.5, 0.5, 0.5, 10.5, 0.5, blocked(Set.of(key(0, 5, 0)))));
    }

    @Test
    void theLineRunsBothWaysAndAcrossZero() {
        assertFalse(AloftRules.clear(-0.5, 0.5, -0.5, -10.5, 0.5, -0.5, blocked(Set.of(key(-6, 0, -1)))));
        assertFalse(AloftRules.clear(10.5, 0.5, 0.5, 0.5, 0.5, 0.5, blocked(Set.of(key(5, 0, 0)))));
        assertTrue(AloftRules.clear(10.5, 0.5, 0.5, 0.5, 0.5, 0.5, blocked(Set.of())));
    }

    @Test
    void aBlockThatDoesNotBlockMotionDoesNotBlockTheLine() {
        // The glue answers OPEN for grass and BLOCKS for what blocks motion (a glass pane does): the rule asks the cell.
        Cells grass = (x, y, z) -> Cells.OPEN;
        assertTrue(AloftRules.clear(0.5, 0.5, 0.5, 10.5, 0.5, 0.5, grass));
        Cells paneAt5 = (x, y, z) -> x == 5 ? Cells.BLOCKS : Cells.OPEN;
        assertFalse(AloftRules.clear(0.5, 0.5, 0.5, 10.5, 0.5, 0.5, paneAt5));
    }

    @Test
    void aColumnNotAtHandCountsAsBlocking() {
        Cells absent = (x, y, z) -> x == 5 ? Cells.ABSENT : Cells.OPEN;
        assertFalse(AloftRules.clear(0.5, 0.5, 0.5, 10.5, 0.5, 0.5, absent));
    }

    @Test
    void theTwoEndCellsAreNotLookedAt() {
        // The eye's cell and the body's cell are the ends: whatever the glue says of them, the line is between.
        Cells ends = (x, y, z) -> (x == 0 || x == 10) ? Cells.BLOCKS : Cells.OPEN;
        assertTrue(AloftRules.clear(0.5, 0.5, 0.5, 10.5, 0.5, 0.5, ends));
    }

    @Test
    void aLineWithinOneCellIsClear() {
        assertTrue(AloftRules.clear(0.2, 0.2, 0.2, 0.8, 0.8, 0.8, (x, y, z) -> Cells.BLOCKS));
    }

    @Test
    void aLongLineIsWalkedCellByCell() {
        int[] asked = {0};
        Cells count = (x, y, z) -> {
            asked[0]++;
            return Cells.OPEN;
        };
        assertTrue(AloftRules.clear(0.5, 0.5, 0.5, 160.5, 0.5, 0.5, count));
        assertEquals(159, asked[0]); // 160 cells between 161, less the two ends: 159 in the middle
    }
}
