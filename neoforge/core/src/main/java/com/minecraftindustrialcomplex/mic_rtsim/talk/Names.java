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

    /** The name of the village with this Java id: "Oak" + "ford". */
    public static String village(long id) {
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
    }
}
