package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Operator and developer diagnostics for districts (I18, as decided: players are shown nothing of what a
 * distant factory does; they build their own readouts from real state). It holds what Java itself knows
 * about a district, the last reconcile applied to it and the warnings plugins attach, and renders them as
 * the text of {@code /rtsim district info} and {@code /rtsim district debug} (permission level 2). No
 * player, display block, sign or atlas ever reads it, and no engine round trip is needed: the engine's own
 * ledger line (IND_LEDGER_REQUEST and IND_LEDGER) is gone from the wire.
 *
 * <p>Plugin warnings (I16, a Create logistics network spanning districts; an Ender Transmission channel
 * reaching another dimension) go to two places only: the server log, once when a warning is set or its text
 * changes and once when it clears, and the operator output above.
 */
public final class DistrictDiagnostics {

    /** The last reconcile applied to a district: deltas, shortfalls and blocks, for the operator output. */
    public record Reconcile(long at, int entries, long requested, long applied, int shortfalls, int broken, int skipped) {}

    private final Industry ind;
    private final Map<Long, Reconcile> reconciles = new HashMap<>();
    /** Warnings plugins attach to a district, by key (I16: a gauge network that spans districts). */
    private final Map<Long, Map<String, String>> warnings = new HashMap<>();

    DistrictDiagnostics(Industry ind) {
        this.ind = ind;
    }

    /**
     * Sets (or, with null text, clears) a warning on a district. Plugins call it on every sweep, so the log
     * line is written only when the warning appears, when its text changes and when it clears.
     */
    public void warn(long district, String key, String text) {
        if (text == null) {
            Map<String, String> m = warnings.get(district);
            if (m != null && m.remove(key) != null) {
                MicRtsim.LOGGER.info("[mic_rtsim] industry: district {} warning '{}' cleared", name(district), key);
                if (m.isEmpty())
                    warnings.remove(district);
            }
            return;
        }
        String before = warnings.computeIfAbsent(district, k -> new java.util.TreeMap<>()).put(key, text);
        if (!text.equals(before))
            MicRtsim.LOGGER.warn("[mic_rtsim] industry: district {} warning '{}': {}", name(district), key, text);
    }

    private String name(long district) {
        District d = ind.district(district);
        return d == null ? Long.toString(district) : district + " '" + d.displayName() + "'";
    }

    /** The warnings on a district, by key order. */
    public List<String> warnings(long district) {
        Map<String, String> m = warnings.get(district);
        return m == null ? List.of() : List.copyOf(m.values());
    }

    public Reconcile lastReconcile(long district) {
        return reconciles.get(district);
    }

    void noteReconcile(District d, long[][] results, int broken, int skipped) {
        long req = 0, app = 0;
        int short_ = 0;
        for (long[] r : results) {
            req += r[2];
            app += r[3];
            if (r[2] != r[3])
                short_++;
        }
        reconciles.put(d.id, new Reconcile(ind.gameTime(), results.length, req, app, short_, broken, skipped));
    }

    // ---- text -----------------------------------------------------------------------------------

    public static String freezeReason(int code) {
        return switch (code) {
            case 0 -> "";
            case IndRecords.NotLearnable.ROTATION_CROSSES -> "rotation crosses the boundary";
            case IndRecords.NotLearnable.ELECTRIC_LINE_CROSSES -> "electric line crosses the boundary";
            case IndRecords.NotLearnable.MIXTURE_NOT_STEADY -> "mixture composition not steady";
            case IndRecords.NotLearnable.FINITE_DEPOSIT_UNKNOWN -> "finite deposit unknown";
            case IndRecords.NotLearnable.MOVING_CONTRAPTION -> "moving drill";
            case IndRecords.NotLearnable.CONVEYOR_CROSSES -> "a belt, chute or pipe crosses the boundary";
            default -> "not learnable (code " + code + ")";
        };
    }

    public static String loadStateName(int state) {
        return switch (state) {
            case IndRecords.LoadState.UNLOADED -> "unloaded";
            case IndRecords.LoadState.PARTIAL -> "partly loaded";
            case IndRecords.LoadState.LIVE -> "loaded";
            default -> "not reported yet";
        };
    }

