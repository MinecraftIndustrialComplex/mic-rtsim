package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.function.IntFunction;

/** The text of {@code /rtsim district sections} (no Minecraft or Brigadier types: tested on its own). */
public final class GlassSectionsText {
    private GlassSectionsText() {}

    /**
     * The command's text for a report: a header, a line per section (the first section of a unit also names the
     * unit's worst good, the good its last window missed its band by the most), and a line per shared store.
     */
    public static List<String> lines(long id, String name, GlassReport r, IntFunction<String> goodName) {
        List<String> out = new ArrayList<>();
        int replaying = r.sectionsReplaying(), may = r.sectionsMayReplay();
        out.add(String.format(Locale.ROOT, "district %d '%s': %d sections (%d may replay, %d replaying) in %s, %d shared stores",
                id, name, r.sections().size(), may, replaying,
                r.units().isEmpty() ? "the district's window" : r.units().size() + " units", r.shared().size()));
        int[] firstOf = new int[r.units().size()];
        java.util.Arrays.fill(firstOf, -1);
        for (int i = 0; i < r.sections().size(); i++) {
            GlassReport.Section s = r.sections().get(i);
            StringBuilder sb = new StringBuilder("  s").append(i);
            if (s.hasLow())
                sb.append(" at (").append(s.lowX()).append(' ').append(s.lowY()).append(' ').append(s.lowZ()).append(')');
            sb.append(": ").append(s.state() == GlassReport.FAILED ? "FAILED" : s.stateName());
            sb.append(s.unit() < 0 ? ", district window" : ", unit " + s.unit());
            sb.append(s.group() != i ? ", group s" + s.group() : "");
            sb.append(s.working() ? ", working" : ", storage");
            sb.append(", ").append(s.nodes()).append(" nodes, ").append(s.storeCount()).append(" stores");
            if (s.storeCount() > 0) {
                sb.append(" [");
                for (int k = 0; k < s.stores().size(); k++) {
                    GlassReport.StoreRef st = s.stores().get(k);
                    sb.append(k == 0 ? "" : " ").append(st.inventory() < 0 ? "-" : "inv " + st.inventory()).append(st.shared() ? "*" : "");
                }
                sb.append(']');
            }
            if (s.hasBox())
                sb.append(", box (").append(s.loX()).append(' ').append(s.loY()).append(' ').append(s.loZ()).append(")..(")
                        .append(s.hiX()).append(' ').append(s.hiY()).append(' ').append(s.hiZ()).append(')');
            sb.append(", fp ").append(s.fingerprintHex());
            sb.append(", window ").append(s.windowTicks()).append(" ticks, ").append(s.windowsCompleted()).append(" done, ")
                    .append(s.strikes()).append(" strikes");
            if (s.mayReplay())
                sb.append(", may replay");
            out.add(sb.toString());
            boolean first = s.unit() < 0 || firstOf[s.unit()] < 0;
            if (s.unit() >= 0 && firstOf[s.unit()] < 0)
                firstOf[s.unit()] = i;
            if (!first)
                out.add("      same unit as s" + firstOf[s.unit()] + ": its worst good is there");
            else if (s.worstGood() >= 0) {
                String g = goodName.apply(s.worstGood());
                out.add(String.format(Locale.ROOT, "      worst good %s: measured %.3f/min, predicted %.3f/min, %.3f/min beyond its band",
                        g == null ? "good " + s.worstGood() : g, s.worstMeasured() * 1200, s.worstPredicted() * 1200, s.worstExcess() * 1200));
            }
        }
        for (GlassReport.Shared h : r.shared()) {
            StringBuilder sb = new StringBuilder("  shared store n").append(h.node());
            if (h.inventory() >= 0)
                sb.append(" inv ").append(h.inventory());
            if (h.hasPos())
                sb.append(" at (").append(h.x()).append(' ').append(h.y()).append(' ').append(h.z()).append(')');
            sb.append(" of ");
            for (int k = 0; k < h.sections().size(); k++)
                sb.append(k == 0 ? "s" : ", s").append(h.sections().get(k));
            if (!h.counted())
                sb.append(": held by one unit, judged inside it");
            else {
                sb.append(": counted at its movers (").append(h.pairs()).append(" pairs), ");
                sb.append(h.countedNow() ? "counted this sweep" : "not counted this sweep");
                if (h.sampleSweep() >= 0)
                    sb.append(" (last sample sweep ").append(h.sampleSweep()).append(')');
                if (h.residue().isEmpty())
                    sb.append(", no residue");
                else {
                    sb.append(", residue");
                    for (long[] x : h.residue()) {
                        String g = goodName.apply((int) x[0]);
                        sb.append(' ').append(g == null ? "good " + x[0] : g).append(' ').append(x[1]);
                    }
                }
            }
            out.add(sb.toString());
        }
        return out;
    }
}
