package com.minecraftindustrialcomplex.mic_rtsim.talk;

import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/**
 * TK3: one talker answers the same player at most once per {@link #PAIR} game ticks and anyone at
 * most once per {@link #ANY}. In memory, bounded: stale entries are dropped when the maps grow.
 */
public final class RateLimit {
    public static final long PAIR = 40, ANY = 10;
    static final int BOUND = 512;

    private record Pair(UUID talker, UUID player) {}

    private final Map<Pair, Long> pairs = new HashMap<>();
    private final Map<UUID, Long> talkers = new HashMap<>();

    /** True (and recorded) when the talker may answer this player now. */
    public boolean tryAcquire(UUID talker, UUID player, long now) {
        Long anyLast = talkers.get(talker);
        if (anyLast != null && now >= anyLast && now - anyLast < ANY)
            return false;
        Pair key = new Pair(talker, player);
        Long pairLast = pairs.get(key);
        if (pairLast != null && now >= pairLast && now - pairLast < PAIR)
            return false;
        if (pairs.size() >= BOUND)
            pairs.values().removeIf(t -> now < t || now - t >= PAIR);
        if (talkers.size() >= BOUND)
            talkers.values().removeIf(t -> now < t || now - t >= ANY);
        pairs.put(key, now);
        talkers.put(talker, now);
        return true;
    }

    int size() {
        return pairs.size() + talkers.size();
    }

    public void clear() {
        pairs.clear();
        talkers.clear();
    }
}
