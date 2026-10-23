package com.minecraftindustrialcomplex.mic_rtsim.talk;

import java.util.ArrayList;
import java.util.List;

/**
 * Every number the engine's answer is turned into speech with (TALK.md §5.2, §5.3, §6; TK4, TK29, TK49 to
 * TK56): weights, thresholds and bands, in one place. All of it is TUNABLE; none of it is on the wire.
 * Pure: no Minecraft types, so a unit test loads it.
 */
public final class TalkTuning {
    private TalkTuning() {}

    /** The game day, ticks. */
    public static final long DAY = 24_000L;

    // ---- mood weights (TUNABLE; the core's own raid, monsters and reputation weights are in Talk and the talkers) ----

    /** A safety news item (TO_US) within 2 days: afraid. TK4. */
    public static final double MOOD_AFRAID = 4.0;
    /** The same within 10 days: uneasy. TK4. */
    public static final double MOOD_UNEASY = 2.0;
    /** A camp's feud with the holder (TK56). */
    public static final double MOOD_FEUD = 2.0;
    /** A dragon's assault on the holder, or one seen near it (TK56). */
    public static final double MOOD_DRAGON = 3.0;
    /** The scarcest good is dear, the glutted one cheap, a scarce food hunger. TK4. */
    public static final double MOOD_PRICES = 1.5;
    /** A closed port or a blockade (TK56). */
    public static final double MOOD_PORT = 2.0;
    /** A cowed camp (its chief fell lately): as loud as fear. TK78. */
    public static final double MOOD_COWED = 4.0;
    /** A camp whose chief changed lately. TK78. */
    public static final double MOOD_NEW_CHIEF = 3.0;
    /** A camp whose war chief is out with a band. TK78. */
    public static final double MOOD_CHIEF_OUT = 3.0;
    /** A camp that has noticed the asking player (its feud is at rung 1 or more). TK78. */
    public static final double MOOD_GRUDGE = 3.0;
    /** A camp short of warriors. TK78. */
    public static final double MOOD_LEAN = 2.0;
    /** A camp at its full strength. TK78. */
    public static final double MOOD_STRONG = 1.5;

    // ---- mood thresholds (TUNABLE) ----

    public static final long AFRAID_TICKS = 2 * DAY;
    public static final long UNEASY_TICKS = 10 * DAY;
    public static final long DRAGON_TICKS = 10 * DAY;
    /** Stock per target, thousandths: under this a good is dear. */
    public static final int DEAR_BELOW = 500;
    /** Over this a good is cheap. */
    public static final int CHEAP_ABOVE = 2000;
    /** A food under this is hunger rather than a price. */
    public static final int HUNGRY_BELOW = 333;

    // ---- news weights (TUNABLE) ----

    /** 3 x freshness x 2 when about the player x 1.5 when about us. TK4. */
    public static final double NEWS_BASE = 3.0;
    public static final double NEWS_ABOUT_PLAYER = 2.0;
    public static final double NEWS_ABOUT_US = 1.5;

    /** Freshness by ticks ago: today, yesterday, under 7 days, older (TK50). */
    public static final double FRESH_TODAY = 1.0, FRESH_YESTERDAY = 0.75, FRESH_WEEK = 0.5, FRESH_OLD = 0.25;

    public static double freshness(long ago) {
        if (ago < DAY)
            return FRESH_TODAY;
        if (ago < 2 * DAY)
            return FRESH_YESTERDAY;
        if (ago < 7 * DAY)
            return FRESH_WEEK;
        return FRESH_OLD;
    }

    /** The picker weight of one news item: 3 x freshness, x 2 about the player, x 1.5 about us. */
    public static double newsWeight(int talkFlags, long ago) {
        double w = NEWS_BASE * freshness(ago);
        if ((talkFlags & (TalkRecords.YOU_DID | TalkRecords.TO_YOU)) != 0)
            w *= NEWS_ABOUT_PLAYER;
        if ((talkFlags & (TalkRecords.TO_US | TalkRecords.BY_US)) != 0)
            w *= NEWS_ABOUT_US;
        return w;
    }

    // ---- when (TALK.md §5.3) ----

    /** The {@code when.<band>} of a report this many ticks ago. */
    public static String when(long ago) {
        if (ago < DAY)
            return "today";
        if (ago < 2 * DAY)
            return "yesterday";
        if (ago < 4 * DAY)
            return "days";
        if (ago < 10 * DAY)
            return "week";
        if (ago < 20 * DAY)
            return "weeks";
        return "long";
    }

    // ---- where (TALK.md §5.3, TK29) ----

