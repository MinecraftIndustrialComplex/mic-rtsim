package com.minecraftindustrialcomplex.mic_rtsim.api;

/**
 * Whose noticeboard and market a talker speaks from (TALK.md §9.2). Unused until slice 2 (news).
 *
 * @param kind one of {@link #VILLAGE}, {@link #CARAVAN}, {@link #FACTION}, {@link #SHIP}
 * @param id   the holder's id in its kind: Java's village id, a puppet's agent, a camp's key
 */
public record TalkHolder(int kind, long id) {
    public static final int VILLAGE = 1;
    public static final int CARAVAN = 2;
    public static final int FACTION = 3;
    public static final int SHIP = 4;
}
