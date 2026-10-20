package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.market.CargoAccess;
import com.minecraftindustrialcomplex.mic_rtsim.market.MarketBook;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.entity.npc.VillagerProfession;
import net.minecraft.world.entity.npc.WanderingTrader;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.trading.ItemCost;
import net.minecraft.world.item.trading.MerchantOffer;
import net.minecraft.world.item.trading.MerchantOffers;
import net.minecraft.world.level.GameType;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * Trades (PLAN.md §8.5, §8.6): a caravan sells out of its llamas' chests (C21), and a villager in an
 * adopted village buys through the village treasury until it runs dry and the offer shows crossed out
 * (C14, walkthrough B). Every trade goes through vanilla's own trading-screen code
 * ({@link RtsimTests#trade}), so the bridge sees it at {@code MerchantResultSlot.onTake} as in play.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class TradeGameTests {
    private TradeGameTests() {}

    private static int left(MerchantOffer o) {
        return o.getMaxUses() - o.getUses();
    }

    private static int cargo(TraderLlama llama, Item item) {
        CargoAccess c = BodyKinds.TRADER_LLAMA.ownCargo(llama);
        return c == null ? 0 : c.count(item);
    }

    /**
     * A caravan arrives (C25) and its offers come from its chest cargo (C21): the engine prices them
     * (MERCHANT_OFFERS); each offer's uses are what the llamas carry; a sale takes the goods out of a
     * chest and puts the payment into one; when the wheat is gone the offer shows crossed out.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400)
    public static void aCaravansOffersComeFromItsChestCargo(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        Vec3 at = RtsimTests.centre(helper);
        ScriptedEngine stub = RtsimTests.engine();
        stub.spawnGroup(agent, 1, BodyKinds.HUMAN_CARAVAN, RtsimTests.dimension(helper), at.x, at.y, at.z, List.of(
                ScriptedEngine.MemberSpec.plain(0),
                new ScriptedEngine.MemberSpec(1, 0f, 0, 3, List.of(new ScriptedEngine.ItemSpec(0, "minecraft:wheat", 30),
                        new ScriptedEngine.ItemSpec(1, "minecraft:leather", 10))),
                new ScriptedEngine.MemberSpec(2, 0f, 0, 3, List.of(new ScriptedEngine.ItemSpec(0, "minecraft:emerald", 5)))));
        stub.merchantOffers(agent, 1, 0, List.of(
                new ScriptedEngine.OfferSpec("minecraft:emerald", 1, null, 0, "minecraft:wheat", 10, 0, Cmd.MerchantOffers.O_FLAG_SELLS),
                new ScriptedEngine.OfferSpec("minecraft:emerald", 2, null, 0, "minecraft:leather", 5, 0, Cmd.MerchantOffers.O_FLAG_SELLS),
                new ScriptedEngine.OfferSpec("minecraft:iron_ingot", 2, null, 0, "minecraft:emerald", 1, 0, Cmd.MerchantOffers.O_FLAG_BUYS)));
        Player player = helper.makeMockPlayer(GameType.SURVIVAL);
        Object[] e = new Object[3];

        helper.startSequence()
                .thenWaitUntil(() -> check("the offers arrived", RtsimTests.embodied(agent).members[0].offers != null))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    WanderingTrader trader = (WanderingTrader) RtsimTests.member(g, 0);
                    TraderLlama wheatLlama = (TraderLlama) RtsimTests.member(g, 1), cashLlama = (TraderLlama) RtsimTests.member(g, 2);
                    e[0] = trader;
                    e[1] = wheatLlama;
                    e[2] = cashLlama;
                    MerchantOffers offers = trader.getOffers();
                    check("exactly the engine's three offers (vanilla's random ones are gone): " + offers.size(), offers.size() == 3);
                    check("wheat: ten for an emerald, three times (30 in the chest)", offers.get(0).getResult().is(Items.WHEAT)
                            && offers.get(0).getResult().getCount() == 10 && left(offers.get(0)) == 3);
                    check("leather: twice (10 in the chest, 5 a trade)", left(offers.get(1)) == 2);
                    check("iron for emeralds: five times (the caravan carries 5 emeralds)", left(offers.get(2)) == 5);
                    ItemStack got = RtsimTests.trade(trader, player, 0, new ItemStack(Items.EMERALD, 1));
                    check("the player got ten wheat: " + got, got.is(Items.WHEAT) && got.getCount() == 10);
                    check("the wheat came out of the chest", cargo(wheatLlama, Items.WHEAT) == 20);
                    check("the emerald went into a chest", cargo(wheatLlama, Items.EMERALD) + cargo(cashLlama, Items.EMERALD) == 6);
                    check("two wheat trades left", left(offers.get(0)) == 2);
                    check("six emeralds now: the iron offer has six uses", left(offers.get(2)) == 6);
                    RtsimTests.trade(trader, player, 0, new ItemStack(Items.EMERALD, 1));
                    RtsimTests.trade(trader, player, 0, new ItemStack(Items.EMERALD, 1));
                    check("the chest's wheat is gone", cargo(wheatLlama, Items.WHEAT) == 0);
                    check("the wheat offer shows crossed out", offers.get(0).isOutOfStock());
                    check("and gives nothing more", RtsimTests.trade(trader, player, 0, new ItemStack(Items.EMERALD, 1)).isEmpty());
                    record("caravanCargo", "after three sales", "wheat " + cargo(wheatLlama, Items.WHEAT) + ", emeralds "
                            + (cargo(wheatLlama, Items.EMERALD) + cargo(cashLlama, Items.EMERALD)));
                })
                .thenWaitUntil(() -> check("three TRADEs reached the engine",
                        RtsimTests.recordsWith(stub, Obs.TRADE, Obs.Trade.AGENT, agent).size() == 3))
                .thenExecute(() -> {
                    List<Long> ops = new ArrayList<>();
                    for (ScriptedEngine.Received r : RtsimTests.recordsWith(stub, Obs.TRADE, Obs.Trade.AGENT, agent)) {
                        check("the merchant sold", r.aux() == Obs.Trade.MERCHANT_SELLS);
                        check("member 0", r.view().getInt(Obs.Trade.MEMBER) == 0);
                        check("one emerald paid", r.view().getInt(Obs.Trade.COST_A_COUNT) == 1
                                && "minecraft:emerald".equals(stub.javaDictName(r.view().getInt(Obs.Trade.COST_A_ITEM))));
                        check("ten wheat sold", r.view().getInt(Obs.Trade.RESULT_COUNT) == 10);
                        long op = r.view().getLong(Obs.Trade.OP_ID);
                        check("a Java op id", (op & Obs.JAVA_OP_BIT) != 0 && !ops.contains(op));
                        ops.add(op);
                    }
                    record("caravanCargo", "TRADE op ids", ops.size());
                    stub.despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the caravan despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    private static Villager farmer(GameTestHelper helper, double x, double z) {
        Villager villager = helper.spawn(EntityType.VILLAGER, new Vec3(x, 2, z));
        villager.setNoAi(true);
        villager.setVillagerData(villager.getVillagerData().setProfession(VillagerProfession.FARMER).setLevel(1));
        MerchantOffers offers = villager.getOffers();
        offers.clear();
        offers.add(new MerchantOffer(new ItemCost(Items.WHEAT, 20), new ItemStack(Items.EMERALD), 16, 2, 0.05f));
        return villager;
    }

    /**
     * A robbery and a trade in the same tick (C21, C24): the chest lost 5 wheat to something that is
     * not the bridge, then a player bought 10. The engine hears the 5 as CARGO_CHANGED and the 10 as
     * the TRADE, each once; the robbery is not folded into the sale or lost behind it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300)
    public static void aRobberyThenATradeAreReportedApart(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        Vec3 at = RtsimTests.centre(helper);
        ScriptedEngine stub = RtsimTests.engine();
        stub.spawnGroup(agent, 1, BodyKinds.HUMAN_CARAVAN, RtsimTests.dimension(helper), at.x, at.y, at.z, List.of(
                ScriptedEngine.MemberSpec.plain(0),
                new ScriptedEngine.MemberSpec(1, 0f, 0, 3, List.of(new ScriptedEngine.ItemSpec(0, "minecraft:wheat", 30))),
                ScriptedEngine.MemberSpec.plain(2)));
        stub.merchantOffers(agent, 1, 0, List.of(
                new ScriptedEngine.OfferSpec("minecraft:emerald", 1, null, 0, "minecraft:wheat", 10, 0, Cmd.MerchantOffers.O_FLAG_SELLS)));
        Player player = helper.makeMockPlayer(GameType.SURVIVAL);
        helper.startSequence()
                .thenWaitUntil(() -> check("the offers arrived", RtsimTests.embodied(agent).members[0].offers != null))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    WanderingTrader trader = (WanderingTrader) RtsimTests.member(g, 0);
                    TraderLlama llama = (TraderLlama) RtsimTests.member(g, 1);
                    llama.getSlot(500).get().shrink(5); // 5 wheat taken by something that is not the bridge
                    ItemStack got = RtsimTests.trade(trader, player, 0, new ItemStack(Items.EMERALD, 1));
                    check("the sale went through: " + got, got.is(Items.WHEAT) && got.getCount() == 10);
                    check("15 wheat left in the chest: " + cargo(llama, Items.WHEAT), cargo(llama, Items.WHEAT) == 15);
                    check("one more sale of ten left", left(trader.getOffers().get(0)) == 1);
                })
                .thenWaitUntil(() -> check("the TRADE and the CARGO_CHANGED reached the engine",
                        RtsimTests.recordsWith(stub, Obs.TRADE, Obs.Trade.AGENT, agent).size() == 1
                                && !RtsimTests.recordsWith(stub, Obs.CARGO_CHANGED, Obs.CargoChanged.AGENT, agent).isEmpty()))
                .thenIdle(40) // a cargo pass or two: nothing more may be reported
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> changes = RtsimTests.recordsWith(stub, Obs.CARGO_CHANGED, Obs.CargoChanged.AGENT, agent);
                    check("one CARGO_CHANGED: " + changes.size(), changes.size() == 1);
                    ScriptedEngine.Received c = changes.get(0);
                    int item = Obs.CargoChanged.ITEMS;
                    check("the robbery alone: wheat -5", c.view().getInt(Obs.CargoChanged.ITEM_COUNT) == 1
                            && "minecraft:wheat".equals(stub.javaDictName(c.view().getInt(item + Obs.Item.ITEM)))
                            && c.view().getInt(item + Obs.Item.COUNT) == -5);
                    ScriptedEngine.Received t = RtsimTests.recordsWith(stub, Obs.TRADE, Obs.Trade.AGENT, agent).get(0);
                    check("the trade alone: ten wheat", t.view().getInt(Obs.Trade.RESULT_COUNT) == 10);
                    record("robberyThenTrade", "reported", "CARGO_CHANGED wheat -5, TRADE wheat 10");
                    stub.despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the caravan despawned", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    /**
     * Two players at two farmers of one village (C14): the treasury is the village's, so a sale at one
     * re-caps the offer the other player has open. Without it each screen would show the whole treasury.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 300, batch = "mic_rtsim_p1_screens")
    public static void twoScreensShareOneTreasury(GameTestHelper helper) {
        Village v = RtsimTests.adoptArena(helper, "human");
        ScriptedEngine stub = RtsimTests.engine();
        Villager a = farmer(helper, 3.5, 3.5), b = farmer(helper, 7.5, 7.5);
        stub.market(v.id, 0, "minecraft:emerald", 3_000, 20_000, 1, 2, List.of(
                new ScriptedEngine.GoodSpec("minecraft:wheat", 400_000, 400_000, 50_000, 3, 4)));
        Player pa = helper.makeMockPlayer(GameType.SURVIVAL), pb = helper.makeMockPlayer(GameType.SURVIVAL);
        helper.startSequence()
                .thenWaitUntil(() -> check("the market snapshot arrived", RtsimTests.session().phase1.markets.known(v.id) != null))
                .thenExecute(() -> {
                    RtsimTests.session().phase1.beforeTrading(pb, b); // player B opens b's screen and keeps it open
                    b.setTradingPlayer(pb);
                    check("b offers three sales (three emeralds)", left(b.getOffers().get(0)) == 3);
                    RtsimTests.session().phase1.beforeTrading(pa, a);
                    for (int i = 0; i < 2; i++)
                        check("A's sale " + (i + 1), RtsimTests.trade(a, pa, 0, new ItemStack(Items.WHEAT, 64)).is(Items.EMERALD));
                    int bLeft = left(b.getOffers().get(0));
                    record("twoScreens", "b's uses after two sales at a", bLeft);
                    check("b's open screen was re-capped to the one emerald left: " + bLeft, bLeft == 1);
                    check("a shows the same", left(a.getOffers().get(0)) == 1);
                    b.setTradingPlayer(null);
                    a.discard();
                    b.discard();
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    /**
     * Walkthrough B (C14): a farmer buys wheat through the village treasury. Each sale drains it at once;
     * the price per emerald rises in wheat as the stock grows; when the treasury cannot pay for one more
     * bundle the offer shows crossed out. A snapshot from before the sales does not give the money back;
     * a caravan's refill (a snapshot that includes them) reopens it.
     */
    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 400, batch = "mic_rtsim_p1_treasury")
    public static void sellingDrainsTheTreasuryAndTheOfferCrossesOut(GameTestHelper helper) {
        Village v = RtsimTests.adoptArena(helper, "human");
        ScriptedEngine stub = RtsimTests.engine();
        Villager villager = helper.spawn(EntityType.VILLAGER, new Vec3(5.5, 2, 5.5));
        villager.setNoAi(true);
        villager.setVillagerData(villager.getVillagerData().setProfession(VillagerProfession.FARMER).setLevel(1));
        MerchantOffers offers = villager.getOffers();
        offers.clear(); // the farmer's novice trades, fixed instead of rolled (vanilla's own shapes)
        offers.add(new MerchantOffer(new ItemCost(Items.WHEAT, 20), new ItemStack(Items.EMERALD), 16, 2, 0.05f));
        offers.add(new MerchantOffer(new ItemCost(Items.EMERALD, 1), new ItemStack(Items.BREAD, 6), 16, 1, 0.05f));
        stub.market(v.id, 0, "minecraft:emerald", 3_000, 20_000, 1, 2, List.of(
                new ScriptedEngine.GoodSpec("minecraft:wheat", 400_000, 400_000, 50_000, 3, 4),
                new ScriptedEngine.GoodSpec("minecraft:bread", 50_000, 40_000, 200_000, 3, 4)));
        Player player = helper.makeMockPlayer(GameType.SURVIVAL);
        long[] lastSeq = {0};
        List<Integer> wheatPerEmerald = new ArrayList<>();

        helper.startSequence()
                .thenWaitUntil(() -> check("the market snapshot arrived", RtsimTests.session().phase1.markets.known(v.id) != null))
                .thenExecute(() -> {
                    RtsimTests.session().phase1.beforeTrading(player, villager); // what the interaction event does
                    Village found = com.minecraftindustrialcomplex.mic_rtsim.village.Residents.villageOf(RtsimTests.session().phase1.villages, villager);
                    check("the farmer is a resident of the test's village: " + (found == null ? "none" : Long.toHexString(found.id))
                            + " vs " + Long.toHexString(v.id) + " at " + villager.blockPosition() + " in " + v.box, found == v);
                    MerchantOffer wheat = offers.get(0);
                    check("still two offers, in the same order", offers.size() == 2 && offers.get(0).getCostA().is(Items.WHEAT));
                    // At 0.05 a wheat and falling as the stock grows, the fewest wheat one emerald pays for is 21.
                    check("about twenty wheat for an emerald at 0.05 a wheat: " + wheat.getCostA(), wheat.getCostA().getCount() == 21
                            && wheat.getResult().is(Items.EMERALD) && wheat.getResult().getCount() == 1);
                    check("three emeralds in the treasury: three uses", left(wheat) == 3);
                    check("bread is sold out of stock, priced in emeralds", offers.get(1).getCostA().is(Items.EMERALD) && left(offers.get(1)) > 0);
                    for (int i = 0; i < 3; i++) {
                        wheatPerEmerald.add(offers.get(0).getCostA().getCount());
                        ItemStack got = RtsimTests.trade(villager, player, 0, new ItemStack(Items.WHEAT, 64));
                        check("sale " + (i + 1) + " paid an emerald: " + got, got.is(Items.EMERALD) && got.getCount() == 1);
                    }
                    record("treasury", "wheat per emerald, sale by sale", wheatPerEmerald);
                    check("each sale raised the stock, so an emerald costs no less wheat",
                            wheatPerEmerald.get(1) >= wheatPerEmerald.get(0) && wheatPerEmerald.get(2) >= wheatPerEmerald.get(1));
                    check("the treasury is empty", RtsimTests.session().phase1.markets.known(v.id).treasury() == 0);
                    check("the wheat offer shows crossed out", offers.get(0).isOutOfStock());
                    check("and pays nothing more", RtsimTests.trade(villager, player, 0, new ItemStack(Items.WHEAT, 64)).isEmpty());
                    // Vanilla's restock (updateDemand, then resetUses on every offer) through the restock mixins.
                    villager.restock();
                    check("a restock leaves an empty treasury's offer crossed out", offers.get(0).isOutOfStock());
                    check("and vanilla's demand did not read the sim's uses as sales: " + offers.get(0).getDemand(),
                            offers.get(0).getDemand() == 0);
                    check("bread still capped by the stock, not refilled to vanilla's 16", left(offers.get(1)) < 16);
                    record("treasury", "after a restock", "wheat crossed out, demand " + offers.get(0).getDemand()
                            + ", bread uses " + left(offers.get(1)));
                })
                .thenWaitUntil(() -> check("three TRADEs reached the engine", RtsimTests.recordsWith(stub, Obs.TRADE, Obs.Trade.NODE, v.id).size() == 3))
                .thenExecute(() -> {
                    for (ScriptedEngine.Received r : RtsimTests.recordsWith(stub, Obs.TRADE, Obs.Trade.NODE, v.id)) {
                        check("the village bought", r.aux() == Obs.Trade.MERCHANT_BUYS);
                        check("a resident, not a puppet", r.view().getLong(Obs.Trade.AGENT) == 0);
                        check("a farmer", "minecraft:farmer".equals(stub.javaDictName(r.view().getInt(Obs.Trade.PROFESSION))));
                        lastSeq[0] = Math.max(lastSeq[0], r.obsSeq());
                    }
                    // A late snapshot, computed before the sales: the local sales stand.
                    stub.market(v.id, 0, "minecraft:emerald", 3_000, 20_000, 1, 2, List.of(
                            new ScriptedEngine.GoodSpec("minecraft:wheat", 400_000, 400_000, 50_000, 3, 4)));
                })
                .thenIdle(5)
                .thenExecute(() -> {
                    MarketBook<Item> book = RtsimTests.session().phase1.markets.known(v.id);
                    check("a snapshot from before the sales did not refill the treasury: " + book.treasury(), book.treasury() == 0);
                    // Walkthrough B3: a caravan bought the surplus and paid; the snapshot includes the sales.
                    stub.market(v.id, lastSeq[0], "minecraft:emerald", 5_000, 20_000, 1, 2, List.of(
                            new ScriptedEngine.GoodSpec("minecraft:wheat", 300_000, 400_000, 50_000, 3, 4)));
                })
                .thenWaitUntil(() -> check("the refill arrived", RtsimTests.session().phase1.markets.known(v.id).treasury() == 5_000))
                .thenExecute(() -> {
                    RtsimTests.session().phase1.beforeTrading(player, villager);
                    MerchantOffer wheat = offers.get(0);
                    check("the offer reopened", !wheat.isOutOfStock() && left(wheat) == 5);
                    check("paying more again: fewer wheat per emerald than at the glut", wheat.getCostA().getCount() < wheatPerEmerald.get(2));
                    record("treasury", "after the refill", wheat.getCostA().getCount() + " wheat per emerald, " + left(wheat) + " uses");
                    villager.discard();
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }
}
