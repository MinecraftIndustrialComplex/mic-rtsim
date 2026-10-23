package com.minecraftindustrialcomplex.mic_rtsim.talk;

import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/**
 * A talk cancels the click's main-hand and off-hand interactions, but the client then also "uses" the
 * off-hand item (an ender pearl, food, a bow) as a separate action. This remembers which players talked
 * in which game tick, so that off-hand use is cancelled too (TALK.md §2). Bounded: stale entries go.
 */
public final class OffHandGuard {
    static final int BOUND = 256;
    /** The use packet follows the interact packets in the same tick, or at worst the next. */
    static final long WINDOW = 1;

    private final Map<UUID, Long> talked = new HashMap<>();

    public void talked(UUID player, long gameTime) {
        if (talked.size() >= BOUND)
            talked.values().removeIf(t -> gameTime < t || gameTime - t > WINDOW);
        talked.put(player, gameTime);
    }

    /** True when this player talked in this game tick or the one before. */
    public boolean justTalked(UUID player, long gameTime) {
        Long t = talked.get(player);
        return t != null && gameTime >= t && gameTime - t <= WINDOW;
    }

    int size() {
        return talked.size();
    }

    public void clear() {
        talked.clear();
    }
}
