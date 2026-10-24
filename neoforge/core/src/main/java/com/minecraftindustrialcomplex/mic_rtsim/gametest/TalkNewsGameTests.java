package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkHolder;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictTalker;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine.TalkActor;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Naming;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Names;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Refs;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talk;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkAsks;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talkers;
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
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.level.GameType;
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

    /**
     * What a report's place can hold (24 bits, rtsim_knowledge::report::COORD_LIMIT): the engine refuses a
     * TALK_REPORT beyond it. The GameTest arenas sit at a random place of each fresh world, up to millions
     * of blocks out and sometimes past that, so a report near the arena is clamped to the edge of what fits.
     */
    private static final int COORD_LIMIT = (1 << 23) - 1;

    private static int held(int v) {
        return Math.max(-COORD_LIMIT - 1, Math.min(COORD_LIMIT, v));
    }

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

    /** Talks to a talker every {@link TalkGameTests#WINDOW} ticks until a line whose key has the marker is heard. */
    private static final class Hunt {
        final ServerLevel level;
        final FakePlayers.Hearer listener;
        final Entity talker;
        final String marker;
        final List<String> keys = new ArrayList<>();
        int talks, seen;
        long next;
        Said found;

        Hunt(ServerLevel level, FakePlayers.Hearer listener, Entity talker, String marker) {
            this.level = level;
            this.listener = listener;
            this.talker = talker;
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
                click(listener, talker, true);
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
        long[] day0 = new long[1];

        helper.startSequence()
                .thenExecute(() -> villager[0] = farmer(helper, 5.5, 5.5))
                .thenWaitUntil(() -> check("the engine knows the village", e.phase1Sentiment(v.id, ben.getUUID()) != Integer.MIN_VALUE))
                .thenExecute(() -> {
                    day0[0] = level.getGameTime() / Talk.DAY;
                    check("the villager is in the village", RtsimApi.villageOf(villager[0]) == v);
                    e.talkReport(v.id, ROBBERY, SEVERITY, 0, TalkActor.player(alex.getUUID()), TalkActor.village(v.id),
                            held(meet.getX() + 60), held(meet.getZ()), AGO);
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
                    // 3 once a game day however many talks; a hunt can span a day boundary of the shared game time.
                    long days = level.getGameTime() / Talk.DAY - day0[0] + 1;
                    check("Ben was credited once a day (" + days + " day(s) spanned): " + minorPositive(villager[0], ben.getUUID()),
                            minorPositive(villager[0], ben.getUUID()) == 3 * days);
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
                    // The report's place is by the talker, 380 blocks from the robber: a described ref at the
                    // report's own place drops its locator (TK29), and this one is not there.
                    e.talkReport(here.id, ROBBERY, SEVERITY, 0, TalkActor.village(east.id), TalkActor.village(here.id),
                            held(meet.getX() + 20), held(meet.getZ()), AGO);
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

    /** A structure the goblin culture claims (rtsim-conflict's goblin.json): the engine founds a camp on it. */
    private static final String GOBLIN_CAMP = "mic_rtsim:conflict/goblin_camp";
    /** A goblin warrior, which stands in as a vindicator (MZB-102). */
    private static final String GOBLIN_WARRIOR = "mic_rtsim:conflict/goblin/warrior";
    /** rtsim-knowledge's ReportKind::Raid and report::raid::PLUNDERED. */
    private static final int RAID = 13, PLUNDERED = 3;

    /**
     * A camp's member tells its camp's news and mood (TALK slice 4, TK71 to TK79): a goblin camp the
     * engine founds (FACTION_FOUND on a structure the goblin culture claims) has a raid it made on its
     * chief's board (TALK_REPORT on the camp's key, TK79); a scouting band of that camp, which has no
     * quarrel with the player, stands by. The member chats (MZB-130), asks about its camp (holder kind
     * 3, the faction block asked for), and tells the raid from its camp's side ({@code by_us}) in the
     * goblin voice, and a camp mood from the faction block, which only the engine's answer can give.
     * The camp itself is founded in the far area ({@link ConflictKit#farBase}): its territory (radius 400
     * for goblins) would otherwise lie over other tests' arenas, and every block they break in it would
     * count as a signal against it too. The camp stays in the reused world when the test ends (the engine
     * keeps a faction it founded); farBase's slots are 1,024 blocks apart and picked at random, so a camp
     * left from an earlier run never reaches another test's slot.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 6000, batch = BATCH + "camp", skyAccess = true)
    public static void talkTellsTheCampNewsAndMood(GameTestHelper helper) {
        NativeEngine e = live(helper, "talk camp");
        if (e == null)
            return;
        ServerLevel level = helper.getLevel();
        String dim = RtsimTests.dimension(helper);
        Vec3 c = RtsimTests.centre(helper);
        BlockPos meet = BlockPos.containing(c);
        List<FakePlayers.Hearer> players = new ArrayList<>();
        arm(helper, players, List.of());
        long agent = ConflictKit.newAgent();
        long[] faction = new long[1];
        Entity[] member = new Entity[1];
        Hunt[] hunt = new Hunt[2];
        FakePlayers.Hearer[] ben = new FakePlayers.Hearer[1];
        long[] answered = new long[1]; // TalkAsks' answers before the member is first asked
        ConflictKit.onEnd(helper, () -> {
            ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 0, c.x, c.z);
            for (Mob m : ConflictKit.bridge().bodies(agent))
                m.discard();
        });

        helper.startSequence()
                .thenExecute(() -> {
                    int[] far = ConflictKit.farBase();
                    BlockPos campAt = new BlockPos(far[0] + 8, 64, far[1] + 8);
                    faction[0] = ConflictKit.bridge().reportCamp(level, GOBLIN_CAMP, campAt, x -> false, null, false, 0, 0);
                    check("a faction key", faction[0] != 0L);
                })
                .thenWaitUntil(() -> check("the engine founded the camp (its TERRITORY came back)",
                        ConflictKit.bridge().territories().get(faction[0]) != null))
                .thenExecute(() -> {
                    e.talkReport(faction[0], RAID, PLUNDERED, 0, new TalkActor(TalkRecords.C_CAMP, faction[0], 0), TalkActor.NONE,
                            held(meet.getX() + 60), held(meet.getZ()), AGO);
                    // Scouts, not hostile, standing still by the arena's centre.
                    ConflictKit.Band band = ConflictKit.band(agent, faction[0], dim);
                    band.purpose = ConflictRecords.PURPOSE_SCOUTS;
                    band.flags = 0;
                    band.arcX = c.x + 2.5;
                    band.arcZ = c.z;
                    band.targetX = c.x + 100;
                    band.targetY = c.y;
                    band.targetZ = c.z;
                    band.speed = 0f;
                    ConflictKit.materialize(band, List.of(new ConflictKit.StackSpec(GOBLIN_WARRIOR, 1, false)));
                })
                .thenWaitUntil(() -> {
                    ConflictBridge.Party p = ConflictKit.party(agent);
                    check("one stack", p.stacks().size() == 1);
                    check("placed", p.stacks().get(0).state() == ConflictBridge.StackState.PLACED);
                })
                .thenExecute(() -> {
                    member[0] = ConflictKit.party(agent).stacks().get(0).slots().get(0).entity();
                    ben[0] = FakePlayers.hearing(level, c.add(-1.5, 0, 0), "talk_camp_ben");
                    players.add(ben[0]);
                    // Creative: a stand-in vindicator finds no target in it.
                    ben[0].setGameMode(GameType.CREATIVE);
                    check("the conflict talker speaks for the member", Talkers.find(member[0], ben[0]) instanceof ConflictTalker);
                    check("it chats with a player its camp has no quarrel with", Talkers.find(member[0], ben[0]).chats(member[0], ben[0]));
                    TalkHolder h = Talkers.find(member[0], ben[0]).holder(member[0]);
                    check("its holder is the camp: " + h, h != null && h.kind() == TalkHolder.FACTION && h.id() == faction[0]);
                    answered[0] = TalkAsks.INSTANCE.answered;
                    hunt[0] = new Hunt(level, ben[0], member[0], ".news.raid.");
                })
                .thenIdle(SETTLE)
                .thenWaitUntil(() -> hunt[0].step())
                .thenExecute(() -> {
                    Said said = hunt[0].found;
                    check("the camp's raid, told from its side in the goblin voice: " + said.key(),
                            said.key().startsWith("mic_rtsim.talk.goblin.news.raid.by_us."));
                    check("the line is the server's English, not its key: " + english(said), !english(said).contains("mic_rtsim."));
                    record("talk camp", "news line", said.key() + " = " + english(said));
                    record("talk camp", "talks until it came up", hunt[0].talks);
                    // The goblin voice has only the faction block's moods (TK78): one heard is the engine's answer.
                    hunt[1] = new Hunt(level, ben[0], member[0], ".goblin.mood.");
                    hunt[1].seen = hunt[0].seen; // only the lines after the news
                    hunt[1].next = level.getGameTime() + WINDOW;
                })
                .thenWaitUntil(() -> hunt[1].step())
                .thenExecute(() -> {
                    Said said = hunt[1].found;
                    record("talk camp", "mood line", said.key() + " = " + english(said));
                    record("talk camp", "lines heard", hunt[0].keys + " then " + hunt[1].keys);
                    check("the engine answered the member's asks", TalkAsks.INSTANCE.answered > answered[0]);
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
        FakePlayers.Hearer kid = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(3.5, 2, 7.5)), "talk_mute_kid");
        arm(helper, List.of(ben, kid), List.of(v));
        TalkAsks.mute = true;
        Villager[] villager = new Villager[1];
        Villager[] child = new Villager[1];
        long[] before = new long[3]; // open, timed out, written to the engine

        helper.startSequence()
                .thenExecute(() -> {
                    villager[0] = farmer(helper, 5.5, 5.5);
                    child[0] = helper.spawn(net.minecraft.world.entity.EntityType.VILLAGER, new Vec3(4.5, 2, 5.5));
                    child[0].setAge(-24_000);
                    child[0].setPersistenceRequired();
                })
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    check("the villager has a holder", RtsimApi.villageOf(villager[0]) == v);
                    // A child is in the village but has no holder: it speaks at once and opens no ask (TK16).
                    check("the child is a baby in the village", child[0].isBaby() && RtsimApi.villageOf(child[0]) == v);
                    check("a child has no holder", Talkers.find(child[0], kid).holder(child[0]) == null);
                    before[0] = TalkAsks.INSTANCE.openCount();
                    click(kid, child[0], true);
                    check("the child spoke at once: " + keys(kid), kid.heard.size() == 1);
                    check("and opened no ask", TalkAsks.INSTANCE.openCount() == before[0]);
                    check("and none of its lines is news: " + keys(kid), !Said.of(kid.heard.get(0)).key().contains(".news."));
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
