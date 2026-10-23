package com.minecraftindustrialcomplex.mic_rtsim.talk;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.List;
import java.util.random.RandomGenerator;

/**
 * Picks one line (TALK.md §5.6, TK4, TK22). Pure: no Minecraft types.
 *
 * <p>A topic is chosen by weight; a topic whose every variant is among the player's last
 * {@link #HISTORY} spoken keys loses half its weight; a variant is chosen at random among those not in
 * the history (the least recent one when all are). The same key never comes twice in a row unless it
 * is the only line there is.
 */
public final class Picker {
    private Picker() {}

    public static final int HISTORY = 8;

    /** A topic with lines: its key prefix, its variant count, its weight, and the arguments of its lines. */
    public record Option(String topic, String prefix, int count, double weight, Object[] args) {
        String key(int n) {
            return prefix + "." + n;
        }
    }

    public record Pick(Option option, String key) {}

    /** The pick, or null when no option has a line. {@code recent} is oldest first. */
    public static Pick pick(List<Option> options, List<String> recent, RandomGenerator rnd) {
        String last = recent.isEmpty() ? null : recent.get(recent.size() - 1);
        List<Option> live = new ArrayList<>();
        for (Option o : options)
            if (o.count > 0 && o.weight > 0)
                live.add(o);
        if (live.isEmpty())
            return null;
        // Never the same key twice in a row while anything else can be said.
        List<Option> fresh = new ArrayList<>(live);
        fresh.removeIf(o -> o.count == 1 && o.key(0).equals(last));
        if (!fresh.isEmpty())
            live = fresh;

        double total = 0;
        double[] w = new double[live.size()];
        for (int i = 0; i < w.length; i++) {
            Option o = live.get(i);
            w[i] = allHeard(o, recent) ? o.weight / 2 : o.weight;
            total += w[i];
        }
        double r = rnd.nextDouble() * total;
        int at = w.length - 1;
        for (int i = 0; i < w.length; i++) {
            r -= w[i];
            if (r < 0) {
                at = i;
                break;
            }
        }
        Option o = live.get(at);

        List<Integer> unheard = new ArrayList<>();
        for (int n = 0; n < o.count; n++)
            if (!recent.contains(o.key(n)))
                unheard.add(n);
        if (!unheard.isEmpty())
            return new Pick(o, o.key(unheard.get(rnd.nextInt(unheard.size()))));
        // All heard: the least recent, which is never the last unless it is the only one.
        int best = 0, bestAt = Integer.MAX_VALUE;
        for (int n = 0; n < o.count; n++) {
            int idx = recent.indexOf(o.key(n));
            if (idx < bestAt) {
                bestAt = idx;
                best = n;
            }
        }
        return new Pick(o, o.key(best));
    }

    private static boolean allHeard(Option o, List<String> recent) {
        for (int n = 0; n < o.count; n++)
            if (!recent.contains(o.key(n)))
                return false;
        return true;
    }

    /** Appends a spoken key, keeping the last {@link #HISTORY}. */
    public static void remember(Deque<String> history, String key) {
        history.remove(key);
        history.addLast(key);
        while (history.size() > HISTORY)
            history.removeFirst();
    }

    public static Deque<String> newHistory() {
        return new ArrayDeque<>();
    }
}
