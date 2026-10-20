package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MutableComponent;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.material.Fluid;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Operator and developer diagnostics for districts (I18, as decided: players are shown nothing of what a
 * distant factory does; they build their own readouts from real state). It keeps the engine's IND_LEDGER
 * replies per district, the last reconcile applied, and the warnings plugins attach, and renders them as
 * the text of {@code /rtsim district info} and {@code /rtsim district debug} (permission level 2). No
 * player, display block, sign or atlas ever reads it. Reading never advances the sim (review fix 12);
 * asking the engine is a latest-wins IND_LEDGER_REQUEST.
 *
 * <p>Plugin warnings (I16, a Create logistics network spanning districts; an Ender Transmission channel
 * reaching another dimension) go to two places only: the server log, once when a warning is set or its text
 * changes and once when it clears, and the operator output above.
 */
public final class DistrictDiagnostics {

    /** One good's line: rate per real hour (72,000 ticks, F30) and the amount since {@code since}. */
    public record GoodLine(int good, long perHour, long sinceStart) {}

    /** The latest IND_LEDGER of a district. */
    public record Line(long district, int state, int freeze, long nextBreakpoint, int nextKind, int nextGood, long since,
                       List<GoodLine> goods, long receivedAt) {}

    /** The last reconcile applied to a district: deltas, shortfalls and blocks, for the operator output. */
    public record Reconcile(long at, int entries, long requested, long applied, int shortfalls, int broken, int skipped) {}

    private record Waiter(CommandSourceStack source, long district, long until) {}

    private final Industry ind;
    private final Map<Long, Line> latest = new HashMap<>();
    private final Map<Long, Reconcile> reconciles = new HashMap<>();
    private final Map<Long, Long> requestedAt = new HashMap<>();
    private final List<Waiter> waiters = new ArrayList<>();
    /** Warnings plugins attach to a district, by key (I16: a gauge network that spans districts). */
    private final Map<Long, Map<String, String>> warnings = new HashMap<>();
    long received;

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

    DistrictDiagnostics(Industry ind) {
        this.ind = ind;
    }

    public Line latest(long district) {
        return latest.get(district);
    }

    public Reconcile lastReconcile(long district) {
        return reconciles.get(district);
    }

    /** Asks the engine for a district's ledger, at most once per 20 ticks per district. */
    public void request(District d) {
        long now = ind.gameTime();
        Long last = requestedAt.get(d.id);
        if (last != null && now - last < 20)
            return;
        int at = ind.latest(IndRecords.IND_LEDGER_REQUEST, IndRecords.DistrictState.SIZE, 0);
        if (at < 0)
            return;
        ByteBuffer b = ind.open();
        b.putLong(at + IndRecords.DistrictState.GAME_TIME, now);
        b.putLong(at + IndRecords.DistrictState.DISTRICT, d.id);
        requestedAt.put(d.id, now);
    }

    /** {@code /rtsim district debug}: shows what is known now, and the engine's next reply when it comes. */
    public void requestFor(CommandSourceStack source, District d) {
        request(d);
        waiters.add(new Waiter(source, d.id, ind.gameTime() + 200));
    }

    void accept(ByteBuffer b, int at, int size, int n) {
        if (n < 0 || IndRecords.Ledger.size(n) > size)
            return;
        List<GoodLine> goods = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            int g = at + IndRecords.Ledger.GOODS + i * IndRecords.Ledger.GOOD;
            goods.add(new GoodLine(b.getInt(g + IndRecords.Ledger.G_GOOD), b.getLong(g + IndRecords.Ledger.G_PER_HOUR),
                    b.getLong(g + IndRecords.Ledger.G_SINCE_START)));
        }
        Line line = new Line(b.getLong(at + IndRecords.Ledger.DISTRICT), b.getInt(at + IndRecords.Ledger.STATE),
                b.getInt(at + IndRecords.Ledger.FREEZE), b.getLong(at + IndRecords.Ledger.NEXT_BREAKPOINT),
                b.getInt(at + IndRecords.Ledger.NEXT_KIND), b.getInt(at + IndRecords.Ledger.NEXT_GOOD),
                b.getLong(at + IndRecords.Ledger.SINCE), List.copyOf(goods), ind.gameTime());
        latest.put(line.district, line);
        received++;
        District d = ind.district(line.district);
        if (d == null)
            return;
        for (Waiter w : new ArrayList<>(waiters)) {
            if (w.district == line.district) {
                waiters.remove(w);
                for (MutableComponent c : debugLines(d))
                    w.source.sendSuccess(() -> c, false);
            }
        }
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

    void tick(long now) {
        if (!waiters.isEmpty())
            waiters.removeIf(w -> {
                if (now <= w.until)
                    return false;
                w.source.sendFailure(Component.literal("mic_rtsim: the engine sent no ledger line for district " + w.district + " (it may not track it yet)"));
                return true;
            });
    }

    // ---- text -----------------------------------------------------------------------------------

    public static String stateName(int state) {
        return switch (state) {
            case IndRecords.Ledger.STATE_LIVE -> "live";
            case IndRecords.Ledger.STATE_LEARNING -> "learning";
            case IndRecords.Ledger.STATE_LEARNED -> "running (simulated)";
            case IndRecords.Ledger.STATE_STALE -> "stale: edited since it was learned";
            case IndRecords.Ledger.STATE_NOT_LEARNABLE -> "not learnable";
            case IndRecords.Ledger.STATE_SUSPENDED -> "suspended: partly loaded";
            case IndRecords.Ledger.STATE_DAMAGED -> "damaged: stopped until reloaded";
            case IndRecords.Ledger.STATE_OUT_OF_SEASON -> "frozen: out of season";
            default -> "state " + state;
        };
    }

