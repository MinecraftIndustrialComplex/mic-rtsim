package com.minecraftindustrialcomplex.mic_rtsim.knowledge;

import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/**
 * The engine's sentiment snapshots (SENTIMENT), per holder (a village): how it feels about each
 * player, on rtsim-knowledge's scale, with the engine's thresholds. Read by the ribbit sorcerers'
 * buff filter (PLAN.md §8.8) and by ribbit merchants' prices (§8.7). Per session: the engine sends them
 * again after a start.
 */
public final class Sentiments {

    /** One holder's snapshot. */
    public static final class Snapshot {
        public final int dislikeAtOrBelow;
        public final int friendAtOrAbove;
        final Map<UUID, Integer> byPlayer = new HashMap<>();

        Snapshot(int dislike, int friend) {
            this.dislikeAtOrBelow = dislike;
            this.friendAtOrAbove = friend;
        }

        /** The sentiment toward a player; 0 (neutral) when the holder has none. */
        public int of(UUID player) {
            return byPlayer.getOrDefault(player, 0);
        }

        public boolean dislikes(UUID player) {
            Integer s = byPlayer.get(player);
            return s != null && s <= dislikeAtOrBelow;
        }

        public boolean friend(UUID player) {
            Integer s = byPlayer.get(player);
            return s != null && s >= friendAtOrAbove;
        }

        public int size() {
            return byPlayer.size();
        }
    }

    private final Map<Long, Snapshot> byHolder = new HashMap<>();

    /** Applies a SENTIMENT command: a full snapshot replaces the last one, otherwise entries update it. */
    public Snapshot update(long holder, int dislike, int friend, boolean replace) {
        Snapshot old = byHolder.get(holder);
        Snapshot s = new Snapshot(dislike, friend);
        if (!replace && old != null)
            s.byPlayer.putAll(old.byPlayer);
        byHolder.put(holder, s);
        return s;
    }

    public void put(Snapshot s, UUID player, int sentiment) {
        if (sentiment == 0)
            s.byPlayer.remove(player);
        else
            s.byPlayer.put(player, sentiment);
    }

    /** The holder's snapshot, or null when the engine has sent none. */
    public Snapshot get(long holder) {
        return byHolder.get(holder);
    }

    public int holders() {
        return byHolder.size();
    }
}
