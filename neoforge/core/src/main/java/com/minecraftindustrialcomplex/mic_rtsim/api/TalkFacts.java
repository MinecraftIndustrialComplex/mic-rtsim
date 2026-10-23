package com.minecraftindustrialcomplex.mic_rtsim.api;

/**
 * What the engine told the core about a talker's holder (TALK.md §9.2), for a talker's own
 * {@link Talker#candidates}: the core already turns the news, the prices and the port into lines itself.
 * Null in {@link TalkContext#facts()} when no answer came (no session, a stub engine, a timeout, or a talker
 * with no holder).
 *
 * @param holderKnown   the engine knows the holder; false means the rest is empty
 * @param sentiment     the holder's sentiment toward the talking player, rtsim-knowledge's scale (0 for a stranger)
 * @param portClosed    the holder's port is closed
 * @param blockaded     the holder's port is blockaded
 * @param scarcest      the item id of the market's scarcest good, or null
 * @param scarcestRatio its stock per target, thousandths
 * @param glutted       the item id of the market's most glutted good, or null
 * @param gluttedRatio  its stock per target, thousandths
 * @param news          how many news items came with the answer (at most 4)
 */
public record TalkFacts(boolean holderKnown, int sentiment, boolean portClosed, boolean blockaded,
                        String scarcest, long scarcestRatio, String glutted, long gluttedRatio, int news) {}
