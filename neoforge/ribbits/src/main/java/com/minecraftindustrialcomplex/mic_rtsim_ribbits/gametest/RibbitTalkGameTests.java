package com.minecraftindustrialcomplex.mic_rtsim_ribbits.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkCandidates;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkContext;
import com.minecraftindustrialcomplex.mic_rtsim.api.TalkHolder;
import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Lines;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Names;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talk;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkAsks;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkDays;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talkers;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.MicRtsimRibbits;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.RibbitBands;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.RibbitMerchantAdapter;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.RibbitTalker;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.Ribbits;
import com.yungnickyoung.minecraft.ribbits.data.RibbitProfession;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.module.RibbitProfessionModule;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.inventory.MerchantMenu;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * TALK slice 3 on the real Ribbits jar (TALK.md §4, §7): ribbits talk in their profession's voice, a
 * crouching empty hand opens no trade screen, a talk with a resident credits its village once a game day per
 * player (the engine hears one TALKED and applies it once), and a puppet talks and credits nothing. The
 * sentiment the engine ends up with is read on the native engine only: the stub keeps no sentiment of its own.
 *
 * <p>Clicks are simulated as the server's packet handler makes them (the shift state, then {@code interactOn}
 * for each hand). The line of a talker with a holder arrives when the engine answers or, on the stub,
 * after the ask's timeout, so a test waits for it. The tests that count asks have a batch of their own: the
 * counter is the process's.
 */
@GameTestHolder(MicRtsimRibbits.MODID)
@PrefixGameTestTemplate(false)
public final class RibbitTalkGameTests {
    private RibbitTalkGameTests() {}

    private static final String BATCH = "mic_rtsim_ribbits_talk";
    /** The village is the arena (a village is its own box and nothing more, so arenas side by side do not overlap). */
    private static final int HALF = 5;
    /** The tick a spawned ribbit needs before it talks. */
    private static final int SETTLE = 3;
    /** More than the pair window of the rate limit (TK3). */
    private static final int WINDOW = 45;
    /** Ticks to give a record that must not arrive to arrive: the journal reaches the engine within the tick or the next. */
    private static final int QUIET = 12;

    // ---- helpers ----------------------------------------------------------------------------------

    /** What a test must put back, however it ends: its players, villages, entities and anything else. */
    private static final class Scene {
        final List<FakePlayers.Hearer> players = new ArrayList<>();
        final List<Village> villages = new ArrayList<>();
        final List<Entity> entities = new ArrayList<>();
        final List<Runnable> after = new ArrayList<>();

        FakePlayers.Hearer player(ServerLevel level, Vec3 at, String name) {
            FakePlayers.Hearer p = FakePlayers.hearing(level, at, name);
            players.add(p);
            return p;
        }

        <E extends Entity> E entity(E e) {
            entities.add(e);
            return e;
        }

        void restore() {
            for (FakePlayers.Hearer p : players)
                FakePlayers.remove(p);
            for (Runnable r : after)
                r.run();
            for (Entity e : entities)
                e.discard();
            for (Village v : villages)
                RtsimTests.forget(v);
        }

