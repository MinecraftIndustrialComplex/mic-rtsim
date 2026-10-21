package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.LongPredicate;
import java.util.function.Predicate;
import java.util.function.ToLongFunction;

/**
 * The game-free part of {@link MinecraftGlassAccess#tickOrder}: places in a sequence of ticker lists, so a
 * unit test can run it without Minecraft on the classpath.
 */
final class TickOrder {
    private TickOrder() {}

    /** A ticker's position when it has none. Never a real block: x would be -2^25, past the world border. */
    static final long NO_POS = Long.MIN_VALUE;

    /**
     * Each wanted position's place over {@code lists} in order, counting every ticker (a removed one or one
     * with no position keeps its place but gives none). The first place wins when a position repeats.
     */
    static <T> Map<Long, Integer> places(List<? extends List<? extends T>> lists, Predicate<? super T> removed,
            ToLongFunction<? super T> pos, LongPredicate wanted) {
        Map<Long, Integer> out = new HashMap<>();
        int i = 0;
        for (List<? extends T> list : lists) {
            for (T t : list) {
                if (!removed.test(t)) {
                    long p = pos.applyAsLong(t);
                    if (p != NO_POS && wanted.test(p))
                        out.putIfAbsent(p, i);
                }
                i++;
            }
        }
        return out;
    }
}
