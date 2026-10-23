package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Facts;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.News;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Ref;
import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.function.IntPredicate;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The engine-side moods an answer gives (TALK.md §5.2, TK4, TK53, TK56). */
class TalkMoodsTest {
    private static final long DAY = TalkTuning.DAY;
    private static final Ref CAMP = new Ref(TalkRecords.C_CAMP, 0, 1, 77, 0, 0, 0);
    private static final Ref DRAGON = new Ref(TalkRecords.C_DRAGON, 0, 2, 5, 6, 0, 0);
    private static final Ref VILLAGE = new Ref(TalkRecords.C_VILLAGE, 0, 3, 9, 0, 0, 0);
    private static final IntPredicate NO_FOOD = id -> false;
    private static final IntPredicate ALL_FOOD = id -> true;

    private static News news(int kind, int talkFlags, long ago, Ref who) {
        return new News(kind, 1, 0, talkFlags, ago, 0, 0, who, VILLAGE, Ref.NONE);
    }

    private static Facts facts(int flags, int scarcest, int scarcestRatio, int glutted, int gluttedRatio, News... news) {
        return new Facts(1, flags, 0, scarcest, scarcestRatio, glutted, gluttedRatio, List.of(news));
    }

    private static Facts withNews(News... news) {
        return facts(TalkRecords.F_HOLDER, 0, 0, 0, 0, news);
    }

    private static List<String> topics(Facts f, IntPredicate food) {
        return TalkMoods.of(f, food).stream().map(TalkMoods.Mood::topic).toList();
    }

    @Test
    void noAnswerAndAnUnknownHolderGiveNoMood() {
        assertTrue(TalkMoods.of(null, NO_FOOD).isEmpty());
        // Everything in the record is ignored when the engine does not know the holder.
        assertTrue(TalkMoods.of(facts(TalkRecords.F_PORT_CLOSED, 4, 10, 5, 9000, news(3, TalkRecords.TO_US, 0, CAMP)), NO_FOOD).isEmpty());
    }

    @Test
    void safetyNewsAboutUsIsAfraidWithinTwoDaysAndUneasyWithinTen() {
        assertEquals(List.of("mood.afraid"), topics(withNews(news(3, TalkRecords.TO_US, 0, CAMP)), NO_FOOD));
        assertEquals(List.of("mood.afraid"), topics(withNews(news(3, TalkRecords.TO_US, 2 * DAY - 1, CAMP)), NO_FOOD));
        assertEquals(List.of("mood.uneasy"), topics(withNews(news(3, TalkRecords.TO_US, 2 * DAY, CAMP)), NO_FOOD));
        assertEquals(List.of("mood.uneasy"), topics(withNews(news(3, TalkRecords.TO_US, 10 * DAY - 1, CAMP)), NO_FOOD));
        assertEquals(List.of(), topics(withNews(news(3, TalkRecords.TO_US, 10 * DAY, CAMP)), NO_FOOD));
        assertEquals(4.0, TalkMoods.of(withNews(news(3, TalkRecords.TO_US, 0, CAMP)), NO_FOOD).get(0).weight());
        assertEquals(2.0, TalkMoods.of(withNews(news(3, TalkRecords.TO_US, 3 * DAY, CAMP)), NO_FOOD).get(0).weight());
    }

    @Test
    void theFreshestSafetyItemDecidesAndOtherKindsAndOtherPeopleDoNotCount() {
        assertEquals(List.of("mood.afraid"), topics(withNews(
                news(3, TalkRecords.TO_US, 5 * DAY, CAMP), news(13, TalkRecords.TO_US, DAY, CAMP)), NO_FOOD));
        // Not about us: a robbery somewhere else.
        assertEquals(List.of(), topics(withNews(news(3, 0, 0, CAMP)), NO_FOOD));
        assertEquals(List.of(), topics(withNews(news(3, TalkRecords.TO_YOU, 0, CAMP)), NO_FOOD));
        // A trade or a rescue is not a danger.
        assertEquals(List.of(), topics(withNews(news(5, TalkRecords.TO_US, 0, CAMP)), NO_FOOD));
        assertEquals(List.of(), topics(withNews(news(6, TalkRecords.TO_US, 0, CAMP)), NO_FOOD));
        // Every safety kind of the spec counts.
        for (int kind : new int[]{1, 2, 3, 7, 8, 13, 16})
            assertEquals(List.of("mood.afraid"), topics(withNews(news(kind, TalkRecords.TO_US, 0, CAMP)), NO_FOOD), "kind " + kind);
    }

    @Test
    void aFeudAgainstUsGivesTheFeudMoodWithTheCampAsItsArgument() {
        List<TalkMoods.Mood> m = TalkMoods.of(withNews(news(10, TalkRecords.TO_US, 3 * DAY, CAMP)), NO_FOOD);
        assertEquals(1, m.size());
        assertEquals("mood.feud", m.get(0).topic());
        assertEquals(TalkMoods.Arg.WHO, m.get(0).arg());
        assertEquals(CAMP, m.get(0).source().who());
        assertEquals(2.0, m.get(0).weight());
        // A feud against someone else is not our trouble.
        assertEquals(List.of(), topics(withNews(news(10, 0, 0, CAMP)), NO_FOOD));
        assertEquals(List.of(), topics(withNews(news(10, TalkRecords.TO_YOU, 0, CAMP)), NO_FOOD));
    }

