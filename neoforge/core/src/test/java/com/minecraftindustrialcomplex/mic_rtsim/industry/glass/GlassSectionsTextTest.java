package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;

import org.junit.jupiter.api.Test;

/** The text of {@code /rtsim district sections} for a report built by hand, as GlassReportTest lays it out. */
class GlassSectionsTextTest {

    private static GlassReport.Section section(int unit, int group, int state, int flags, int worst) {
        return new GlassReport.Section(unit, group, 9, 2, 1, 1, flags, state, true, -3, 60, 4, 5, 64, 9, true, -2, 61, 5,
                0xDEADBEEF01234567L, state == GlassReport.FAILED ? GlassReport.FAIL : GlassReport.PASS, 1, 2, 300, worst,
                0.05, 0.1, 0.05, List.of(new GlassReport.StoreRef(4, true), new GlassReport.StoreRef(-1, false)));
    }

    private static GlassReport report(List<GlassReport.Section> sections, List<GlassReport.Shared> shared, int units) {
        GlassReport.Checksum c = new GlassReport.Checksum(0, 0, 0, 0, 0, -1, 0, -1, -1, 0, new long[8]);
        GlassReport.Replay rp = new GlassReport.Replay(GlassReport.REPLAY_GLASS, 5000, 0, 0, 0, 0, true, 0);
        List<GlassReport.Unit> us = new java.util.ArrayList<>();
        for (int i = 0; i < units; i++)
            us.add(new GlassReport.Unit(1, 9, false, 0, 0, 0, 0, 0, 0, 0, 0, GlassReport.PASS, 0, true, false, true, false, false, 0));
        return new GlassReport(1, 1, true, 9, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, c, rp, List.of(), us, sections, shared);
    }

    @Test
    void oneLinePerSectionThenTheSharedStores() {
        int flagsReplaying = GlassReport.Section.PASSED | GlassReport.Section.MAY_REPLAY | GlassReport.Section.WORKING;
        GlassReport r = report(List.of(
                section(0, 0, GlassReport.REPLAYING, flagsReplaying, -1),
                section(1, 1, GlassReport.FAILED, GlassReport.Section.WORKING, 7),
                section(1, 1, GlassReport.FAILED, GlassReport.Section.WORKING, 7)),
                List.of(new GlassReport.Shared(12, 4, true, 1, 62, 3, List.of(0, 1), true, 2, true, 41, List.<long[]>of(new long[] {7, -68})),
                        new GlassReport.Shared(13, -1, false, 0, 0, 0, List.of(1, 2), false, 0, false, -1, List.of())),
                2);
        List<String> out = GlassSectionsText.lines(5, "mill", r, id -> id == 7 ? "sheet" : null);
        assertEquals("district 5 'mill': 3 sections (1 may replay, 1 replaying) in 2 units, 2 shared stores", out.get(0));
        assertTrue(out.get(1).startsWith("  s0 at (-2 61 5): replaying, unit 0, working, 9 nodes, 2 stores [inv 4* -]"), out.get(1));
        assertTrue(out.get(1).contains("box (-3 60 4)..(5 64 9), fp deadbeef01234567, window 300 ticks, 2 done, 1 strikes, may replay"), out.get(1));
        assertTrue(out.get(2).startsWith("  s1 at") && out.get(2).contains(": FAILED, unit 1, group s1") == false
                && out.get(2).contains(": FAILED, unit 1, working"), out.get(2));
        assertEquals("      worst good sheet: measured 60.000/min, predicted 120.000/min, 60.000/min beyond its band", out.get(3));
        assertTrue(out.get(4).contains(": FAILED, unit 1"), out.get(4));
        assertEquals("  shared store n12 inv 4 at (1 62 3) of s0, s1: counted at its movers (2 pairs), counted this sweep (last sample sweep 41), residue sheet -68",
                out.get(5));
        assertEquals("  shared store n13 of s1, s2: held by one unit, judged inside it", out.get(6));
        assertEquals(7, out.size(), "the second section of a unit repeats no worst good");
    }

    @Test
    void aDistrictOfOneWindowNamesNoUnits() {
        GlassReport r = report(List.of(section(-1, 0, GlassReport.PASSED_STATE, GlassReport.Section.PASSED | GlassReport.Section.WORKING, -1)), List.of(), 0);
        List<String> out = GlassSectionsText.lines(1, "a", r, id -> null);
        assertEquals("district 1 'a': 1 sections (0 may replay, 0 replaying) in the district's window, 0 shared stores", out.get(0));
        assertTrue(out.get(1).contains(": passed, district window, working"), out.get(1));
        assertEquals(2, out.size());
    }

    @Test
    void aDistrictOfOneWindowNamesItsWorstGoodOnceUnderTheHeader() {
        GlassReport r = report(List.of(
                section(-1, 0, GlassReport.FAILED, GlassReport.Section.WORKING, 7),
                section(-1, 1, GlassReport.FAILED, GlassReport.Section.WORKING, 7)), List.of(), 0);
        List<String> out = GlassSectionsText.lines(1, "a", r, id -> "sheet");
        assertTrue(out.get(1).startsWith("      worst good sheet"), out.get(1));
        assertTrue(out.get(2).startsWith("  s0 ") && out.get(3).startsWith("  s1 "), out.toString());
        assertEquals(4, out.size());
    }
}