    public static String freezeReason(int code) {
        return switch (code) {
            case 0 -> "";
            case IndRecords.NotLearnable.ROTATION_CROSSES -> "rotation crosses the boundary";
            case IndRecords.NotLearnable.ELECTRIC_LINE_CROSSES -> "electric line crosses the boundary";
            case IndRecords.NotLearnable.MIXTURE_NOT_STEADY -> "mixture composition not steady";
            case IndRecords.NotLearnable.FINITE_DEPOSIT_UNKNOWN -> "finite deposit unknown";
            case IndRecords.NotLearnable.MOVING_CONTRAPTION -> "moving drill";
            case IndRecords.NotLearnable.SPAWN_FED_FARM -> "mob drops not simulated";
            case IndRecords.NotLearnable.CONVEYOR_CROSSES -> "a belt, chute or pipe crosses the boundary";
            default -> "not learnable";
        };
    }

    /** A good's name for display: the item's or fluid's translatable name, so each client reads its own language. */
    public MutableComponent goodName(int engineGoodId) {
        Goods.Name n = Goods.parse(ind.engineName(IndRecords.DICT_GOOD, engineGoodId));
        if (n == null)
            return Component.literal("good " + engineGoodId);
        if (n.cls() == 'i') {
            Item item = BuiltInRegistries.ITEM.getOptional(n.id()).orElse(null);
            if (item != null)
                return Component.translatable(item.getDescriptionId());
        } else if (n.cls() == 'f') {
            Fluid f = BuiltInRegistries.FLUID.getOptional(n.id()).orElse(null);
            if (f != null)
                return Component.translatable(f.getFluidType().getDescriptionId());
        } else if (n.cls() == 'e') {
            return Component.literal("FE");
        }
        return Component.literal(n.id().toString());
    }

    private static String duration(long ticks) {
        if (ticks < 0)
            return "now";
        long minutes = ticks / 1200;
        return minutes >= 60 ? (minutes / 60) + " h " + (minutes % 60) + " min" : minutes + " min";
    }

    private static String signed(long v) {
        return v > 0 ? "+" + v : Long.toString(v);
    }

    /** The head line: name and state, with what froze the district if anything did. */
    private String head(District d, Line l) {
        String head = d.displayName() + ": " + stateName(l.state);
        return l.freeze != 0 ? head + " (" + freezeReason(l.freeze) + ")" : head;
    }

    private MutableComponent nextLine(Line l) {
        long in = l.nextBreakpoint < 0 ? -1 : l.nextBreakpoint - ind.gameTime();
        MutableComponent good = l.nextGood == 0 ? Component.literal("") : goodName(l.nextGood);
        return switch (l.nextKind) {
            case IndRecords.Ledger.NEXT_INPUT_EMPTIES -> Component.empty().append(good).append(Component.literal(" runs out in " + duration(in)));
            case IndRecords.Ledger.NEXT_OUTPUT_FULL -> Component.literal("output full in " + duration(in));
            case IndRecords.Ledger.NEXT_TARGET -> Component.empty().append(good).append(Component.literal(" target in " + duration(in)));
            case IndRecords.Ledger.NEXT_DEPOSIT_EXHAUSTED -> Component.literal("deposit exhausted in " + duration(in));
            case IndRecords.Ledger.NEXT_SOURCE_STOPPED -> Component.literal("stops with its power source");
            case IndRecords.Ledger.NEXT_RESTART -> Component.literal("restarts in " + duration(in));
            default -> Component.literal("next: " + l.nextKind);
        };
    }

    /** The operator output: the district, the engine's ledger line with every good, the last reconcile, warnings. */
    public List<MutableComponent> debugLines(District d) {
        List<MutableComponent> out = new ArrayList<>();
        Line l = latest.get(d.id);
        out.add(Component.literal("District " + d.id + " '" + d.displayName() + "': " + d.chunks.size() + " chunks, "
                + d.inventories.size() + " boundary inventories, " + d.stations.size() + " stations, revision " + d.revision
                + (d.notLearnable != 0 ? ", not learnable: " + freezeReason(d.notLearnable) : "")));
        if (l == null) {
            out.add(Component.literal("  no ledger line from the engine yet (use /rtsim district debug)"));
        } else {
            out.add(Component.literal(head(d, l)));
            List<GoodLine> goods = new ArrayList<>(l.goods);
            goods.sort(Comparator.comparingLong((GoodLine g) -> -Math.abs(g.perHour)).thenComparingInt(GoodLine::good));
            for (GoodLine g : goods)
                out.add(Component.literal("  " + signed(g.perHour) + "/h, " + signed(g.sinceStart) + " since tick " + l.since + ": ").append(goodName(g.good)));
            if (l.nextKind != 0)
                out.add(Component.literal("  next: ").append(nextLine(l)));
        }
        for (String w : warnings(d.id))
            out.add(Component.literal("  warning: " + w));
        Reconcile r = reconciles.get(d.id);
        if (r != null)
            out.add(Component.literal("  last reconcile at tick " + r.at + ": " + r.entries + " deltas, " + r.applied + " of " + r.requested
                    + " applied, " + r.shortfalls + " short, " + r.broken + " blocks broken, " + r.skipped + " skipped"));
        return out;
    }
}