    @Test
    void aDragonsAssaultOnUsOrSeenNearUsWithinTenDaysGivesTheDragonMood() {
        assertEquals(List.of("mood.afraid", "mood.dragon"), topics(withNews(news(1, TalkRecords.TO_US, DAY, DRAGON)), NO_FOOD));
        assertEquals(List.of("mood.dragon"), topics(withNews(news(1, TalkRecords.SEEN, DAY, DRAGON)), NO_FOOD));
        assertEquals(List.of(), topics(withNews(news(1, TalkRecords.SEEN, 10 * DAY, DRAGON)), NO_FOOD));
        // Heard of, not near: no mood. Not a dragon: no dragon mood.
        assertEquals(List.of(), topics(withNews(news(1, 0, DAY, DRAGON)), NO_FOOD));
        assertEquals(List.of("mood.uneasy"), topics(withNews(news(1, TalkRecords.TO_US, 5 * DAY, CAMP)), NO_FOOD));
        // Only an assault.
        assertEquals(List.of(), topics(withNews(news(5, TalkRecords.SEEN, DAY, DRAGON)), NO_FOOD));
    }

    @Test
    void theScarcestGoodUnderHalfItsTargetIsDear() {
        List<TalkMoods.Mood> m = TalkMoods.of(facts(TalkRecords.F_HOLDER, 7, 499, 0, 0), NO_FOOD);
        assertEquals(1, m.size());
        assertEquals("mood.dear", m.get(0).topic());
        assertEquals(TalkMoods.Arg.SCARCEST, m.get(0).arg());
        assertEquals(1.5, m.get(0).weight());
        assertEquals(List.of(), topics(facts(TalkRecords.F_HOLDER, 7, 500, 0, 0), NO_FOOD));
        assertEquals(List.of(), topics(facts(TalkRecords.F_HOLDER, 0, 10, 0, 0), NO_FOOD), "good 0 is none");
    }

    @Test
    void aScarceFoodUnderAThirdIsHungerInsteadOfDear() {
        assertEquals(List.of("mood.hungry"), topics(facts(TalkRecords.F_HOLDER, 7, 332, 0, 0), ALL_FOOD));
        assertEquals(List.of("mood.dear"), topics(facts(TalkRecords.F_HOLDER, 7, 333, 0, 0), ALL_FOOD));
        assertEquals(List.of("mood.dear"), topics(facts(TalkRecords.F_HOLDER, 7, 100, 0, 0), NO_FOOD));
        assertEquals(TalkMoods.Arg.NONE, TalkMoods.of(facts(TalkRecords.F_HOLDER, 7, 100, 0, 0), ALL_FOOD).get(0).arg());
    }

    @Test
    void theGlutOverTwiceItsTargetIsCheap() {
        List<TalkMoods.Mood> m = TalkMoods.of(facts(TalkRecords.F_HOLDER, 0, 0, 8, 2001), NO_FOOD);
        assertEquals(1, m.size());
        assertEquals("mood.cheap", m.get(0).topic());
        assertEquals(TalkMoods.Arg.GLUTTED, m.get(0).arg());
        assertEquals(List.of(), topics(facts(TalkRecords.F_HOLDER, 0, 0, 8, 2000), NO_FOOD));
        // The ratio is unsigned on the wire.
        assertEquals(List.of("mood.cheap"), topics(facts(TalkRecords.F_HOLDER, 0, 0, 8, -1), NO_FOOD));
        assertEquals(List.of(), topics(facts(TalkRecords.F_HOLDER, 0, 0, 0, 5000), NO_FOOD), "good 0 is none");
    }

    @Test
    void aClosedPortAndABlockadeEachGiveTheirMood() {
        assertEquals(List.of("mood.port_closed"), topics(facts(TalkRecords.F_HOLDER | TalkRecords.F_PORT_CLOSED, 0, 0, 0, 0), NO_FOOD));
        assertEquals(List.of("mood.blockade"), topics(facts(TalkRecords.F_HOLDER | TalkRecords.F_BLOCKADED, 0, 0, 0, 0), NO_FOOD));
        assertEquals(List.of("mood.port_closed", "mood.blockade"),
                topics(facts(TalkRecords.F_HOLDER | TalkRecords.F_PORT_CLOSED | TalkRecords.F_BLOCKADED, 0, 0, 0, 0), NO_FOOD));
        assertEquals(2.0, TalkMoods.of(facts(TalkRecords.F_HOLDER | TalkRecords.F_PORT_CLOSED, 0, 0, 0, 0), NO_FOOD).get(0).weight());
    }
}
