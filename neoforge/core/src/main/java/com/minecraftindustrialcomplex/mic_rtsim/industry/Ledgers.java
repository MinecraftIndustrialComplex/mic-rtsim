package com.minecraftindustrialcomplex.mic_rtsim.industry;

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
 * The read-only ledger (phase I-A, I18): the engine's IND_LEDGER replies, kept per district, and the
 * text the "District ledger" display source and {@code /rtsim district ledger} show. The ledger never
 * advances the sim (review fix 12); asking for one is a latest-wins IND_LEDGER_REQUEST.
 */
public final class Ledgers {

    /** One good's line: rate per real hour (72,000 ticks, F30) and the amount since {@code since}. */
    public record GoodLine(int good, long perHour, long sinceStart) {}

    /** The latest IND_LEDGER of a district. */
    public record Line(long district, int state, int freeze, long nextBreakpoint, int nextKind, int nextGood, long since,
                       List<GoodLine> goods, long receivedAt) {}

    /** The last reconcile applied to a district: deltas, shortfalls and blocks, for the chat ledger. */
    public record Reconcile(long at, int entries, long requested, long applied, int shortfalls, int broken, int skipped) {}

    private record Waiter(CommandSourceStack source, long district, long until) {}

    private final Industry ind;
    private final Map<Long, Line> latest = new HashMap<>();
    private final Map<Long, Reconcile> reconciles = new HashMap<>();
    private final Map<Long, Long> requestedAt = new HashMap<>();
    private final List<Waiter> waiters = new ArrayList<>();
    /** Warnings plugins attach to a district's ledger, by key (I16: a gauge network that spans districts). */
    private final Map<Long, Map<String, String>> warnings = new HashMap<>();
    long received;

    /** Sets (or, with null text, clears) a warning on a district's ledger. */
    public void warn(long district, String key, String text) {
        if (text == null) {
            Map<String, String> m = warnings.get(district);
            if (m != null && m.remove(key) != null && m.isEmpty())
                warnings.remove(district);
            return;
        }
        warnings.computeIfAbsent(district, k -> new java.util.TreeMap<>()).put(key, text);
    }

    /** The warnings on a district's ledger, by key order. */
    public List<String> warnings(long district) {
        Map<String, String> m = warnings.get(district);
        return m == null ? List.of() : List.copyOf(m.values());
    }

    Ledgers(Industry ind) {
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

    /** {@code /rtsim district ledger}: shows what is known now, and the engine's next reply when it comes. */
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
                for (MutableComponent c : chatLines(d))
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
                w.source.sendFailure(Component.literal("mic_rtsim: the engine sent no ledger for district " + w.district + " (it may not track it yet)"));
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

    /** The display source's lines: name and state, the top three goods per hour, the next breakpoint. */
    public List<MutableComponent> displayLines(District d, int maxLines) {
        List<MutableComponent> out = new ArrayList<>();
        Line l = latest.get(d.id);
        if (l == null) {
            out.add(Component.literal(d.displayName() + ": no ledger yet"));
            for (String w : warnings(d.id))
                if (out.size() < maxLines)
                    out.add(Component.literal(w));
            return out;
        }
        String head = d.displayName() + ": " + stateName(l.state);
        if (l.freeze != 0)
            head += " (" + freezeReason(l.freeze) + ")";
        out.add(Component.literal(head));
        List<GoodLine> goods = new ArrayList<>(l.goods);
        goods.sort(Comparator.comparingLong((GoodLine g) -> -Math.abs(g.perHour)).thenComparingInt(GoodLine::good));
        boolean simulated = l.state != IndRecords.Ledger.STATE_LIVE && l.state != IndRecords.Ledger.STATE_LEARNING;
        for (GoodLine g : goods) {
            if (out.size() >= maxLines - (l.nextKind != 0 ? 1 : 0))
                break;
            MutableComponent c = Component.literal(signed(g.perHour) + "/h ").append(goodName(g.good));
            if (simulated && g.sinceStart != 0)
                c.append(Component.literal(" (" + (g.sinceStart > 0 ? "would have made " : "would have used ") + Math.abs(g.sinceStart) + ")"));
            out.add(c);
        }
        if (l.nextKind != 0 && out.size() < maxLines)
            out.add(nextLine(l));
        for (String w : warnings(d.id))
            if (out.size() < maxLines)
                out.add(Component.literal(w));
        return out;
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

    /** The chat ledger: everything the display shows, every good, the last reconcile, and what froze it. */
    public List<MutableComponent> chatLines(District d) {
        List<MutableComponent> out = new ArrayList<>();
        Line l = latest.get(d.id);
        out.add(Component.literal("District " + d.id + " '" + d.displayName() + "': " + d.chunks.size() + " chunks, "
                + d.inventories.size() + " boundary inventories, " + d.stations.size() + " stations, revision " + d.revision
                + (d.notLearnable != 0 ? ", not learnable: " + freezeReason(d.notLearnable) : "")));
        if (l == null) {
            out.add(Component.literal("  no ledger from the engine yet"));
        } else {
            out.addAll(displayLines(d, 1).subList(0, 1));
            for (GoodLine g : l.goods)
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
