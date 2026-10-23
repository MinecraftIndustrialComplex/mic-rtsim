package com.minecraftindustrialcomplex.mic_rtsim.talk;

import net.minecraft.locale.Language;

import java.util.concurrent.ConcurrentHashMap;

/**
 * Where the lines are (TALK.md §8, TK15). A line is the lang key {@code mic_rtsim.talk.<voice>.<topic>.<n>};
 * the variants of a (voice, topic) are counted from {@code n = 0} while the server's own language table
 * has the key. A voice combines its levels, the voice and then its parents (drop the last dotted part), and
 * falls back to {@code any} only when none of them has lines (TK31).
 * Memoized; the cache is cleared when a server starts and stops.
 */
public final class Lines {
    private Lines() {}

    public static final String ROOT = "mic_rtsim.talk.";

    /** The key prefix that has lines and how many variants it has. */
    public record Found(String prefix, int count) {
        public String key(int n) {
            return prefix + "." + n;
        }
    }

    private static final java.util.List<Found> NONE = java.util.List.of();
    private static final ConcurrentHashMap<String, java.util.List<Found>> CACHE = new ConcurrentHashMap<>();

    /**
     * Every level of the voice's chain that has lines for this topic, most specific first (TK31): a
     * farmer's {@code villager.farmer.fluff} and then {@code villager.fluff}. {@code any} is used only
     * when no more specific level has lines. Empty when there is none.
     */
    public static java.util.List<Found> levels(String voice, String topic) {
        return CACHE.computeIfAbsent(voice + "|" + topic, k -> resolve(voice, topic));
    }

    /** The most specific level with lines, or null. */
    public static Found find(String voice, String topic) {
        java.util.List<Found> l = levels(voice, topic);
        return l.isEmpty() ? null : l.get(0);
    }

    private static java.util.List<Found> resolve(String voice, String topic) {
        java.util.List<Found> out = new java.util.ArrayList<>();
        String v = voice;
        while (true) {
            Found f = probe(v, topic);
            if (f != null)
                out.add(f);
            int dot = v.lastIndexOf('.');
            if (dot < 0)
                break;
            v = v.substring(0, dot);
        }
        if (out.isEmpty()) {
            Found any = probe("any", topic);
            if (any != null)
                out.add(any);
        }
        return out.isEmpty() ? NONE : java.util.List.copyOf(out);
    }

    private static Found probe(String voice, String topic) {
        String prefix = ROOT + voice + "." + topic;
        int n = count(prefix);
        return n == 0 ? null : new Found(prefix, n);
    }

    /** How many variants {@code prefix.0}, {@code prefix.1}, ... the server's language has. */
    public static int count(String prefix) {
        Language lang = Language.getInstance();
        int n = 0;
        while (lang.has(prefix + "." + n))
            n++;
        return n;
    }

    public static void clear() {
        CACHE.clear();
    }
}
