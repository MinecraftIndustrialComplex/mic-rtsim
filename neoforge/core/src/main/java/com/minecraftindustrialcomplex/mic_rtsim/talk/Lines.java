package com.minecraftindustrialcomplex.mic_rtsim.talk;

import net.minecraft.locale.Language;

import java.util.concurrent.ConcurrentHashMap;

/**
 * Where the lines are (TALK.md §8, TK15). A line is the lang key {@code mic_rtsim.talk.<voice>.<topic>.<n>};
 * the variants of a (voice, topic) are counted from {@code n = 0} while the server's own language table
 * has the key, and a voice falls back through its parents (drop the last dotted part), then {@code any}.
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

    private static final Found NONE = new Found("", 0);
    private static final ConcurrentHashMap<String, Found> CACHE = new ConcurrentHashMap<>();

    /** The first prefix in the voice's chain with lines for this topic, or null when there is none. */
    public static Found find(String voice, String topic) {
        Found f = CACHE.computeIfAbsent(voice + "|" + topic, k -> resolve(voice, topic));
        return f == NONE ? null : f;
    }

    private static Found resolve(String voice, String topic) {
        String v = voice;
        while (true) {
            Found f = probe(v, topic);
            if (f != null)
                return f;
            int dot = v.lastIndexOf('.');
            if (dot < 0)
                break;
            v = v.substring(0, dot);
        }
        Found any = probe("any", topic);
        return any == null ? NONE : any;
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
