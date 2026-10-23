package com.minecraftindustrialcomplex.mic_rtsim.talk;

import java.util.concurrent.ConcurrentHashMap;

/**
 * Deterministic place names (TALK.md §6, TQ-3 (a), TK13): two parts from the core's lang lists, chosen
 * by a SplitMix64 hash of the key, so every player and every session hears the same name. Composed
 * from the server's English table and sent as plain text.
 */
public final class Names {
    private Names() {}

    private static final ConcurrentHashMap<Long, String> VILLAGES = new ConcurrentHashMap<>();
    /** A cache this large is emptied: a name is a pure function of its key, so only the work is lost. */
    private static final int BOUND = 4096;

    /** The name of the village with this Java id: "Oak" + "ford". */
    public static String village(long id) {
        if (VILLAGES.size() >= BOUND)
            VILLAGES.clear();
        return VILLAGES.computeIfAbsent(id, Names::compose);
    }

    private static String compose(long id) {
        String a = Lines.ROOT + "name.village.a";
        String b = Lines.ROOT + "name.village.b";
        int na = Lines.count(a), nb = Lines.count(b);
        if (na == 0 || nb == 0)
            return "Village";
        net.minecraft.locale.Language lang = net.minecraft.locale.Language.getInstance();
        int[] ij = indices(id, na, nb);
        return capitalise(lang.getOrDefault(a + "." + ij[0])) + lang.getOrDefault(b + "." + ij[1]);
    }

    private static final ConcurrentHashMap<String, String> OTHERS = new ConcurrentHashMap<>();
    private static final String NONE = "";

    /**
     * A camp's name by its culture and faction key (TK49): "Mud" + "hollow" for goblins, "the Crow Road gang"
     * for bandits, "the Grey Tower" for raiders, from {@code name.<culture>.a/b/format}. Null when the culture
     * has no lists (a described ref instead).
     */
    public static String camp(String culture, long key) {
        return culture == null ? null : twoPart(culture, culture, key);
    }

    /** A pirate haven's name by its key: "Black" + "water" from {@code name.haven.a/b/format}; null with no lists. */
    public static String haven(long key) {
        return twoPart("haven", "haven", key);
    }

    /**
     * A chief's own name by its culture and seed: one entry of {@code name.chief.<culture>.<n>} (Vorr, Gitz,
     * Lugg); null when the culture has none.
     */
    public static String chief(String culture, long seed) {
        if (culture == null)
            return null;
        if (OTHERS.size() >= BOUND)
            OTHERS.clear();
        String name = OTHERS.computeIfAbsent("chief|" + culture + "|" + seed, k -> {
            String base = Lines.ROOT + "name.chief." + culture;
            int n = Lines.count(base);
            if (n == 0)
                return NONE;
            return net.minecraft.locale.Language.getInstance().getOrDefault(base + "." + (int) ((mix(seed) & 0xFFFFFFFFL) % n));
        });
        return name.isEmpty() ? null : name;
    }

    private static String twoPart(String kind, String people, long key) {
        if (OTHERS.size() >= BOUND)
            OTHERS.clear();
        String name = OTHERS.computeIfAbsent(kind + "|" + people + "|" + key, k -> {
            String a = Lines.ROOT + "name." + people + ".a";
            String b = Lines.ROOT + "name." + people + ".b";
            int na = Lines.count(a), nb = Lines.count(b);
            if (na == 0 || nb == 0)
                return NONE;
            net.minecraft.locale.Language lang = net.minecraft.locale.Language.getInstance();
            int[] ij = indices(key, na, nb);
            String format = lang.getOrDefault(Lines.ROOT + "name." + people + ".format");
            // No capital here: "the Crow Road gang" is lower case mid-sentence (Sentences.capitalise opens a sentence with it).
            return String.format(format.contains("%") ? format : "%1$s%2$s",
                    lang.getOrDefault(a + "." + ij[0]), lang.getOrDefault(b + "." + ij[1]));
        });
        return name.isEmpty() ? null : name;
    }

    /** SplitMix64's finaliser over {@code id + golden gamma}. */
    public static long mix(long id) {
        long z = id + 0x9E3779B97F4A7C15L;
        z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
        z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
        return z ^ (z >>> 31);
    }

    /** The index into list a (from the low 32 bits of the hash) and into list b (from the high 32 bits). */
    public static int[] indices(long id, int na, int nb) {
        long z = mix(id);
        return new int[]{(int) ((z & 0xFFFFFFFFL) % na), (int) ((z >>> 32) % nb)};
    }

    private static String capitalise(String s) {
        return s.isEmpty() ? s : Character.toUpperCase(s.charAt(0)) + s.substring(1);
    }

    public static void clear() {
        VILLAGES.clear();
        OTHERS.clear();
    }
}
