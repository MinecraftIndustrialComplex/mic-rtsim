package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Lines;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Names;
import com.minecraftindustrialcomplex.mic_rtsim.talk.Talk;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkDays;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.ai.gossip.GossipType;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.entity.npc.VillagerProfession;
import net.minecraft.world.entity.npc.WanderingTrader;
import net.minecraft.world.inventory.MerchantMenu;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.phys.Vec3;
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
        check("the farmer has trades", !v.getOffers().isEmpty());
        return v;
    }

    private static InteractionResult[] click(net.minecraft.server.level.ServerPlayer player, net.minecraft.world.entity.Entity target, boolean crouch) {
        player.setShiftKeyDown(crouch);
        InteractionResult main = player.interactOn(target, InteractionHand.MAIN_HAND);
        InteractionResult off = player.interactOn(target, InteractionHand.OFF_HAND);
        return new InteractionResult[]{main, off};
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
        Villager[] v = new Villager[1];
        helper.startSequence()
                .thenExecute(() -> v[0] = farmer(helper, 5.5, 5.5))
                .thenIdle(SETTLE)
                .thenExecute(() -> {
                    check("Ben has not been hinted yet", !ben.getData(RtsimAttachments.TALK_HINTED));
                    InteractionResult[] r = click(ben, v[0], true);
                    check("the main hand swings once: " + r[0], r[0] == InteractionResult.SUCCESS);
                    check("the off hand is consumed silently: " + r[1], r[1] == InteractionResult.CONSUME);
                    check("Ben received exactly one message: " + ben.heard.size(), ben.heard.size() == 1);
                    TranslatableContents say = translatable(ben.heard.get(0));
                    check("it is the say template: " + say.getKey(), say.getKey().equals("mic_rtsim.talk.say"));
                    check("with a speaker and a line", say.getArgs().length == 2 && say.getArgs()[1] instanceof Component);
                    TranslatableContents line = translatable((Component) say.getArgs()[1]);
                    check("the line is one of ours: " + line.getKey(), line.getKey().startsWith("mic_rtsim.talk."));
                    check("it carries the server's English as the fallback: " + line.getFallback(),
                            line.getFallback() != null && !line.getFallback().equals(line.getKey()));
                    check("the say template carries its English too", say.getFallback() != null && say.getFallback().contains("%2$s"));
                    check("Alex received nothing: " + alex.heard.size(), alex.heard.isEmpty());
                    check("Ben is hinted by talking", ben.getData(RtsimAttachments.TALK_HINTED));
                    check("Alex is not", !alex.getData(RtsimAttachments.TALK_HINTED));
                    record("talk", "line", line.getKey() + " = " + line.getFallback());
                    record("talk", "speaker", ((Component) say.getArgs()[0]).getString());
                })
                .thenExecute(() -> {
                    FakePlayers.remove(ben);
                    FakePlayers.remove(alex);
                })
                .thenSucceed();
    }

    /** A crouching click opens no trades, on a villager or a wandering trader; walking up to the trader still does. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void talkOpensNoTradeScreen(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(5.5, 2, 7.5)), "talk_tradeless");
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
                    click(ben, v[0], true);
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

                    // The control: not crouching, the trader opens its trades as vanilla does.
                    click(ben, t[0], false);
                    check("without crouching the trader opens its trades: " + ben.containerMenu,
                            ben.containerMenu instanceof MerchantMenu);
                    ben.closeContainer();
                    check("closed again", ben.containerMenu == ben.inventoryMenu);
                    check("the first closed trade screen hinted nobody twice: ben already hinted by talking",
                            ben.overlay.isEmpty());
                    FakePlayers.remove(ben);
                })
                .thenSucceed();
    }

    /** The first talk of a game day adds MINOR_POSITIVE 3; a second the same day adds nothing; the next day adds 3 again. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400)
    public static void talkGossipCappedPerDay(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(7.5, 2, 5.5)), "talk_daily");
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
                    FakePlayers.remove(ben);
                })
                .thenSucceed();
    }

    /** Something in the main hand keeps its own use: no talk, no gossip, and the trader trades as vanilla does. */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void talkNeedsAnEmptyHand(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        FakePlayers.Hearer ben = FakePlayers.hearing(level, helper.absoluteVec(new Vec3(5.5, 2, 7.5)), "talk_stick");
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
                    click(ben, v[0], true);
                    check("a stick in hand: the villager says nothing: " + ben.heard.size(), ben.heard.isEmpty());
                    check("and gains no gossip", minorPositive(v[0], ben.getUUID()) == 0);
                    check("and is not hinted", !ben.getData(RtsimAttachments.TALK_HINTED));
                    click(ben, t[0], true);
                    check("a stick in hand: a crouching click on the trader opens its trades as vanilla does: " + ben.containerMenu,
                            ben.containerMenu instanceof MerchantMenu);
                    check("and nothing was said: " + ben.heard.size(), ben.heard.isEmpty());
                    ben.closeContainer();
                    check("the first closed trade screen shows the hint on the action bar: " + ben.overlay.size(), ben.overlay.size() == 1);
                    check("and marks him hinted", ben.getData(RtsimAttachments.TALK_HINTED));
                    ben.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                    click(ben, t[0], false);
                    ben.closeContainer();
                    check("never again: " + ben.overlay.size(), ben.overlay.size() == 1);
                    FakePlayers.remove(ben);
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
                    Lines.Found own = Lines.find("villager.farmer", "fluff");
                    check("farmer + fluff has the farmer's own lines: " + own,
                            own != null && own.prefix().equals("mic_rtsim.talk.villager.farmer.fluff"));
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
}
