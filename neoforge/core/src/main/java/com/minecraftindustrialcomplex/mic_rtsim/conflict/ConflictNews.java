package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import java.util.UUID;

/**
 * Where conflict news would reach players, and does not yet (MZB-101). Ben's decision of 2026-10-22: the
 * conflict sends nothing to chat, the action bar or titles; feuds, pacts, raids and deaths reach players
 * only as gossip that traders, caravans and villagers carry, through the news-by-messengers system
 * ({@code rtsim-trade} news, {@code village/GossipWriter.java}). That routing is being built on branch
 * {@code cl/news-in-world} (the mic-rtsim-clnews worktree); this bridge builds no news path of its own.
 *
 * <p>{@link #post} is the hook it calls where a plan step would have told a player something in words
 * (a notice played, a demand made or settled, a raid at the gate, a chief killed). It does nothing.
 * TODO(cl/news-in-world): route these to the gossip system, or delete the hook if the engine's own news
 * already covers them.
 */
public final class ConflictNews {
    private ConflictNews() {}

    /** What happened, for the news system to word. */
    public enum Event {
        NOTICE_PLAYED, DEMAND_MADE, DEMAND_SETTLED, BAND_ARRIVED, CHIEF_KILLED
    }

    /** Counted for GameTests: how often the hook was reached (nothing else happens). */
    static long posted;

    /**
     * News of {@code event} about {@code faction} for {@code player} (null when it is about nobody in
     * particular). A no-op until cl/news-in-world lands: never chat, the action bar or a title.
     */
    public static void post(Event event, long faction, UUID player) {
        posted++;
    }

    public static long posted() {
        return posted;
    }
}