        void arm(GameTestHelper helper) {
            helper.testInfo.addListener(new GameTestListener() {
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
    }

    /** A ribbit village over the arena: the engine's own on the native engine (it applies a TALKED), a scripted one on the stub. */
    private static Village village(GameTestHelper helper, Scene scene) {
        Village v = RtsimTests.engine() instanceof NativeEngine
                ? RtsimTests.adoptArenaLive(helper, MicRtsimRibbits.CULTURE, "ribbits:ribbit_village",
                        BlockPos.containing(RtsimTests.centre(helper)), HALF)
                : RtsimTests.adoptArena(helper, MicRtsimRibbits.CULTURE);
        scene.villages.add(v);
        return v;
    }

    private static InteractionResult[] click(ServerPlayer player, Entity target, boolean crouch) {
        player.setShiftKeyDown(crouch);
        InteractionResult main = player.interactOn(target, InteractionHand.MAIN_HAND);
        InteractionResult off = player.interactOn(target, InteractionHand.OFF_HAND);
        return new InteractionResult[]{main, off};
    }

    private static TranslatableContents translatable(Component c) {
        check("a translatable component: " + c, c.getContents() instanceof TranslatableContents);
        return (TranslatableContents) c.getContents();
    }

    private static String keys(FakePlayers.Hearer p) {
        StringBuilder sb = new StringBuilder("[");
        for (Component c : p.raw)
            sb.append(c.getContents() instanceof TranslatableContents t ? t.getKey() : c.getString()).append("; ");
        return sb.append("]").toString();
    }

    /** The talk line {@code heard} at index {@code i}: the say template's arguments, speaker and line. */
    private record Said(TranslatableContents say, Component speaker, TranslatableContents line) {
        static Said of(Component message) {
            TranslatableContents say = translatable(message);
            check("the say template: " + say.getKey(), say.getKey().equals("mic_rtsim.talk.say"));
            check("with a speaker and a line", say.getArgs().length == 2 && say.getArgs()[0] instanceof Component && say.getArgs()[1] instanceof Component);
            return new Said(say, (Component) say.getArgs()[0], translatable((Component) say.getArgs()[1]));
        }

        TranslatableContents speakerContents() {
            return translatable(speaker);
        }
    }

    /** The TALKED records the engine received for this village and player, not skipped. */
    private static List<ScriptedEngine.Received> talked(ScriptedEngine engine, long village, UUID player) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        for (ScriptedEngine.Received r : engine.received(TalkRecords.TALKED)) {
            if (r.skipped() || r.bytes().length < TalkRecords.Talked.SIZE)
                continue;
            ByteBuffer b = r.view();
            if (b.getLong(TalkRecords.Talked.VILLAGE) == village
                    && b.getLong(TalkRecords.Talked.PLAYER_MOST) == player.getMostSignificantBits()
                    && b.getLong(TalkRecords.Talked.PLAYER_LEAST) == player.getLeastSignificantBits())
                out.add(r);
        }
        return out;
    }

    /** Every TALKED the engine received for this player, whichever village. */
    private static int talkedBy(ScriptedEngine engine, UUID player) {
        int n = 0;
        for (ScriptedEngine.Received r : engine.received(TalkRecords.TALKED)) {
            if (r.skipped() || r.bytes().length < TalkRecords.Talked.SIZE)
                continue;
            ByteBuffer b = r.view();
            if (b.getLong(TalkRecords.Talked.PLAYER_MOST) == player.getMostSignificantBits()
                    && b.getLong(TalkRecords.Talked.PLAYER_LEAST) == player.getLeastSignificantBits())
                n++;
        }
        return n;
    }

    private static long today(ServerLevel level) {
        return level.getGameTime() / Talk.DAY;
    }

    // ---- the credit -------------------------------------------------------------------------------

    /**
     * A crouching empty-hand click on a resident ribbit speaks in its profession's voice and journals one
     * TALKED; a second the same day speaks and journals nothing; so does a second resident of the same village
     * (the cap is the village's, kept on the player: per ribbit it would be +30 from ten ribbits, TK60); after
     * the day passes a talk journals again. On the native engine the village's sentiment toward the player is
     * +3, then +6. In a batch of its own: the test counts the process's asks.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 900, batch = BATCH + "_credit")
    public static void ribbit_talk_credits_the_village_once_a_day(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Scene scene = new Scene();
        scene.arm(helper);
        ScriptedEngine engine = RtsimTests.engine();
        NativeEngine live = engine instanceof NativeEngine n ? n : null;
        Village v = village(helper, scene);
        Vec3 c = RtsimTests.centre(helper);
        FakePlayers.Hearer ben = scene.player(level, helper.absoluteVec(new Vec3(5.5, 2, 8.5)), "ribbit_talk_ben");
        RibbitEntity[] ribbit = new RibbitEntity[2];
        int[] heard = new int[1];
        long[] asks = new long[1];
        long[] day = new long[1];
        int[] base = new int[1];
        ScriptedEngine.Received[] first = new ScriptedEngine.Received[1];

        helper.startSequence()
                .thenExecute(() -> {
                    ribbit[0] = scene.entity(RibbitGameTests.spawn(helper, c.add(-1.5, 0, 0), RibbitProfessionModule.GARDENER));
                    ribbit[1] = scene.entity(RibbitGameTests.spawn(helper, c.add(1.5, 0, 0), RibbitProfessionModule.FISHERMAN));
                    ribbit[0].setNoAi(true);
                    ribbit[1].setNoAi(true);
                })
                .thenWaitUntil(() -> check("the engine knows the village",
                        live == null || live.phase1Sentiment(v.id, ben.getUUID()) != Integer.MIN_VALUE))
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    check("both ribbits live in the village", RtsimApi.villageOf(ribbit[0]) == v && RtsimApi.villageOf(ribbit[1]) == v);
                    check("the gardener's talker is the plugin's", Talkers.find(ribbit[0], ben) instanceof RibbitTalker);
                    check("Ben has no talk days yet", ben.getData(RibbitTalker.DAYS).isEmpty());
                    check("the ribbits have no talk days yet", ribbit[0].getData(RtsimAttachments.TALK_DAYS).isEmpty()
                            && ribbit[1].getData(RtsimAttachments.TALK_DAYS).isEmpty());
                    check("the engine has heard no TALKED about Ben", talked(engine, v.id, ben.getUUID()).isEmpty());
                    if (live != null)
                        base[0] = live.phase1Sentiment(v.id, ben.getUUID());
                    heard[0] = ben.heard.size();
                    asks[0] = TalkAsks.INSTANCE.asked;
                    day[0] = today(level);
                    InteractionResult[] r = click(ben, ribbit[0], true);
                    check("the main hand swings once: " + r[0], r[0] == InteractionResult.SUCCESS);
                    check("the off hand is consumed silently: " + r[1], r[1] == InteractionResult.CONSUME);
                    // The stub never answers, so on it no ask opens and the talker speaks at once (TK49).
                    check("a resident asks its village what it knows: " + (TalkAsks.INSTANCE.asked - asks[0]) + " ask(s)",
                            live == null || TalkAsks.INSTANCE.asked - asks[0] == 1);
                })
                .thenWaitUntil(() -> check("the gardener's line arrived", ben.heard.size() > heard[0]))
                .thenExecute(() -> {
                    check("exactly one talk line: " + keys(ben), ben.heard.size() == heard[0] + 1);
                    Said said = Said.of(ben.heard.get(heard[0]));
                    check("a ribbit line: " + said.line().getKey(), said.line().getKey().startsWith("mic_rtsim.talk.ribbit."));
                    check("it carries the server's English: " + said.line().getFallback(),
                            said.line().getFallback() != null && !said.line().getFallback().equals(said.line().getKey()));
                    TranslatableContents who = said.speakerContents();
                    check("a placed ribbit has a name and a profession: " + who.getKey(), who.getKey().equals("mic_rtsim.talk.speaker.ribbit"));
                    check("the name is the village's list's: " + who.getArgs()[0], who.getArgs()[0] instanceof String s && !s.isBlank());
                    check("and the profession is the gardener's: " + said.speaker().getString(), said.speaker().getString().endsWith("Gardener"));
                    check("the day was written on Ben, for this village: " + ben.getData(RibbitTalker.DAYS),
                            ben.getData(RibbitTalker.DAYS).lastDay(RibbitTalker.key(v.id)) >= day[0]
                                    && ben.getData(RibbitTalker.DAYS).lastDay(RibbitTalker.key(v.id)) <= day[0] + 1);
                    check("and the core wrote the gardener's own day", ribbit[0].getData(RtsimAttachments.TALK_DAYS).lastDay(ben.getUUID()) >= day[0]);
                    record("ribbit talk", "line", said.line().getKey() + " = " + said.line().getFallback());
                    record("ribbit talk", "speaker", said.speaker().getString());
                })
                .thenWaitUntil(() -> check("the engine received the TALKED", talked(engine, v.id, ben.getUUID()).size() >= 1))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> t = talked(engine, v.id, ben.getUUID());
                    check("exactly one TALKED: " + t.size(), t.size() == 1);
                    first[0] = t.get(0);
                    ByteBuffer b = first[0].view();
                    check("it names the village and the player", b.getLong(TalkRecords.Talked.VILLAGE) == v.id);
                    check("it carries a Java op id: " + Long.toHexString(b.getLong(TalkRecords.Talked.OP_ID)), b.getLong(TalkRecords.Talked.OP_ID) < 0);
                    check("and the body that talked: " + engine.javaDictName(b.getInt(TalkRecords.Talked.BODY)),
                            "ribbits:ribbit".equals(engine.javaDictName(b.getInt(TalkRecords.Talked.BODY))));
                })
                .thenIdle(WINDOW)
                .thenExecute(() -> {
                    // Today as of this very click, so a day boundary between the clicks cannot credit again.
                    ben.setData(RibbitTalker.DAYS, TalkDays.EMPTY.with(RibbitTalker.key(v.id), today(level)));
                    heard[0] = ben.heard.size();
                    click(ben, ribbit[0], true);
                })
                .thenWaitUntil(() -> check("the same ribbit speaks again", ben.heard.size() > heard[0]))
                .thenIdle(QUIET)
                .thenExecute(() -> {
                    check("the second talk with the same ribbit journals nothing: " + talked(engine, v.id, ben.getUUID()).size(),
                            talked(engine, v.id, ben.getUUID()).size() == 1);
                    // A second resident of the same village, on a ribbit with no day of its own: the cap is the village's.
                    check("the fisherman has never talked to Ben", ribbit[1].getData(RtsimAttachments.TALK_DAYS).isEmpty());
                    ben.setData(RibbitTalker.DAYS, TalkDays.EMPTY.with(RibbitTalker.key(v.id), today(level)));
                    heard[0] = ben.heard.size();
                    click(ben, ribbit[1], true);
                })
                .thenWaitUntil(() -> check("the fisherman speaks", ben.heard.size() > heard[0]))
                .thenIdle(QUIET)
                .thenExecute(() -> {
                    Said said = Said.of(ben.heard.get(heard[0]));
                    check("in his own voice: " + said.speaker().getString(), said.speaker().getString().endsWith("Fisherman"));
                    check("the second ribbit of the village journals nothing the same day: " + talked(engine, v.id, ben.getUUID()).size(),
                            talked(engine, v.id, ben.getUUID()).size() == 1);
                    check("and the core wrote no day on it", ribbit[1].getData(RtsimAttachments.TALK_DAYS).isEmpty());
                    if (live == null)
                        record("ribbit talk", "sentiment", "stub engine: the sentiment is not read");
                })
                .thenWaitUntil(() -> check("the village's sentiment toward Ben rose by the credit, 3",
                        live == null || live.phase1Sentiment(v.id, ben.getUUID()) != base[0]))
                .thenExecute(() -> {
                    if (live == null)
                        return;
                    int s = live.phase1Sentiment(v.id, ben.getUUID());
                    check("exactly +3 after one credit though three talks: " + s + " from " + base[0], s == base[0] + 3);
                    var snapshot = RtsimApi.sentiment(v.id);
                    record("ribbit talk", "sentiment (engine, snapshot)", s + ", " + (snapshot == null ? "none yet" : snapshot.of(ben.getUUID())));
                    check("a village that likes a player a little is not his friend: talk alone makes none",
                            snapshot == null || !snapshot.friend(ben.getUUID()));
                })
                .thenExecute(() -> {
                    // The day rolls over, emulated by moving both records back one day (other suites share the game time,
                    // so game time itself is not advanced): Ben's village entry, and the gardener's own, which the core
                    // wrote. Both are yesterday's, so only the day comparison lets the next talk through.
                    long d = today(level);
                    check("Ben's entry was today's before it moves", ben.getData(RibbitTalker.DAYS).creditedOn(RibbitTalker.key(v.id), d)
                            || ben.getData(RibbitTalker.DAYS).creditedOn(RibbitTalker.key(v.id), d - 1));
                    ben.setData(RibbitTalker.DAYS, TalkDays.EMPTY.with(RibbitTalker.key(v.id), d - 1));
                    ribbit[0].setData(RtsimAttachments.TALK_DAYS, TalkDays.EMPTY.with(ben.getUUID(), d - 1));
                    check("yesterday's entry does not block a talk today",
                            Talkers.find(ribbit[0], ben).credits(ribbit[0], ben));
                })
                .thenIdle(WINDOW)
                .thenExecute(() -> {
                    heard[0] = ben.heard.size();
                    day[0] = today(level);
                    click(ben, ribbit[0], true);
                })
                .thenWaitUntil(() -> check("the gardener speaks on the next day", ben.heard.size() > heard[0]))
                .thenWaitUntil(() -> check("the engine received a second TALKED", talked(engine, v.id, ben.getUUID()).size() >= 2))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> t = talked(engine, v.id, ben.getUUID());
                    check("exactly two TALKED over two days: " + t.size(), t.size() == 2);
                    long a = t.get(0).view().getLong(TalkRecords.Talked.OP_ID), b = t.get(1).view().getLong(TalkRecords.Talked.OP_ID);
                    check("each with its own op id", a != b);
                    check("the day is Ben's again, today's: " + ben.getData(RibbitTalker.DAYS),
                            ben.getData(RibbitTalker.DAYS).lastDay(RibbitTalker.key(v.id)) >= day[0]);
                    check("and the gardener's own record moved on from yesterday: " + ribbit[0].getData(RtsimAttachments.TALK_DAYS),
                            ribbit[0].getData(RtsimAttachments.TALK_DAYS).lastDay(ben.getUUID()) >= day[0]);
                    record("ribbit talk", "TALKED over two days", t.size());
                })
                .thenWaitUntil(() -> check("the next day's credit reached the village",
                        live == null || live.phase1Sentiment(v.id, ben.getUUID()) > base[0] + 3))
                .thenExecute(() -> {
                    if (live == null)
                        return;
                    int s = live.phase1Sentiment(v.id, ben.getUUID());
                    check("exactly +6 after two credits on two days: " + s + " from " + base[0], s == base[0] + 6);
                    record("ribbit talk", "sentiment after two days", s);
                })
                .thenSucceed();
    }

    // ---- no trade screen --------------------------------------------------------------------------

    /**
     * A crouching empty hand on a ribbit merchant opens no trades and speaks; walking up to it (not crouching)
     * still opens them, as Ribbits does. The merchant has no village here, so it speaks at once as a ribbit
     * with no place and credits nothing.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200, batch = BATCH)
    public static void ribbit_merchant_opens_no_trade_screen(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Scene scene = new Scene();
        scene.arm(helper);
        ScriptedEngine engine = RtsimTests.engine();
        FakePlayers.Hearer ben = scene.player(level, helper.absoluteVec(new Vec3(5.5, 2, 8.5)), "ribbit_talk_trader");
        RibbitEntity[] m = new RibbitEntity[1];

        helper.startSequence()
                .thenExecute(() -> {
                    m[0] = scene.entity(RibbitGameTests.spawn(helper, RtsimTests.centre(helper), RibbitProfessionModule.MERCHANT));
                    m[0].setNoAi(true);
                    check("the merchant has trades", !m[0].getOffers().isEmpty());
                    // The flag that lets the client cancel the click too: with it no screen opens and no second packet comes.
                    check("ribbits always talk", Talkers.alwaysTalks(Ribbits.type()));
                })
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    check("it lives in no village here", RtsimApi.villageOf(m[0]) == null);
                    int raw0 = ben.raw.size(), overlay0 = ben.overlay.size();
                    InteractionResult[] r = click(ben, m[0], true);
                    check("one message for the click, none on the action bar: " + (ben.raw.size() - raw0) + " " + (ben.overlay.size() - overlay0),
                            ben.raw.size() - raw0 == 1 && ben.overlay.size() == overlay0);
                    check("a crouching click on a merchant opens nothing: " + ben.containerMenu, ben.containerMenu == ben.inventoryMenu);
                    check("and the merchant is not trading", m[0].getTradingPlayer() == null);
                    check("main hand SUCCESS, off hand CONSUME: " + r[0] + " " + r[1],
                            r[0] == InteractionResult.SUCCESS && r[1] == InteractionResult.CONSUME);
                    check("the merchant was talked to: " + keys(ben), ben.heard.size() == 1);
                    Said said = Said.of(ben.heard.get(0));
                    check("in the merchant's voice: " + said.line().getKey(), said.line().getKey().startsWith("mic_rtsim.talk.ribbit."));
                    check("a ribbit with no place has no name: " + said.speakerContents().getKey(),
                            said.speakerContents().getKey().equals("mic_rtsim.talk.speaker.ribbit.unplaced"));
                    check("and says what it is: " + said.speaker().getString(), said.speaker().getString().contains("Merchant"));
                    record("ribbit merchant", "line", said.line().getKey() + " = " + said.line().getFallback());

                    // The control: not crouching, the merchant opens its trades.
                    click(ben, m[0], false);
                    check("without crouching the merchant opens its trades: " + ben.containerMenu, ben.containerMenu instanceof MerchantMenu);
                    check("and that is no talk: " + ben.heard.size(), ben.heard.size() == 1);
                    ben.closeContainer();
                    check("closed again", ben.containerMenu == ben.inventoryMenu);
                    check("closing the trade screen puts nothing on the action bar", ben.overlay.isEmpty());
                })
                .thenIdle(QUIET)
                .thenExecute(() -> check("no village, no credit: the engine heard no TALKED", talkedBy(engine, ben.getUUID()) == 0))
                .thenSucceed();
    }

    // ---- puppets ----------------------------------------------------------------------------------

    /**
     * A merchant that is a caravan's puppet speaks from its caravan's board and credits nothing: no TALKED, no
     * day on the player (a visitor is not its village, and a village sentiment is not the caravan's to give).
     * In a batch of its own: the test counts the process's asks.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400, batch = BATCH + "_puppet")
    public static void ribbit_puppet_talks_without_credit(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Scene scene = new Scene();
        scene.arm(helper);
        ScriptedEngine engine = RtsimTests.engine();
        // The caravan stands inside an adopted ribbit village: a resident there would credit it, so that a puppet
        // earns nothing is the talker's rule and not an empty arena.
        Village v = RtsimTests.adoptArena(helper, MicRtsimRibbits.CULTURE);
        scene.villages.add(v);
        long agent = RtsimTests.newAgent();
        Vec3 c = RtsimTests.centre(helper);
        engine.spawnGroup(agent, 1, RibbitMerchantAdapter.NAME, RtsimTests.dimension(helper), c.x, c.y, c.z, List.of(
                new ScriptedEngine.MemberSpec(0, 0f, 1, -1, List.of(new ScriptedEngine.ItemSpec(0, "minecraft:slime_ball", 20),
                        new ScriptedEngine.ItemSpec(1, "minecraft:amethyst_shard", 5)))));
        engine.merchantOffers(agent, 1, 0, List.of(
                new ScriptedEngine.OfferSpec("minecraft:amethyst_shard", 1, null, 0, "minecraft:slime_ball", 4, 0,
                        com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd.MerchantOffers.O_FLAG_SELLS)));
        scene.after.add(() -> engine.despawnGroup(agent, 1));
        FakePlayers.Hearer ben = scene.player(level, helper.absoluteVec(new Vec3(5.5, 2, 8.5)), "ribbit_talk_puppet");
        RibbitEntity[] m = new RibbitEntity[1];
        int[] heard = new int[1];
        long[] asks = new long[1];

        helper.startSequence()
                .thenWaitUntil(() -> check("the offers arrived", RtsimTests.embodied(agent).members[0].offers != null))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    m[0] = (RibbitEntity) RtsimTests.member(g, 0);
                    check("a ribbit merchant", "ribbits:merchant".equals(Ribbits.professionName(m[0])));
                    check("it is a puppet of the caravan", RtsimApi.group(m[0]) == g);
                    check("and it stands in an adopted ribbit village (a resident here would credit it)", RtsimApi.villageOf(m[0]) == v);
                    check("and the plugin's talker answers for it", Talkers.find(m[0], ben) instanceof RibbitTalker);
                    Talker talker = Talkers.find(m[0], ben);
                    TalkHolder holder = talker.holder(m[0]);
                    check("it speaks from its caravan's board: " + holder, holder != null && holder.kind() == TalkHolder.CARAVAN && holder.id() == agent);
                    check("a puppet has no home here, though it stands in a village: " + talker.home(m[0]), talker.home(m[0]) == null);
                    check("and earns no credit", !talker.credits(m[0], ben));
                })
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    heard[0] = ben.heard.size();
                    asks[0] = TalkAsks.INSTANCE.asked;
                    InteractionResult[] r = click(ben, m[0], true);
                    check("the main hand swings: " + r[0], r[0] == InteractionResult.SUCCESS);
                    check("a crouching click opens nothing", ben.containerMenu == ben.inventoryMenu);
                    check("the caravan was asked: " + (TalkAsks.INSTANCE.asked - asks[0]) + " ask(s)",
                            !(engine instanceof NativeEngine) || TalkAsks.INSTANCE.asked - asks[0] == 1);
                })
                .thenWaitUntil(() -> check("the puppet's line arrived", ben.heard.size() > heard[0]))
                .thenIdle(QUIET)
                .thenExecute(() -> {
                    check("exactly one line: " + keys(ben), ben.heard.size() == heard[0] + 1);
                    Said said = Said.of(ben.heard.get(heard[0]));
                    check("a ribbit line: " + said.line().getKey(), said.line().getKey().startsWith("mic_rtsim.talk.ribbit."));
                    check("from a ribbit with no place: " + said.speakerContents().getKey(),
                            said.speakerContents().getKey().equals("mic_rtsim.talk.speaker.ribbit.unplaced"));
                    check("the engine heard no TALKED: " + talkedBy(engine, ben.getUUID()), talkedBy(engine, ben.getUUID()) == 0);
                    check("no day on Ben", ben.getData(RibbitTalker.DAYS).isEmpty());
                    record("ribbit puppet", "line", said.line().getKey() + " = " + said.line().getFallback());
                    engine.despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the caravan is gone", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    // ---- what the talker decides ------------------------------------------------------------------

    private static TalkContext context(ServerLevel level, RibbitEntity r, ServerPlayer p, Talker t, String home) {
        return new TalkContext(r, p, level, level.getGameTime(), t.state(r, p), t.attitude(r, p), home, null);
    }

    private static List<String> topics(TalkCandidates out) {
        List<String> l = new ArrayList<>();
        for (TalkCandidates.Candidate c : out.list())
            l.add(c.topic());
        return l;
    }

    /**
     * What the talker decides, call by call, on a village with a scripted sentiment (a friend, a foe and a
     * stranger): the voice of each profession, a baby's plain one, busy while playing or casting, wary toward
     * a player the village dislikes, the mood candidates, the holder, and that a band's musician is a puppet
     * with no village and no credit even standing inside one.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400, batch = BATCH)
    public static void ribbit_talker_decides_voice_state_attitude_and_holder(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Scene scene = new Scene();
        scene.arm(helper);
        ScriptedEngine engine = RtsimTests.engine();
        Village v = RtsimTests.adoptArena(helper, MicRtsimRibbits.CULTURE);
        scene.villages.add(v);
        Vec3 c = RtsimTests.centre(helper);
        FakePlayers.Hearer friend = scene.player(level, helper.absoluteVec(new Vec3(3.5, 2, 8.5)), "ribbit_friend");
        FakePlayers.Hearer foe = scene.player(level, helper.absoluteVec(new Vec3(5.5, 2, 8.5)), "ribbit_foe");
        FakePlayers.Hearer stranger = scene.player(level, helper.absoluteVec(new Vec3(7.5, 2, 8.5)), "ribbit_stranger");
        engine.sentiment(v.id, -32, 32, true, Map.of(foe.getUUID(), -100, friend.getUUID(), 64));
        RibbitProfession[] professions = {RibbitProfessionModule.GARDENER, RibbitProfessionModule.FISHERMAN, RibbitProfessionModule.MERCHANT,
                RibbitProfessionModule.SORCERER, RibbitProfessionModule.NITWIT};
        String[] paths = {"gardener", "fisherman", "merchant", "sorcerer", "nitwit"};
        RibbitEntity[] residents = new RibbitEntity[professions.length];
        RibbitEntity[] baby = new RibbitEntity[1];
        long agent = RtsimTests.newAgent();
        scene.after.add(() -> engine.despawnGroup(agent, 1));

        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < professions.length; i++) {
                        residents[i] = scene.entity(RibbitGameTests.spawn(helper, c.add(-2 + i * 0.8, 0, -2), professions[i]));
                        residents[i].setNoAi(true);
                    }
                    baby[0] = scene.entity(RibbitGameTests.spawn(helper, c.add(0, 0, 2), RibbitProfessionModule.GARDENER));
                    baby[0].setNoAi(true);
                    baby[0].setAge(-24000);
                    engine.spawnGroup(agent, 1, RibbitBands.BAND, RtsimTests.dimension(helper), c.x + 3, c.y, c.z + 3, List.of(
                            ScriptedEngine.MemberSpec.plain(0), ScriptedEngine.MemberSpec.plain(1),
                            ScriptedEngine.MemberSpec.plain(2), ScriptedEngine.MemberSpec.plain(3)));
                })
                .thenWaitUntil(() -> check("the village's sentiment arrived", RtsimApi.sentiment(v.id) != null))
                .thenWaitUntil(() -> check("the band stands", RtsimTests.embodied(agent) != null))
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    Set<String> voices = new HashSet<>();
                    for (int i = 0; i < professions.length; i++) {
                        Talker t = Talkers.find(residents[i], stranger);
                        check("the plugin's talker answers for the " + paths[i], t instanceof RibbitTalker);
                        String voice = t.voice(residents[i], stranger);
                        check("the " + paths[i] + " speaks as ribbit." + paths[i] + ": " + voice, voice.equals("ribbit." + paths[i]));
                        voices.add(voice);
                        TalkHolder h = t.holder(residents[i]);
                        check("a resident speaks from its village's board: " + h, h != null && h.kind() == TalkHolder.VILLAGE && h.id() == v.id);
                        check("and calls the village home: " + t.home(residents[i]), Names.village(v.id).equals(t.home(residents[i])));
                        check("a calm resident chats and is not busy", t.state(residents[i], stranger) == Talker.State.NONE);
                    }
                    check("five voices: " + voices, voices.size() == 5);

                    // A baby: the plain ribbit, no board, no credit.
                    Talker bt = Talkers.find(baby[0], stranger);
                    check("a baby ribbit is a plain ribbit: " + bt.voice(baby[0], stranger), bt.voice(baby[0], stranger).equals("ribbit"));
                    check("with no board to ask", bt.holder(baby[0]) == null);
                    check("and no credit", !bt.credits(baby[0], stranger));

                    // The credit's conditions, on a calm gardener: yes; not for a fake player, a baby, or a busy one.
                    Talker gt = Talkers.find(residents[0], stranger);
                    check("a calm resident's talk would credit", gt.credits(residents[0], stranger));
                    check("asking whether it would credit writes nothing", stranger.getData(RibbitTalker.DAYS).isEmpty());
                    residents[4].setPlayingInstrument(true);
                    check("a nitwit playing in a band is busy: " + gt.state(residents[4], stranger),
                            Talkers.find(residents[4], stranger).state(residents[4], stranger) == Talker.State.BUSY);
                    check("and a busy ribbit credits nothing", !Talkers.find(residents[4], stranger).credits(residents[4], stranger));
                    residents[4].setPlayingInstrument(false);
                    residents[3].setBuffing(true);
                    check("a sorcerer casting is busy",
                            Talkers.find(residents[3], stranger).state(residents[3], stranger) == Talker.State.BUSY);
                    residents[3].setBuffing(false);
                    check("and calm again after", Talkers.find(residents[3], stranger).state(residents[3], stranger) == Talker.State.NONE);

                    // Attitude and mood.
                    check("wary of a player the village dislikes: " + gt.attitude(residents[0], foe), gt.attitude(residents[0], foe) == Talker.Attitude.WARY);
                    check("friendly to its friend", gt.attitude(residents[0], friend) == Talker.Attitude.FRIENDLY);
                    check("and to a stranger", gt.attitude(residents[0], stranger) == Talker.Attitude.FRIENDLY);
                    TalkCandidates liked = new TalkCandidates(), disliked = new TalkCandidates(), neutral = new TalkCandidates();
                    gt.candidates(context(level, residents[0], friend, gt, gt.home(residents[0])), liked);
                    gt.candidates(context(level, residents[0], foe, gt, gt.home(residents[0])), disliked);
                    gt.candidates(context(level, residents[0], stranger, gt, gt.home(residents[0])), neutral);
                    check("a friend hears the village is fond: " + topics(liked), topics(liked).equals(List.of("mood.liked")));
                    check("a foe hears it is not: " + topics(disliked), topics(disliked).equals(List.of("mood.disliked")));
                    check("a stranger hears neither: " + topics(neutral), topics(neutral).isEmpty());
                    record("ribbit talker", "voices", voices);
                })
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    check("a band's musician is a puppet", g != null);
                    RibbitEntity musician = (RibbitEntity) RtsimTests.member(g, 1);
                    check("a nitwit", "ribbits:nitwit".equals(Ribbits.professionName(musician)));
                    Talker mt = Talkers.find(musician, stranger);
                    check("the plugin's talker answers for it", mt instanceof RibbitTalker);
                    check("a musician on the road speaks as one: " + mt.voice(musician, stranger), mt.voice(musician, stranger).equals("ribbit.musician"));
                    check("the engine keeps no board for a band: " + mt.holder(musician), mt.holder(musician) == null);
                    check("and it is no one's resident: " + mt.home(musician), mt.home(musician) == null);
                    check("it credits no village", !mt.credits(musician, stranger));
                    check("it is wary of no one's village's foe", mt.attitude(musician, foe) == Talker.Attitude.FRIENDLY);
                    engine.despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the band left", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    /**
     * A ribbit standing in a village of another culture has no village: the lookup falls back to position, but a
     * human village's sentiment is not a ribbit's to give or to speak of (TK70). It talks as the plain voice of
     * its profession, from no board, and credits nothing.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200, batch = BATCH)
    public static void ribbit_in_another_cultures_village_has_no_village(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Scene scene = new Scene();
        scene.arm(helper);
        Village other = RtsimTests.adoptArena(helper, "human");
        scene.villages.add(other);
        FakePlayers.Hearer ben = scene.player(level, helper.absoluteVec(new Vec3(5.5, 2, 8.5)), "ribbit_other_culture");
        RibbitEntity[] r = new RibbitEntity[1];

        helper.startSequence()
                .thenExecute(() -> {
                    r[0] = scene.entity(RibbitGameTests.spawn(helper, RtsimTests.centre(helper), RibbitProfessionModule.GARDENER));
                    r[0].setNoAi(true);
                })
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    check("the engine's lookup finds the human village by position: " + RtsimApi.villageOf(r[0]),
                            RtsimApi.villageOf(r[0]) == other);
                    Talker t = Talkers.find(r[0], ben);
                    check("the plugin's talker answers", t instanceof RibbitTalker);
                    check("it is no village's board: " + t.holder(r[0]), t.holder(r[0]) == null);
                    check("and no village's home: " + t.home(r[0]), t.home(r[0]) == null);
                    check("it credits nothing", !t.credits(r[0], ben));
                    check("it still speaks as a gardener: " + t.voice(r[0], ben), t.voice(r[0], ben).equals("ribbit.gardener"));
                    TalkCandidates out = new TalkCandidates();
                    t.candidates(context(level, r[0], ben, t, null), out);
                    check("and has no village mood to share: " + topics(out), topics(out).isEmpty());
                    check("asking wrote nothing", ben.getData(RibbitTalker.DAYS).isEmpty());
                })
                .thenSucceed();
    }

    /**
     * The player's record of credited villages (TK69): a village already in it is replaced, and when it is over
     * its cap only entries from before today are dropped, the oldest first, so today's talks are never lost to a
     * tie the core's rule would break by dropping one of them.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 40, batch = BATCH)
    public static void ribbit_credit_record_drops_only_older_days(GameTestHelper helper) {
        helper.startSequence()
                .thenExecute(() -> {
                    long today = 100;
                    TalkDays days = TalkDays.EMPTY;
                    for (long v = 1; v <= RibbitTalker.KEEP + 4; v++)
                        days = RibbitTalker.credited(days, RibbitTalker.key(v), today);
                    check("every village talked to today is kept, past the cap: " + days.entries().size(),
                            days.entries().size() == RibbitTalker.KEEP + 4);
                    for (long v = 1; v <= RibbitTalker.KEEP + 4; v++)
                        check("village " + v + " is kept for today", days.creditedOn(RibbitTalker.key(v), today));

                    // Older entries go first, oldest first, and only down to the cap.
                    days = TalkDays.EMPTY;
                    for (long v = 1; v <= RibbitTalker.KEEP; v++)
                        days = RibbitTalker.credited(days, RibbitTalker.key(v), 90 + (v % 5));
                    check("full at the cap: " + days.entries().size(), days.entries().size() == RibbitTalker.KEEP);
                    days = RibbitTalker.credited(days, RibbitTalker.key(1000), today);
                    check("one more keeps the cap: " + days.entries().size(), days.entries().size() == RibbitTalker.KEEP);
                    check("and it is today's", days.creditedOn(RibbitTalker.key(1000), today));
                    long oldest = Long.MAX_VALUE;
                    for (TalkDays.Entry e : days.entries())
                        oldest = Math.min(oldest, e.day());
                    check("the dropped one was an oldest (day 90 is gone, none is older): " + oldest, oldest >= 90);
                    int day90 = 0;
                    for (TalkDays.Entry e : days.entries())
                        if (e.day() == 90)
                            day90++;
                    check("one of the day-90 entries went: " + day90, day90 == 2); // v % 5 == 0 for v = 5, 10, 15 (three of 16)

                    // The village already there is replaced, not added.
                    TalkDays again = RibbitTalker.credited(days, RibbitTalker.key(1000), today + 1);
                    check("a village already there is replaced: " + again.entries().size(), again.entries().size() == days.entries().size());
                    check("with the newer day", again.lastDay(RibbitTalker.key(1000)) == today + 1);
                })
                .thenSucceed();
    }

    // ---- lines ------------------------------------------------------------------------------------

    /** The dedicated server has the plugin's en_us: its lines resolve through the core's fallback chain and count. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 40, batch = BATCH)
    public static void ribbit_lines_load_on_the_server(GameTestHelper helper) {
        helper.startSequence()
                .thenExecute(() -> {
                    check("the speaker template is on the server", Talk.english("mic_rtsim.talk.speaker.ribbit").equals("%1$s %2$s"));
                    check("and the unplaced one", Talk.english("mic_rtsim.talk.speaker.ribbit.unplaced").contains("%1$s"));
                    for (String p : new String[]{"gardener", "fisherman", "merchant", "sorcerer", "nitwit", "musician"}) {
                        String key = "mic_rtsim.talk.ribbit.profession." + p;
                        check("the " + p + " has a name: " + Talk.english(key), !Talk.english(key).equals(key));
                    }
                    int fluff = Lines.count("mic_rtsim.talk.ribbit.fluff");
                    check("at least eight ribbit fluff lines: " + fluff, fluff >= 8);
                    java.util.List<Lines.Found> gardener = Lines.levels("ribbit.gardener", "fluff");
                    check("a gardener draws from her own fluff and the ribbits': " + gardener,
                            gardener.size() == 2 && gardener.get(0).prefix().equals("mic_rtsim.talk.ribbit.gardener.fluff")
                                    && gardener.get(1).prefix().equals("mic_rtsim.talk.ribbit.fluff"));
                    java.util.List<Lines.Found> musician = Lines.levels("ribbit.musician", "fluff");
                    check("a musician draws from its own fluff and the ribbits': " + musician,
                            musician.size() == 2 && musician.get(0).prefix().equals("mic_rtsim.talk.ribbit.musician.fluff")
                                    && musician.get(1).prefix().equals("mic_rtsim.talk.ribbit.fluff"));
                    java.util.List<Lines.Found> liked = Lines.levels("ribbit.sorcerer", "mood.liked");
                    check("a sorcerer has her own fond lines and the ribbits': " + liked, liked.size() == 2);
                    check("a ribbit has disliking lines: " + Lines.find("ribbit.merchant", "mood.disliked"),
                            Lines.find("ribbit.merchant", "mood.disliked") != null);
                    Lines.Found asleep = Lines.find("ribbit.fisherman", "state.asleep");
                    check("a sleeping ribbit has the ribbits' lines and not any's: " + asleep,
                            asleep != null && asleep.prefix().equals("mic_rtsim.talk.ribbit.state.asleep"));
                    check("a busy one too", Lines.find("ribbit.nitwit", "state.busy") != null);
                    check("and a hurt one", Lines.find("ribbit.nitwit", "state.hurt") != null);
                    for (String topic : new String[]{"news.assault.us", "news.robbery.us", "news.trade.us", "news.trade.you", "news.rescue.us", "news.rescue.you"})
                        check("a village's news in a ribbit's words: " + topic + " " + Lines.find("ribbit.gardener", topic),
                                Lines.find("ribbit.gardener", topic) != null
                                        && Lines.find("ribbit.gardener", topic).prefix().equals("mic_rtsim.talk.ribbit." + topic));
                    // The .us lines say what the doer (%1) did to the village; the .you lines what the player did to a whom (%2).
                    check("trade.us names the trader: " + Talk.english("mic_rtsim.talk.ribbit.news.trade.us.0"),
                            Talk.english("mic_rtsim.talk.ribbit.news.trade.us.0").startsWith("%1$s"));
                    check("trade.you names whom: " + Talk.english("mic_rtsim.talk.ribbit.news.trade.you.0"),
                            Talk.english("mic_rtsim.talk.ribbit.news.trade.you.0").contains("%2$s"));
                    check("rescue.you names whom: " + Talk.english("mic_rtsim.talk.ribbit.news.rescue.you.0"),
                            Talk.english("mic_rtsim.talk.ribbit.news.rescue.you.0").contains("%2$s"));
                    // The name lists: the ribbit's format and its two halves.
                    Set<String> names = new HashSet<>();
                    for (long k = 1; k <= 24; k++) {
                        String name = Names.camp("ribbit", k * 7919L);
                        check("a ribbit name for key " + k, name != null && !name.isBlank());
                        check("the same twice", name.equals(Names.camp("ribbit", k * 7919L)));
                        names.add(name);
                    }
                    check("names vary: " + names.size() + " in 24", names.size() >= 12);
                    record("ribbit lines", "ribbit.fluff variants", fluff);
                    record("ribbit lines", "names", names);
                })
                .thenSucceed();
    }
}
