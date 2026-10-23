package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.PlayerListAccessor;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Lines;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Names;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talk;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkDays;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestInfo;
import net.minecraft.gametest.framework.GameTestListener;
import net.minecraft.gametest.framework.GameTestRunner;
import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talkers;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.ai.gossip.GossipType;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.entity.npc.VillagerProfession;
import net.minecraft.world.entity.npc.WanderingTrader;
import net.minecraft.world.inventory.MerchantMenu;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.common.util.FakePlayer;
import net.neoforged.neoforge.common.util.FakePlayerFactory;
import net.neoforged.neoforge.event.entity.player.PlayerInteractEvent;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * TALK slice 1 (TALK.md §12): the gesture, who hears the line, that no screen opens, the daily gossip
 * cap, the empty hand, and that the dedicated server has the core's lines. Clicks are simulated as the
 * server's packet handler makes them: the shift state, then {@code interactOn} for each hand.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class TalkGameTests {
    private TalkGameTests() {}

    /** The tick the talker's brain needs before talking (vanilla's first maybeDecayGossip sets its clock). */
    private static final int SETTLE = 3;
    /** More than the pair window of the rate limit (TK3). */
    private static final int WINDOW = 45;

    private static Villager farmer(GameTestHelper helper, double x, double z) {
        Villager v = helper.spawn(EntityType.VILLAGER, new Vec3(x, 2, z));
        v.setVillagerData(v.getVillagerData().setProfession(VillagerProfession.FARMER).setLevel(1));
        v.setPersistenceRequired();
        // Some xp, or vanilla's ResetProfession strips the job on the first brain tick.
        v.setVillagerXp(1);
        check("the farmer has trades", !v.getOffers().isEmpty());
        return v;
    }

    /** Takes the hearing players out of the level when the test ends, however it ends. */
    private static void cleanup(GameTestHelper helper, FakePlayers.Hearer... players) {
        helper.testInfo.addListener(new GameTestListener() {
            private void remove() {
                for (FakePlayers.Hearer p : players)
                    FakePlayers.remove(p);
            }

            @Override
            public void testStructureLoaded(GameTestInfo info) {}

            @Override
            public void testPassed(GameTestInfo info, GameTestRunner runner) {
                remove();
            }

            @Override
            public void testFailed(GameTestInfo info, GameTestRunner runner) {
                remove();
            }

            @Override
            public void testAddedForRerun(GameTestInfo oldTest, GameTestInfo newTest, GameTestRunner runner) {}
        });
    }

    private static InteractionResult[] click(net.minecraft.server.level.ServerPlayer player, net.minecraft.world.entity.Entity target, boolean crouch) {
        player.setShiftKeyDown(crouch);
        InteractionResult main = player.interactOn(target, InteractionHand.MAIN_HAND);
        InteractionResult off = player.interactOn(target, InteractionHand.OFF_HAND);
        return new InteractionResult[]{main, off};
    }

    /** Every message a hearer got, by key, for failure messages. */
    private static String keys(FakePlayers.Hearer p) {
        StringBuilder sb = new StringBuilder("[");
        for (Component c : p.raw)
            sb.append(c.getContents() instanceof TranslatableContents t ? t.getKey() : c.getString()).append("; ");
        return sb.append("]").toString();
    }

    private static TranslatableContents translatable(Component c) {
        check("a translatable component: " + c, c.getContents() instanceof TranslatableContents);
        return (TranslatableContents) c.getContents();
    }

    private static int minorPositive(Villager v, UUID player) {
        return v.getGossips().getReputation(player, t -> t == GossipType.MINOR_POSITIVE);
    }

    /** A crouching click on a villager: only the player who clicked reads a line, and it is one of ours. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void talkLineReachesOnlyTheTalker(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(7.5, 2, 5.5)), "talk_ben");
        FakePlayers.Hearer alex = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(3.5, 2, 5.5)), "talk_alex");
        cleanup(helper, ben, alex);
        Villager[] v = new Villager[1];
        helper.startSequence()
                .thenExecute(() -> v[0] = farmer(helper, 5.5, 5.5))
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    // Both players are on the server's player list for the click only, so a line sent to
                    // everyone (a broadcast) would reach Alex too. Add, click, remove: one synchronous block.
                    java.util.List<net.minecraft.server.level.ServerPlayer> all = ((PlayerListAccessor) level.getServer().getPlayerList()).mic_rtsim$players();
                    InteractionResult[] r;
                    int benRaw = ben.raw.size(), benOverlay = ben.overlay.size();
                    int alexRaw = alex.raw.size(), alexOverlay = alex.overlay.size();
                    all.add(ben);
                    all.add(alex);
                    try {
                        r = click(ben, v[0], true);
                    } finally {
                        all.remove(ben);
                        all.remove(alex);
                    }
                    check("the main hand swings once: " + r[0], r[0] == InteractionResult.SUCCESS);
                    check("the off hand is consumed silently: " + r[1], r[1] == InteractionResult.CONSUME);
                    check("Ben received exactly one talk line: " + ben.heard.size() + " of " + keys(ben), ben.heard.size() == 1);
                    TranslatableContents say = translatable(ben.heard.get(0));
                    check("it is the say template: " + say.getKey(), say.getKey().equals("mic_rtsim.talk.say"));
                    check("with a speaker and a line", say.getArgs().length == 2 && say.getArgs()[1] instanceof Component);
                    TranslatableContents line = translatable((Component) say.getArgs()[1]);
                    check("the line is one of ours: " + line.getKey(), line.getKey().startsWith("mic_rtsim.talk."));
                    check("it carries the server's English as the fallback: " + line.getFallback(),
                            line.getFallback() != null && !line.getFallback().equals(line.getKey()));
                    check("the say template carries its English too", say.getFallback() != null && say.getFallback().contains("%2$s"));
                    check("Alex received no talk line: " + keys(alex), alex.heard.isEmpty());
                    // Same tick as the click, so nothing else can have been sent in between: exact counts.
                    check("Ben got exactly one message of any kind: " + (ben.raw.size() - benRaw) + " of " + keys(ben), ben.raw.size() - benRaw == 1);
                    check("and none on the action bar: " + (ben.overlay.size() - benOverlay), ben.overlay.size() == benOverlay);
                    check("Alex got no message of any kind: " + (alex.raw.size() - alexRaw) + " of " + keys(alex), alex.raw.size() == alexRaw);
                    check("and none on the action bar: " + (alex.overlay.size() - alexOverlay), alex.overlay.size() == alexOverlay);
                    record("talk", "line", line.getKey() + " = " + line.getFallback());
                    record("talk", "speaker", ((Component) say.getArgs()[0]).getString());
                })
                .thenSucceed();
    }

    /** A crouching click opens no trades, on a villager or a wandering trader; walking up to the trader still does. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void talkOpensNoTradeScreen(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(5.5, 2, 7.5)), "talk_tradeless");
        cleanup(helper, ben);
        Villager[] v = new Villager[1];
        WanderingTrader[] t = new WanderingTrader[1];
        helper.startSequence()
                .thenExecute(() -> {
                    v[0] = farmer(helper, 4.5, 5.5);
                    t[0] = helper.spawn(EntityType.WANDERING_TRADER, new Vec3(6.5, 2, 5.5));
                    t[0].setPersistenceRequired();
                    check("the trader has trades", !t[0].getOffers().isEmpty());
                })
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    int raw0 = ben.raw.size(), overlay0 = ben.overlay.size();
                    click(ben, v[0], true);
                    check("one message for the click, none on the action bar: " + (ben.raw.size() - raw0) + " " + (ben.overlay.size() - overlay0),
                            ben.raw.size() - raw0 == 1 && ben.overlay.size() == overlay0);
                    check("a crouching click on a villager with trades opens nothing", ben.containerMenu == ben.inventoryMenu);
                    check("and the villager is not trading", v[0].getTradingPlayer() == null);
                    check("the villager was talked to", ben.heard.size() == 1);
                    InteractionResult[] r = click(ben, t[0], true);
                    check("a crouching click on the trader opens nothing", ben.containerMenu == ben.inventoryMenu);
                    check("and the trader is not trading", t[0].getTradingPlayer() == null);
                    check("the trader was talked to: " + ben.heard.size(), ben.heard.size() == 2);
                    check("main hand SUCCESS, off hand CONSUME: " + r[0] + " " + r[1],
                            r[0] == InteractionResult.SUCCESS && r[1] == InteractionResult.CONSUME);
                    TranslatableContents say = translatable(ben.heard.get(1));
                    TranslatableContents line = translatable((Component) say.getArgs()[1]);
                    check("a trader line: " + line.getKey(), line.getKey().startsWith("mic_rtsim.talk.trader.")
                            || line.getKey().startsWith("mic_rtsim.talk.any."));

                    // The controls: not crouching, the villager and the trader open their trades as vanilla does.
                    click(ben, v[0], false);
                    check("without crouching the villager opens its trades: " + ben.containerMenu,
                            ben.containerMenu instanceof MerchantMenu);
                    ben.closeContainer();
                    check("closed again", ben.containerMenu == ben.inventoryMenu);
                    click(ben, t[0], false);
                    check("without crouching the trader opens its trades: " + ben.containerMenu,
                            ben.containerMenu instanceof MerchantMenu);
                    ben.closeContainer();
                    check("closed again", ben.containerMenu == ben.inventoryMenu);
                    check("closing the trade screen puts nothing on the action bar", ben.overlay.isEmpty());
                })
                .thenSucceed();
    }

    /** The first talk of a game day adds MINOR_POSITIVE 3; a second the same day adds nothing; the next day adds 3 again. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400)
    public static void talkGossipCappedPerDay(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(7.5, 2, 5.5)), "talk_daily");
        cleanup(helper, ben);
        Villager[] v = new Villager[1];
        helper.startSequence()
                .thenExecute(() -> v[0] = farmer(helper, 5.5, 5.5))
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    check("no gossip about Ben yet", minorPositive(v[0], ben.getUUID()) == 0);
                    click(ben, v[0], true);
                    check("a talk adds minor positive 3: " + minorPositive(v[0], ben.getUUID()), minorPositive(v[0], ben.getUUID()) == 3);
                    check("and records the day", v[0].getData(RtsimAttachments.TALK_DAYS).creditedOn(ben.getUUID(), level.getGameTime() / Talk.DAY));
                })
                .thenIdle(WINDOW)
                .thenExecute(() -> {
                    int before = ben.heard.size();
                    // Today as of this very click, so a day boundary between the clicks cannot credit again.
                    v[0].setData(RtsimAttachments.TALK_DAYS, TalkDays.EMPTY.with(ben.getUUID(), level.getGameTime() / Talk.DAY));
                    click(ben, v[0], true);
                    check("the second talk is heard: " + ben.heard.size(), ben.heard.size() == before + 1);
                    check("but adds nothing the same day: " + minorPositive(v[0], ben.getUUID()), minorPositive(v[0], ben.getUUID()) == 3);
                    // A new day, emulated by rewriting the villager's record: other suites share the game time.
                    long day = level.getGameTime() / Talk.DAY;
                    v[0].setData(RtsimAttachments.TALK_DAYS, TalkDays.EMPTY.with(ben.getUUID(), day - 1));
                })
                .thenIdle(WINDOW)
                .thenExecute(() -> {
                    click(ben, v[0], true);
                    check("the next day's talk adds 3 again: " + minorPositive(v[0], ben.getUUID()), minorPositive(v[0], ben.getUUID()) == 6);
                    record("talk", "gossip after three talks over two days", minorPositive(v[0], ben.getUUID()));
                })
                .thenSucceed();
    }

    /** Something in the main hand keeps its own use: no talk, no gossip, and the trader trades as vanilla does. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void talkNeedsAnEmptyHand(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(5.5, 2, 7.5)), "talk_stick");
        cleanup(helper, ben);
        Villager[] v = new Villager[1];
        WanderingTrader[] t = new WanderingTrader[1];
        helper.startSequence()
                .thenExecute(() -> {
                    v[0] = farmer(helper, 4.5, 5.5);
                    t[0] = helper.spawn(EntityType.WANDERING_TRADER, new Vec3(6.5, 2, 5.5));
                    t[0].setPersistenceRequired();
                    check("the trader has trades", !t[0].getOffers().isEmpty());
                    ben.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.STICK));
                })
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    int raw0 = ben.raw.size();
                    click(ben, v[0], true);
                    check("a stick in hand: the villager says nothing: " + ben.heard.size(), ben.heard.isEmpty());
                    check("not a message of any kind: " + (ben.raw.size() - raw0), ben.raw.size() == raw0);
                    check("and gains no gossip", minorPositive(v[0], ben.getUUID()) == 0);
                    click(ben, t[0], true);
                    check("a stick in hand: a crouching click on the trader opens its trades as vanilla does: " + ben.containerMenu,
                            ben.containerMenu instanceof MerchantMenu);
                    check("and nothing was said: " + ben.heard.size(), ben.heard.isEmpty());
                    ben.closeContainer();
                    check("closing it puts nothing on the action bar: " + ben.overlay.size(), ben.overlay.isEmpty());
                    ben.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                })
                .thenSucceed();
    }

    /** The dedicated server has the core's en_us: lines resolve through the fallback chain and count. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 40)
    public static void talkLinesLoadOnTheServer(GameTestHelper helper) {
        helper.startSequence()
                .thenExecute(() -> {
                    check("the say template is on the server", Talk.english("mic_rtsim.talk.say").contains("%2$s"));
                    Lines.Found rain = Lines.find("villager.farmer", "fluff.rain");
                    check("farmer + fluff.rain falls back to the villager's rain lines: " + rain,
                            rain != null && rain.prefix().equals("mic_rtsim.talk.villager.fluff.rain") && rain.count() >= 1);
                    java.util.List<Lines.Found> levels = Lines.levels("villager.farmer", "fluff");
                    check("a farmer draws from both her own fluff and the villagers': " + levels,
                            levels.size() == 2 && levels.get(0).prefix().equals("mic_rtsim.talk.villager.farmer.fluff")
                                    && levels.get(1).prefix().equals("mic_rtsim.talk.villager.fluff"));
                    java.util.List<Lines.Found> home = Lines.levels("villager.cartographer", "fluff.home");
                    check("a cartographer draws from both home lines: " + home, home.size() == 2);
                    java.util.List<Lines.Found> asleepChain = Lines.levels("villager.farmer", "state.asleep");
                    check("a sleeping farmer has the villagers' lines and not any's: " + asleepChain,
                            asleepChain.size() == 1 && asleepChain.get(0).prefix().equals("mic_rtsim.talk.villager.state.asleep"));
                    int fluff = Lines.count("mic_rtsim.talk.villager.fluff");
                    check("at least ten villager fluff lines: " + fluff, fluff >= 10);
                    Lines.Found asleep = Lines.find("child", "state.asleep");
                    check("a child asleep falls back to any: " + asleep, asleep != null && asleep.prefix().equals("mic_rtsim.talk.any.state.asleep"));
                    check("no such topic: nothing", Lines.find("villager.farmer", "no.such.topic") == null);
                    String name = Names.village(42L);
                    check("a village name: " + name, !name.isEmpty() && !name.equals("Village"));
                    check("the same twice", name.equals(Names.village(42L)));
                    check("another id, usually another name", !Names.village(43L).isEmpty());
                    record("talk", "villager.fluff variants", fluff);
                    record("talk", "Names.village(42)", name);
                })
                .thenSucceed();
    }

    /** The probe talkers register once per JVM; they answer only for entities with their own custom name. */
    private static final String PROBE = "talk_probe";
    private static boolean probesRegistered;

    private static final class Probe implements Talker {
        @Override
        public boolean talks(Entity entity, ServerPlayer player) {
            return entity.hasCustomName() && entity.getCustomName().getString().equals(PROBE);
        }

        @Override
        public String voice(Entity entity, ServerPlayer player) {
            return "villager";
        }

        @Override
        public Component speaker(Entity entity, ServerPlayer player) {
            return Component.literal("Probe");
        }
    }

    private static synchronized void registerProbes() {
        if (probesRegistered)
            return;
        probesRegistered = true;
        // On the villager (the built-in talker's own type) and on a type the core does not know, not always talking.
        Talkers.register(ResourceLocation.withDefaultNamespace("villager"), new Probe(), true);
        Talkers.register(ResourceLocation.withDefaultNamespace("cow"), new Probe(), false);
    }

    /** A machine (a NeoForge fake player, as a Create deployer is) crouch-clicking a villager is not a talk. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 100)
    public static void talkIgnoresFakePlayers(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Villager[] v = new Villager[1];
        helper.startSequence()
                .thenExecute(() -> v[0] = farmer(helper, 5.5, 5.5))
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    FakePlayer machine = FakePlayerFactory.getMinecraft(level);
                    check("it is a fake player", machine.isFakePlayer());
                    machine.setShiftKeyDown(true);
                    machine.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                    for (InteractionHand hand : InteractionHand.values()) {
                        PlayerInteractEvent.EntityInteract event = new PlayerInteractEvent.EntityInteract(machine, hand, v[0]);
                        NeoForge.EVENT_BUS.post(event);
                        check("the click is not cancelled by a talk (" + hand + ")", !event.isCanceled());
                    }
                    machine.setShiftKeyDown(false);
                    check("no gossip was credited", minorPositive(v[0], machine.getUUID()) == 0);
                    check("and no day recorded", v[0].getData(RtsimAttachments.TALK_DAYS).isEmpty());
                })
                .thenSucceed();
    }

    /** A plugin's talker is asked before the core's built-in one on the same type, and only for what it means. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 100)
    public static void talkPluginTalkerIsAskedFirst(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        registerProbes();
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(7.5, 2, 5.5)), "talk_plugin");
        cleanup(helper, ben);
        Villager[] v = new Villager[2];
        helper.startSequence()
                .thenExecute(() -> {
                    v[0] = farmer(helper, 4.5, 5.5);
                    v[1] = farmer(helper, 5.5, 5.5);
                    v[1].setCustomName(Component.literal(PROBE));
                })
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    Talker plain = Talkers.find(v[0], ben);
                    Talker named = Talkers.find(v[1], ben);
                    check("an unnamed farmer is the built-in's: " + plain, plain != null && plain.getClass().getSimpleName().equals("VillagerTalker"));
                    check("the named one is the plugin's, though the built-in would talk too: " + named, named instanceof Probe);
                    click(ben, v[1], true);
                    check("one line: " + keys(ben), ben.heard.size() == 1);
                    TranslatableContents say = translatable(ben.heard.get(0));
                    check("spoken by the plugin's speaker: " + say.getArgs()[0], ((Component) say.getArgs()[0]).getString().equals("Probe"));
                })
                .thenSucceed();
    }

    /**
     * A type that does not always talk: the client sends the off-hand item's use after the two interact
     * packets, and that use is cancelled in the talk's own tick (so a snowball is not thrown), while the
     * same use a while later works.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void talkCancelsTheOffHandItemUse(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        registerProbes();
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(7.5, 2, 5.5)), "talk_offhand");
        cleanup(helper, ben);
        net.minecraft.world.entity.animal.Cow[] cow = new net.minecraft.world.entity.animal.Cow[1];
        helper.startSequence()
                .thenExecute(() -> {
                    cow[0] = helper.spawn(EntityType.COW, new Vec3(5.5, 2, 5.5));
                    cow[0].setCustomName(Component.literal(PROBE));
                    cow[0].setPersistenceRequired();
                    ben.setItemInHand(InteractionHand.OFF_HAND, new ItemStack(Items.SNOWBALL, 4));
                    check("the cow's type does not always talk", !Talkers.alwaysTalks(EntityType.COW));
                })
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    long snowballs = level.getEntitiesOfClass(net.minecraft.world.entity.projectile.Snowball.class, ben.getBoundingBox().inflate(16)).size();
                    InteractionResult[] r = click(ben, cow[0], true);
                    check("the cow talked: " + keys(ben), r[0] == InteractionResult.SUCCESS && ben.heard.size() == 1);
                    // The client's separate use of the off-hand item, in the same tick.
                    InteractionResult use = ben.gameMode.useItem(ben, level, ben.getOffhandItem(), InteractionHand.OFF_HAND);
                    check("the use is cancelled: " + use, use == InteractionResult.CONSUME);
                    check("the snowballs are all still there: " + ben.getOffhandItem().getCount(), ben.getOffhandItem().getCount() == 4);
                    check("none was thrown", level.getEntitiesOfClass(net.minecraft.world.entity.projectile.Snowball.class, ben.getBoundingBox().inflate(16)).size() == snowballs);
                })
                .thenIdle(3)
                .thenExecute(() -> {
                    ben.setShiftKeyDown(false);
                    ben.gameMode.useItem(ben, level, ben.getOffhandItem(), InteractionHand.OFF_HAND);
                    check("a use a few ticks later is not touched: " + ben.getOffhandItem().getCount(), ben.getOffhandItem().getCount() == 3);
                })
                .thenSucceed();
    }
}