    /** Distance bands, blocks: right here, close by, a short walk, a day's ride, far off. */
    public static final int HERE = 24, CLOSE = 128, WALK = 600, RIDE = 3000;

    /** The {@code where.<band>} of a place this far (dx, dz) from the talker. */
    public static String where(int dx, int dz) {
        long d2 = (long) dx * dx + (long) dz * dz;
        if (d2 < (long) HERE * HERE)
            return "here";
        if (d2 < (long) CLOSE * CLOSE)
            return "close";
        if (d2 < (long) WALK * WALK)
            return "walk";
        if (d2 < (long) RIDE * RIDE)
            return "ride";
        return "far";
    }

    /** The distance in blocks, for the naming thresholds. */
    public static double distance(int dx, int dz) {
        return Math.sqrt((double) dx * dx + (double) dz * dz);
    }

    private static final String[] COMPASS = {"n", "ne", "e", "se", "s", "sw", "w", "nw"};

    /**
     * The {@code dir.<key>} of a vector, an 8-point compass with north at -z and east at +x (Minecraft's
     * own). A zero vector reads as north.
     */
    public static String compass(int dx, int dz) {
        double deg = Math.toDegrees(Math.atan2(dx, -dz));
        int i = (int) Math.round(deg / 45.0);
        return COMPASS[((i % 8) + 8) % 8];
    }

    // ---- perspective and kinds ----------------------------------------------------------------

    /**
     * The perspectives a news item can be told from, best first (TK51): the holder's own side before the
     * listener's ({@code us}, {@code by_us}), then {@code you} (the listener did it), {@code to_you} (it was
     * done to the listener), and {@code other} last, which always applies. The holder's side wins because a
     * villager told that "you robbed us" says "Since you robbed us today, we've been counting every crust"
     * (the line's {@code %1$s} is "you"), not "You robbed Oakford", and one that "we hit you" says
     * {@code by_us} ("We hit you"), not {@code to_you} ("Heard Oakford hit you").
     */
    public static List<String> perspectives(int talkFlags) {
        List<String> out = new ArrayList<>(3);
        if ((talkFlags & TalkRecords.TO_US) != 0)
            out.add("us");
        if ((talkFlags & TalkRecords.BY_US) != 0)
            out.add("by_us");
        if ((talkFlags & TalkRecords.YOU_DID) != 0)
            out.add("you");
        if ((talkFlags & TalkRecords.TO_YOU) != 0)
            out.add("to_you");
        out.add("other");
        return out;
    }

    /**
     * Whether an item that would be told from {@code chosen} is told at all. One the holder did (BY_US) is
     * told only from its own perspective ({@code by_us}) or {@code us}: with neither it would fall to
     * {@code other} and read "Word is us attacked Oakford", so it is dropped (TK57); and so is one that
     * would fall to {@code to_you}, which names the holder in the third person ("Heard Mudhollow nearly
     * killed you", TK81).
     */
    public static boolean tellable(int talkFlags, String chosen) {
        return !((chosen.equals("other") || chosen.equals("to_you")) && (talkFlags & TalkRecords.BY_US) != 0);
    }

    /** The first perspective of {@link #perspectives}: the one a news item is told from when its topic has lines. */
    public static String perspective(int talkFlags) {
        return perspectives(talkFlags).get(0);
    }

    /** The lang names of rtsim-knowledge's report kinds, by id (the gaps are spare or reserved); null for one this build does not know. */
    private static final String[] KINDS = new String[21];

    static {
        KINDS[1] = "assault";
        KINDS[2] = "killing";
        KINDS[3] = "robbery";
        KINDS[4] = "theft";
        KINDS[5] = "trade";
        KINDS[6] = "rescue";
        KINDS[7] = "resident_assault";
        KINDS[8] = "resident_killing";
        KINDS[9] = "encroachment";
        KINDS[10] = "feud";
        KINDS[11] = "pact_sworn";
        KINDS[12] = "pact_broken";
        KINDS[13] = "raid";
        KINDS[14] = "fell";
        KINDS[16] = "port_raided";
        KINDS[17] = "blockade";
        KINDS[18] = "port_closed";
        KINDS[19] = "port_reopened";
        KINDS[20] = "haven_fell";
    }

    public static String kindName(int kind) {
        return kind >= 0 && kind < KINDS.length ? KINDS[kind] : null;
    }

    /** Report kinds the safety mood reads (an assault, killing, robbery, raid or port raid; resident assault and killing too). */
    public static boolean isSafetyKind(int kind) {
        return switch (kind) {
            case 1, 2, 3, 7, 8, 13, 16 -> true;
            default -> false;
        };
    }

    public static final int KIND_ASSAULT = 1, KIND_FEUD = 10;
}
