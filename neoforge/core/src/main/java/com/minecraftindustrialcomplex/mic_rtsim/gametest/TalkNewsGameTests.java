package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine.TalkActor;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Naming;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Names;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Refs;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talk;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkAsks;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.TalkGameTests.SETTLE;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.TalkGameTests.WINDOW;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.TalkGameTests.click;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.TalkGameTests.farmer;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.TalkGameTests.keys;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.TalkGameTests.minorPositive;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.TalkGameTests.translatable;

/**
 * TALK slice 2b (TALK.md §9, docs/boundary.md §13): a talker in a village the engine drives asks it what it
 * knows (TALK_ASK) and tells the answer (TALK_FACTS) as news; with no answer it speaks without it; a talk
 * whose talker is gone says nothing. Native engine only: the stub never answers.
 *
 * <p>Each test is in a batch of its own: they set the process-wide hooks {@link Refs#force} and
 * {@link TalkAsks#mute}, which tests in one batch (they run side by side) would share. The village is the
 * arena itself, its box wide enough that a villager who wanders stays in it, and every hook, player and
 * village is put back when the test ends however it ends. The news is put on the village's board with the
 * engine's test order TALK_REPORT (a robbery by a named player, a day ago); the talker is clicked until it
 * tells it, since the picker draws among its fluff and moods too.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class TalkNewsGameTests {
    private TalkNewsGameTests() {}

    private static final String BATCH = "mic_rtsim_p1_live_talk_";
    /** The village is the arena, wide enough for a wandering villager. */
    private static final int HALF = 30;
    /** A report from less than a day back, so the freshness is the highest (TK50). */
    private static final long AGO = 20_000;
    /** A hunt for one kind of line gives up after this many talks (each one rate limit apart). */
    private static final int MAX_TALKS = 40;

    private static final int ROBBERY = 3, SEVERITY = 2;

    private static NativeEngine live(GameTestHelper helper, String test) {
        if (RtsimTests.engine() instanceof NativeEngine e)
            return e;
        record(test, "skipped", "the stub engine never answers a TALK_ASK; this test needs the native engine");
        helper.succeed();
        return null;
    }

    /** What a test must put back: the hooks, the players it added, the villages it adopted. */
    private static void arm(GameTestHelper helper, List<FakePlayers.Hearer> players, List<Village> villages) {
        helper.testInfo.addListener(new GameTestListener() {
            private void restore() {
                Refs.force = Naming.Force.NONE;
                TalkAsks.mute = false;
                for (FakePlayers.Hearer p : players)
                    FakePlayers.remove(p);
                for (Village v : villages)
                    RtsimTests.forget(v);
            }

            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                restore();
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                restore();
            }

            @Override
            public void testAddedForRerun(GameTestInfo oldTest, GameTestInfo newTest, GameTestRunner runner) {}
        });
    }

    /** One line a talker said: the key of its line and the line's arguments. */
    private record Said(String key, Object[] args, Component line) {
        static Said of(Component say) {
            TranslatableContents s = translatable(say);
            Component line = (Component) s.getArgs()[1];
            TranslatableContents t = translatable(line);
            return new Said(t.getKey(), t.getArgs(), line);
        }

        String arg(int i) {
            Object a = args[i];
            return a instanceof Component c ? c.getString() : String.valueOf(a);
        }
    }

    /** Talks to a villager every {@link TalkGameTests#WINDOW} ticks until a line whose key has the marker is heard. */
    private static final class Hunt {
        final ServerLevel level;
        final FakePlayers.Hearer listener;
        final Villager villager;
        final String marker;
        final List<String> keys = new ArrayList<>();
        int talks, seen;
        long next;
        Said found;

        Hunt(ServerLevel level, FakePlayers.Hearer listener, Villager villager, String marker) {
            this.level = level;
            this.listener = listener;
            this.villager = villager;
            this.marker = marker;
        }

        /** Throws until the line is heard, as a wait condition does. */
        void step() {
            while (seen < listener.heard.size()) {
                Said s = Said.of(listener.heard.get(seen++));
                keys.add(s.key());
                if (found == null && s.key().contains(marker))
                    found = s;
            }
            if (found != null)
                return;
            long now = level.getGameTime();
            if (now >= next) {
                check("a " + marker + " line within " + MAX_TALKS + " talks, heard " + keys, talks < MAX_TALKS);
                click(listener, villager, true);
                talks++;
                next = now + WINDOW;
            }
            check("no " + marker + " line yet after " + talks + " talks: " + keys, false);
        }
    }

    /** The English of a line as the player reads it. */
    private static String english(Said s) {
        return s.line().getString();
    }

    /**
     * The village's own news: a robbery by a named player is on its board, and a talker of the village
     * tells it, in the third person about "us", with the thief's name; the name is forced (TK52) so the
     * chance that the talker would only describe a far player does not decide the test.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 4000, batch = BATCH + "news")
    public static void talkTellsTheVillageNews(GameTestHelper helper) {
        NativeEngine e = live(helper, "talk news");
        if (e == null)
            return;
        ServerLevel level = helper.getLevel();
        BlockPos meet = BlockPos.containing(RtsimTests.centre(helper));
        Village v = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", meet, HALF, 20);
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(7.5, 2, 7.5)), "talk_news_ben");
        FakePlayers.Hearer alex = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(3.5, 2, 7.5)), "Alex");
        arm(helper, List.of(ben, alex), List.of(v));
        Refs.force = Naming.Force.NAMED;
        Villager[] villager = new Villager[1];
        Hunt[] hunt = new Hunt[1];

        helper.startSequence()
                .thenExecute(() -> villager[0] = farmer(helper, 5.5, 5.5))
                .thenWaitUntil(() -> check("the engine knows the village", e.phase1Sentiment(v.id, ben.getUUID()) != Integer.MIN_VALUE))
                .thenExecute(() -> {
                    check("the villager is in the village", RtsimApi.villageOf(villager[0]) == v);
                    e.talkReport(v.id, ROBBERY, SEVERITY, 0, TalkActor.player(alex.getUUID()), TalkActor.village(v.id),
                            meet.getX() + 60, meet.getZ(), AGO);
                    hunt[0] = new Hunt(level, ben, villager[0], ".news.robbery.");
                })
                .thenIdle(SETTLE)
                .thenWaitUntil(() -> hunt[0].step())
                .thenExecute(() -> {
                    Said said = hunt[0].found;
                    check("it is a robbery told about us: " + said.key(), said.key().contains(".news.robbery.us."));
                    check("it names the thief: " + said.arg(0), said.arg(0).equals("Alex"));
                    check("it says when (today): " + said.arg(2), said.arg(2).equalsIgnoreCase("today"));
                    check("it says where, from the talker: " + said.arg(3), !said.arg(3).isEmpty());
                    check("the line is the server's English, not its key: " + english(said), !english(said).contains("mic_rtsim."));
                    check("one line per talk: " + hunt[0].keys.size() + " lines in " + hunt[0].talks + " talks",
                            hunt[0].keys.size() == hunt[0].talks);
                    // Once a game day, 3 each; the hunt can span a day boundary of the shared game time.
                    check("Ben was credited: " + minorPositive(villager[0], ben.getUUID()), minorPositive(villager[0], ben.getUUID()) >= 3);
                    record("talk news", "line", said.key() + " = " + english(said));
                    record("talk news", "talks until it came up", hunt[0].talks);
                    record("talk news", "lines heard", hunt[0].keys);
                    record("talk news", "asks (asked, answered, timed out)", TalkAsks.INSTANCE.asked + ", " + TalkAsks.INSTANCE.answered
                            + ", " + TalkAsks.INSTANCE.timedOut);
                    check("the engine answered asks", TalkAsks.INSTANCE.answered > 0);
                })
                .thenSucceed();
    }

    /**
     * A ref told by what it is and where (TALK.md §6, TK29): another village robs this one, the ref is
     * forced to be described, and it reads as a village in the direction it lies, never by its name.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 4000, batch = BATCH + "described")
    public static void talkDescribesWhatItDoesNotName(GameTestHelper helper) {
        NativeEngine e = live(helper, "talk described");
        if (e == null)
            return;
        ServerLevel level = helper.getLevel();
        BlockPos meet = BlockPos.containing(RtsimTests.centre(helper));
        Village here = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", meet, HALF, 20);
        // 400 blocks east of the talker; its blocks are never loaded.
        Village east = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_taiga", meet.offset(400, 0, 0), 20, 20);
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(7.5, 2, 7.5)), "talk_desc_ben");
        arm(helper, List.of(ben), List.of(here, east));
        Refs.force = Naming.Force.DESCRIBED;
        Villager[] villager = new Villager[1];
        Hunt[] hunt = new Hunt[1];

        helper.startSequence()
                .thenExecute(() -> villager[0] = farmer(helper, 5.5, 5.5))
                .thenWaitUntil(() -> {
                    check("the engine knows both villages", e.phase1Sentiment(here.id, ben.getUUID()) != Integer.MIN_VALUE
                            && e.phase1Sentiment(east.id, ben.getUUID()) != Integer.MIN_VALUE);
                })
                .thenExecute(() -> {
                    check("the villager is in the village", RtsimApi.villageOf(villager[0]) == here);
                    // At the other village's side of the world, well away from the villager.
                    e.talkReport(here.id, ROBBERY, SEVERITY, 0, TalkActor.village(east.id), TalkActor.village(here.id),
                            meet.getX() + 20, meet.getZ(), AGO);
                    hunt[0] = new Hunt(level, ben, villager[0], ".news.robbery.");
                })
                .thenIdle(SETTLE)
                .thenWaitUntil(() -> hunt[0].step())
                .thenExecute(() -> {
                    Said said = hunt[0].found;
                    String who = said.arg(0);
                    check("a robbery told about us: " + said.key(), said.key().contains(".news.robbery.us."));
                    check("the robber is a village: " + who, who.toLowerCase().contains("village"));
                    check("not called by its name: " + who, !who.contains(Names.village(east.id)));
                    check("it lies to the east: " + who, who.contains("east"));
                    record("talk described", "line", said.key() + " = " + english(said));
                    record("talk described", "the ref", who);
                })
                .thenSucceed();
    }

    /**
     * An engine that never answers (TK5): the talker still speaks, after the ask's timeout, one line of
     * its own fluff or mood, never two, and credits the player once.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400, batch = BATCH + "mute")
    public static void talkSpeaksWithoutAnAnswer(GameTestHelper helper) {
        NativeEngine e = live(helper, "talk mute");
        if (e == null)
            return;
        ServerLevel level = helper.getLevel();
        BlockPos meet = BlockPos.containing(RtsimTests.centre(helper));
        Village v = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", meet, HALF, 20);
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(7.5, 2, 7.5)), "talk_mute_ben");
        arm(helper, List.of(ben), List.of(v));
        TalkAsks.mute = true;
        Villager[] villager = new Villager[1];
        long[] before = new long[3]; // open, timed out, written to the engine

        helper.startSequence()
                .thenExecute(() -> villager[0] = farmer(helper, 5.5, 5.5))
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    check("the villager has a holder", RtsimApi.villageOf(villager[0]) == v);
                    before[0] = TalkAsks.INSTANCE.openCount();
                    before[1] = TalkAsks.INSTANCE.timedOut;
                    before[2] = TalkAsks.INSTANCE.flushed;
                    click(ben, villager[0], true);
                    check("the talk waits for the answer: " + keys(ben), ben.heard.isEmpty());
                    check("an ask is open", TalkAsks.INSTANCE.openCount() == before[0] + 1);
                })
                .thenIdle(10)
                .thenExecute(() -> check("still nothing, the ask has not timed out: " + keys(ben), ben.heard.isEmpty()))
                .thenIdle(TalkAsks.TIMEOUT_TICKS - 10 + 5)
                .thenExecute(() -> {
                    check("one line after the timeout: " + keys(ben), ben.heard.size() == 1);
                    Said said = Said.of(ben.heard.get(0));
                    check("not news, the engine said nothing: " + said.key(), !said.key().contains(".news."));
                    check("a line of the core's own: " + said.key(), said.key().startsWith("mic_rtsim.talk."));
                    check("the ask timed out once: " + (TalkAsks.INSTANCE.timedOut - before[1]), TalkAsks.INSTANCE.timedOut == before[1] + 1);
                    check("the engine heard of no ask: " + (TalkAsks.INSTANCE.flushed - before[2]), TalkAsks.INSTANCE.flushed == before[2]);
                    check("none open any more", TalkAsks.INSTANCE.openCount() == before[0]);
                    check("credited once: " + minorPositive(villager[0], ben.getUUID()), minorPositive(villager[0], ben.getUUID()) == 3);
                    record("talk mute", "line", said.key() + " = " + english(said));
                })
                .thenIdle(WINDOW)
                .thenExecute(() -> {
                    check("never a second line: " + keys(ben), ben.heard.size() == 1);
                    check("nor a second credit: " + minorPositive(villager[0], ben.getUUID()), minorPositive(villager[0], ben.getUUID()) == 3);
                })
                .thenSucceed();
    }

    /** A pending talk whose talker is removed says nothing and credits nothing (TK54). */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400, batch = BATCH + "gone")
    public static void talkOfARemovedTalkerSaysNothing(GameTestHelper helper) {
        NativeEngine e = live(helper, "talk gone");
        if (e == null)
            return;
        ServerLevel level = helper.getLevel();
        BlockPos meet = BlockPos.containing(RtsimTests.centre(helper));
        Village v = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", meet, HALF, 20);
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(7.5, 2, 7.5)), "talk_gone_ben");
        arm(helper, List.of(ben), List.of(v));
        Villager[] villager = new Villager[1];
        int[] dropped = new int[1];
        long[] open = new long[1];

        helper.startSequence()
                .thenExecute(() -> villager[0] = farmer(helper, 5.5, 5.5))
                .thenWaitUntil(() -> check("the engine knows the village", e.phase1Sentiment(v.id, ben.getUUID()) != Integer.MIN_VALUE))
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    check("the villager has a holder", RtsimApi.villageOf(villager[0]) == v);
                    dropped[0] = Talk.dropped;
                    open[0] = TalkAsks.INSTANCE.openCount();
                    click(ben, villager[0], true);
                    check("the talk waits for the answer: " + keys(ben), ben.heard.isEmpty());
                    check("an ask is open", TalkAsks.INSTANCE.openCount() == open[0] + 1);
                    // Gone before the answer can come: it is a later tick that reads the engine's records.
                    villager[0].discard();
                })
                .thenIdle(TalkAsks.TIMEOUT_TICKS + 10)
                .thenExecute(() -> {
                    check("the ask is closed: " + TalkAsks.INSTANCE.openCount(), TalkAsks.INSTANCE.openCount() == open[0]);
                    check("nothing was said: " + keys(ben), ben.heard.isEmpty());
                    check("and no message of any kind to Ben: " + keys(ben), ben.raw.isEmpty());
                    check("the talk was dropped, once: " + (Talk.dropped - dropped[0]), Talk.dropped == dropped[0] + 1);
                    record("talk gone", "dropped", Talk.dropped - dropped[0]);
                })
                .thenSucceed();
    }
}