    /**
     * The operator output of {@code info} and {@code debug}: what Java knows of the district (its chunks, which
     * are loaded, the load state last reported to the engine, inventories, stations, revision and the
     * detector's not-learnable reason), the last sweep and defence survey, the last reconcile and the
     * plugin warnings. Everything here is Java's own state; the engine is not asked.
     */
    public List<MutableComponent> debugLines(District d) {
        List<MutableComponent> out = new java.util.ArrayList<>();
        out.add(Component.literal("District " + d.id + " '" + d.displayName() + "' in " + d.dim.location() + ": " + d.chunks.size()
                + " chunks (" + ind.districts.loadedChunks(d) + " loaded), " + d.inventories.size() + " boundary inventories, "
                + d.stations.size() + " stations, revision " + d.revision + " (declared " + d.declaredRevision + ")"
                + (d.notLearnable != 0 ? ", not learnable: " + freezeReason(d.notLearnable) : "")));
        out.add(Component.literal("  load state sent to the engine: " + loadStateName(d.loadState) + ", " + d.tickingChunks
                + " chunks ticking" + (d.watched ? ", watched by a player" : "")));
        out.add(Component.literal("  last sweep " + d.sweepNo + ": " + d.sweepInventories + " inventories, " + d.sweepRecords + " records"
                + (d.sweepLive ? ", live" : "")));
        long[] survey = ind.threats.lastSurvey(d.id);
        if (survey != null)
            out.add(Component.literal("  last defence survey: perimeter " + survey[0] + " columns, " + survey[1] + " gaps, " + survey[2]
                    + " dark spawnable blocks, " + survey[3] + " machines, " + survey[4] + " roofed"));
        for (String w : warnings(d.id))
            out.add(Component.literal("  warning: " + w));
        Reconcile r = reconciles.get(d.id);
        out.add(Component.literal(r == null ? "  no reconcile applied yet"
                : "  last reconcile at tick " + r.at + ": " + r.entries + " deltas, " + r.applied + " of " + r.requested
                        + " applied, " + r.shortfalls + " short, " + r.broken + " blocks broken, " + r.skipped + " skipped"));
        out.addAll(glassLines(d));
        return out;
    }

    /**
     * The glass box's checksum for operators (GLASSBOX.md §5.3: the server log and this output, never the
     * world): the engine's report, asked between steps. Only the native engine has one; the stub simulates
     * nothing.
     */
    private List<MutableComponent> glassLines(District d) {
        com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession s = com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession.current();
        if (s == null || !(s.engine() instanceof com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine n))
            return List.of();
        GlassReport r;
        try {
            r = GlassReport.decode(n.glassReport(d.id));
        } catch (RuntimeException e) {
            return List.of(Component.literal("  glass: no report (" + e.getMessage() + ")"));
        }
        if (r == null)
            return List.of(Component.literal("  glass: no complete compile yet"));
        List<MutableComponent> out = new java.util.ArrayList<>();
        out.add(Component.literal("  glass: " + r.summary()));
        out.add(Component.literal("  glass checksum: " + r.checksum().summary()));
        for (GlassReport.Good g : r.goods().stream().limit(MAX_LISTED).toList()) {
            String name = ind.goodName(g.good());
            out.add(Component.literal("    " + g.describe(name == null ? "good " + g.good() : name)));
        }
        return out;
    }

    /** {@code debug} adds the member chunks (loaded or not), the boundary inventories and the stations. */
    public List<MutableComponent> detailLines(District d) {
        List<MutableComponent> out = new java.util.ArrayList<>();
        long[] chunks = d.chunks.toLongArray();
        java.util.Arrays.sort(chunks);
        StringBuilder sb = new StringBuilder("  chunks:");
        net.minecraft.server.level.ServerLevel level = ind.level(d.dim);
        for (int i = 0; i < chunks.length && i < MAX_LISTED; i++) {
            int x = net.minecraft.world.level.ChunkPos.getX(chunks[i]), z = net.minecraft.world.level.ChunkPos.getZ(chunks[i]);
            boolean loaded = level != null && level.getChunkSource().getChunkNow(x, z) != null;
            sb.append(" (").append(x).append(',').append(z).append(loaded ? "*" : "").append(')');
        }
        if (chunks.length > MAX_LISTED)
            sb.append(" and ").append(chunks.length - MAX_LISTED).append(" more");
        out.add(Component.literal(sb + "  (* loaded)"));
        for (District.Inventory inv : d.inventories.stream().limit(MAX_LISTED).toList())
            out.add(Component.literal("  inventory " + inv.index() + " at " + inv.pos().toShortString() + ": " + inv.itemSlots() + " item slots, "
                    + inv.fluidCapacity() + " mB, " + inv.energyCapacity() + " FE"));
        for (District.Station s : d.stations)
            out.add(Component.literal("  station '" + s.name() + "' at " + s.pos().toShortString()));
        return out;
    }

    private static final int MAX_LISTED = 40;
}
