package com.minecraftindustrialcomplex.mic_rtsim_ribbits.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.build.PlayerBlocks;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.market.CargoAccess;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.MicRtsimRibbits;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.RibbitBands;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.RibbitCargo;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.RibbitMerchantAdapter;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.Ribbits;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.mixin.RibbitEntityAccessor;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.entity.goal.RibbitApplyBuffGoal;
import com.yungnickyoung.minecraft.ribbits.module.RibbitProfessionModule;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.trading.ItemCost;
import net.minecraft.world.item.trading.MerchantOffer;
import net.minecraft.world.item.trading.MerchantOffers;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The Ribbits plugin with Ribbits 4.1.6, YUNG's API and GeckoLib on the GameTest server (C7, C23):
 * sorcerers who remember, village growth from YUNG's own house templates, merchant caravans that sell
 * out of their pack, bands that play around their own master, and ribbit merchants' trades through the
 * treasury.
 */
@GameTestHolder(MicRtsimRibbits.MODID)
@PrefixGameTestTemplate(false)
public final class RibbitGameTests {
    private RibbitGameTests() {}

    /** A server player in the level, in survival (so the sorcerer's combat targeting counts it). */
    static ServerPlayer player(GameTestHelper helper, Vec3 at) {
        return com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.add(helper.getLevel(), at, "ribbits-test");
    }

    static void leave(GameTestHelper helper, ServerPlayer... players) {
        for (ServerPlayer p : players)
            com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.remove(p);
    }

    static RibbitEntity spawn(GameTestHelper helper, Vec3 at, com.yungnickyoung.minecraft.ribbits.data.RibbitProfession profession) {
        RibbitEntity r = Ribbits.create(helper.getLevel(), at, profession, Ribbits.umbrella(0));
        check("a ribbit was made", r != null);
        r.setPersistenceRequired();
        helper.getLevel().addFreshEntity(r);
        return r;
    }

    private static int left(MerchantOffer o) {
        return o.getMaxUses() - o.getUses();
    }

    private static StubEngine.Received result(StubEngine stub, long op) {
        List<StubEngine.Received> rs = RtsimTests.recordsWith(stub, Obs.COMMAND_RESULT, Obs.CommandResult.OP_ID, op);
        return rs.isEmpty() ? null : rs.get(rs.size() - 1);
    }

    private static long engineOp() {
        return java.util.concurrent.ThreadLocalRandom.current().nextLong() & Long.MAX_VALUE;
    }

    // ---- sorcerers who remember -------------------------------------------------------------------

    /**
     * Walkthrough F: a sorcerer buffs a friend standing next to it and leaves out a player its village
     * dislikes; with only the disliked player near, it does not cast at all; once the village forgives
     * them, it casts again. The goal is Ribbits' own {@code RibbitApplyBuffGoal}, filtered by the mixin.
     */
    @GameTest(template = "arena", timeoutTicks = 300, batch = "mic_rtsim_ribbits_sorcerer")
    public static void aSorcererSkipsADislikedPlayer(GameTestHelper helper) {
        Village v = RtsimTests.adoptArena(helper, MicRtsimRibbits.CULTURE);
        Vec3 c = RtsimTests.centre(helper);
        RibbitEntity sorcerer = spawn(helper, c, RibbitProfessionModule.SORCERER);
        sorcerer.setNoAi(true); // the test drives the buff goal itself
        ServerPlayer friend = player(helper, c.add(2, 0, 0)), foe = player(helper, c.add(-2, 0, 0));
        StubEngine stub = RtsimTests.stub();
        stub.sentiment(v.id, -32, 32, true, Map.of(foe.getUUID(), -100, friend.getUUID(), 64));
        RibbitApplyBuffGoal goal = ((RibbitEntityAccessor) sorcerer).mic_rtsim_ribbits$applyBuffGoal();

        helper.startSequence()
                .thenWaitUntil(() -> check("the village's sentiment arrived", RtsimApi.sentiment(v.id) != null))
                .thenExecute(() -> {
                    check("the sorcerer belongs to the village", RtsimApi.villageOf(sorcerer) == v);
                    sorcerer.setBuffCooldown(0);
                    check("someone it may buff is near", goal.canUse());
                    goal.start();
                    for (int i = 0; i < 30; i++)
                        goal.tick();
                    record("sorcerer", "effects (friend, foe)", friend.getActiveEffects().size() + ", " + foe.getActiveEffects().size());
                    check("the friend was buffed", !friend.getActiveEffects().isEmpty());
                    check("the disliked player was left out", foe.getActiveEffects().isEmpty());
                    goal.stop();
                    // Only the disliked player is near: the sorcerer does not cast at all.
                    friend.moveTo(c.x + 300, c.y, c.z);
                    sorcerer.setBuffCooldown(0);
                    check("nobody it likes is near: no cast", !goal.canUse());
                    // The village forgives them (walkthrough F3).
                    stub.sentiment(v.id, -32, 32, false, Map.of(foe.getUUID(), 10));
                })
                .thenWaitUntil(() -> check("forgiven", !RtsimApi.sentiment(v.id).dislikes(foe.getUUID())))
                .thenExecute(() -> {
                    sorcerer.setBuffCooldown(0);
                    check("it casts for them again", goal.canUse());
                    leave(helper, friend, foe);
                    sorcerer.discard();
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    // ---- village growth -----------------------------------------------------------------------------

    /**
     * Walkthrough H: a ribbit village grows by one of YUNG's own houses
     * ({@code ribbits:houses/small_house_red_1} with the {@code ribbits:main} processors), on the first
     * free plot of a search around its centre, never over a block a player placed; then a new resident
     * moves in at the house's resident slot (the {@code ribbits:ribbits} jigsaw), homed there.
     */
    @GameTest(template = "plot", timeoutTicks = 600, batch = "mic_rtsim_ribbits_house")
    public static void aRibbitHouseGoesOnAFreePlotAndAResidentMovesIn(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        RtsimSession s = RtsimTests.session();
        BlockPos centre = helper.absolutePos(new BlockPos(14, 2, 14));
        BoundingBox box = BoundingBox.fromCorners(helper.absolutePos(new BlockPos(0, 0, 0)), helper.absolutePos(new BlockPos(27, 15, 27)));
        long id = 0x0E00_0000_0000_0000L | (java.util.concurrent.ThreadLocalRandom.current().nextLong() & 0xFFFF_FFFF_FFFFL);
        Village v = VillageAdoption.adoptDirect(s, s.phase1.villages, id, level.dimension(), ResourceLocation.parse("ribbits:ribbit_village"),
                MicRtsimRibbits.CULTURE, box, centre, s.gameTime());
        v.margin = 0;
        // A block a player placed in the middle: no house may cover it.
        helper.setBlock(new BlockPos(14, 2, 14), Blocks.SNOW);
        PlayerBlocks.mark(level.getChunkAt(centre), centre);
        StubEngine stub = RtsimTests.stub();
        String dim = RtsimTests.dimension(helper);
        long house = engineOp(), resident = engineOp();
        stub.placeHouse(house, v.id, dim, "ribbits:houses/small_house_red_1", "ribbits:main", centre.getX(), centre.getY(), centre.getZ(),
                0, 0, Cmd.PlaceHouse.FLAG_SEARCH, 14, 42, 0);
        stub.spawnResident(resident, v.id, dim, "ribbits:ribbit", "ribbits:nitwit", Cmd.SpawnResident.FLAG_AT_HOUSE, 0, 0, 0, house);
        BlockPos[] slot = new BlockPos[1];
        BoundingBox[] built = new BoundingBox[1];

        helper.startSequence()
                .thenWaitUntil(() -> check("the house was answered", result(stub, house) != null))
                .thenExecute(() -> {
                    check("built: status " + result(stub, house).aux(), result(stub, house).aux() == Obs.CommandResult.APPLIED);
                    StubEngine.Received hp = RtsimTests.recordsWith(stub, Obs.HOUSE_PLACED, Obs.HousePlaced.OP_ID, house).get(0);
                    var b = hp.view();
                    built[0] = new BoundingBox(b.getInt(Obs.HousePlaced.MIN_X), b.getInt(Obs.HousePlaced.MIN_Y), b.getInt(Obs.HousePlaced.MIN_Z),
                            b.getInt(Obs.HousePlaced.MAX_X), b.getInt(Obs.HousePlaced.MAX_Y), b.getInt(Obs.HousePlaced.MAX_Z));
                    slot[0] = new BlockPos(b.getInt(Obs.HousePlaced.SLOT_X), b.getInt(Obs.HousePlaced.SLOT_Y), b.getInt(Obs.HousePlaced.SLOT_Z));
                    check("not over the player's block", !built[0].isInside(centre));
                    check("the player's block is still there", level.getBlockState(centre).is(Blocks.SNOW));
                    int toadstool = 0, jigsaws = 0;
                    for (BlockPos p : BlockPos.betweenClosed(built[0].minX(), built[0].minY(), built[0].minZ(), built[0].maxX(), built[0].maxY(), built[0].maxZ())) {
                        var st = level.getBlockState(p);
                        if (st.getBlock().builtInRegistryHolder().key().location().toString().equals("ribbits:red_toadstool"))
                            toadstool++;
                        if (st.is(Blocks.JIGSAW))
                            jigsaws++;
                    }
                    record("ribbitHouse", "house", built[0] + ", " + toadstool + " red toadstool blocks, resident slot " + slot[0]);
                    check("YUNG's red toadstool house stands there", toadstool > 50);
                    check("its jigsaw blocks were replaced", jigsaws == 0);
                })
                .thenWaitUntil(() -> check("the resident was answered (after a house)", built[0] != null && result(stub, resident) != null))
                .thenExecute(() -> {
                    check("spawned: status " + result(stub, resident).aux(), result(stub, resident).aux() == Obs.CommandResult.APPLIED);
                    List<RibbitEntity> found = level.getEntitiesOfClass(RibbitEntity.class, new AABB(slot[0]).inflate(2));
                    check("a ribbit stands in the new house", found.size() == 1);
                    RibbitEntity r = found.get(0);
                    check("a nitwit", "ribbits:nitwit".equals(Ribbits.professionName(r)));
                    check("homed in its house: " + r.getHomePosition(), r.getHomePosition().closerThan(slot[0], 1.5));
                    check("an ordinary entity (C2), not a puppet", !RtsimApi.isPuppet(r));
                    check("it carries its COMMAND_RESULT until the engine has it", r.hasData(RtsimAttachments.PENDING));
                    record("ribbitHouse", "resident", Ribbits.professionName(r) + " at " + r.blockPosition());
                    r.discard();
                    s.phase1.villages.remove(v.id);
                })
                .thenSucceed();
    }

    // ---- merchant caravans --------------------------------------------------------------------------

    /**
     * A ribbit merchant caravan (C7) sells out of the pack it carries (P31's recommendation): a sale
     * takes the goods out and puts the amethyst in; killing it drops the whole pack once, with the DIED.
     */
    @GameTest(template = "arena", timeoutTicks = 300)
    public static void aRibbitMerchantSellsOutOfItsPackAndDropsItWhenKilled(GameTestHelper helper) {
        long agent = RtsimTests.newAgent();
        Vec3 c = RtsimTests.centre(helper);
        StubEngine stub = RtsimTests.stub();
        stub.spawnGroup(agent, 1, RibbitMerchantAdapter.NAME, RtsimTests.dimension(helper), c.x, c.y, c.z, List.of(
                new StubEngine.MemberSpec(0, 0f, 1, -1, List.of(new StubEngine.ItemSpec(0, "minecraft:slime_ball", 20),
                        new StubEngine.ItemSpec(1, "minecraft:amethyst_shard", 5)))));
        stub.merchantOffers(agent, 1, 0, List.of(
                new StubEngine.OfferSpec("minecraft:amethyst_shard", 1, null, 0, "minecraft:slime_ball", 4, 0, Cmd.MerchantOffers.O_FLAG_SELLS)));
        Player player = helper.makeMockPlayer(GameType.SURVIVAL);
        RibbitEntity[] m = new RibbitEntity[1];

        helper.startSequence()
                .thenWaitUntil(() -> check("the offers arrived", RtsimTests.embodied(agent).members[0].offers != null))
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    m[0] = (RibbitEntity) RtsimTests.member(g, 0);
                    check("a ribbit merchant", "ribbits:merchant".equals(Ribbits.professionName(m[0])));
                    MerchantOffers offers = m[0].getOffers();
                    check("only the caravan's offer", offers.size() == 1 && left(offers.get(0)) == 5);
                    MerchantOffer o = offers.get(0);
                    String state = o.getCostA() + " -> " + o.getResult() + ", uses " + o.getUses() + "/" + o.getMaxUses()
                            + ", satisfied " + o.satisfiedBy(new ItemStack(Items.AMETHYST_SHARD, 1), ItemStack.EMPTY) + ", offers " + m[0].getOffers().size();
                    ItemStack got = RtsimTests.trade(m[0], player, 0, new ItemStack(Items.AMETHYST_SHARD, 1));
                    check("four slime balls for a shard: " + got + " (" + state + ")", got.is(Items.SLIME_BALL) && got.getCount() == 4);
                    CargoAccess pack = RibbitCargo.of(m[0]);
                    check("out of its pack", pack.count(Items.SLIME_BALL) == 16);
                    check("the shard into its pack", pack.count(Items.AMETHYST_SHARD) == 6);
                    m[0].hurt(helper.getLevel().damageSources().playerAttack(player), 1000f);
                })
                .thenWaitUntil(() -> check("DIED reached the engine", !RtsimTests.recordsFor(stub, Obs.DIED, agent).isEmpty()))
                .thenExecute(() -> {
                    List<StubEngine.Received> all = RtsimTests.allFor(stub, agent);
                    StubEngine.Received died = RtsimTests.recordsFor(stub, Obs.DIED, agent).get(0);
                    StubEngine.Received state = all.get(all.indexOf(died) - 1);
                    check("its state at death lists the pack", state.kind() == Obs.MEMBER_STATE && state.view().getInt(Obs.MemberState.ITEM_COUNT) == 2);
                    int slime = 0, shards = 0, carriers = 0;
                    for (ItemEntity e : helper.getLevel().getEntitiesOfClass(ItemEntity.class, new AABB(BlockPos.containing(c)).inflate(8))) {
                        if (e.getItem().is(Items.SLIME_BALL))
                            slime += e.getItem().getCount();
                        if (e.getItem().is(Items.AMETHYST_SHARD))
                            shards += e.getItem().getCount();
                        if (e.hasData(RtsimAttachments.PENDING))
                            carriers++;
                        e.discard();
                    }
                    record("ribbitMerchant", "dropped", slime + " slime, " + shards + " shards, " + carriers + " carrying the DIED");
                    check("the pack dropped once: 16 slime balls", slime == 16);
                    check("and 6 shards", shards == 6);
                    check("the drops carry the DIED", carriers >= 2);
                })
                .thenWaitUntil(() -> check("the agent is gone", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    // ---- touring bands ------------------------------------------------------------------------------

    /**
     * A band comes to town (walkthrough G): four musicians that stopped play around their own master,
     * even with a village band already playing next to them (Ribbits alone would have them join it).
     */
    @GameTest(template = "arena", timeoutTicks = 600, batch = "mic_rtsim_ribbits_band")
    public static void aTouringBandPlaysAroundItsOwnMaster(GameTestHelper helper) {
        helper.getLevel().setDayTime(6000); // Ribbits' bands play by day
        long agent = RtsimTests.newAgent();
        Vec3 c = RtsimTests.centre(helper);
        RibbitEntity local = spawn(helper, c.add(3, 0, 3), RibbitProfessionModule.NITWIT);
        local.setNoAi(true);
        local.setMasterRibbit(local); // a village band's master, playing
        local.setPlayingInstrument(true);
        StubEngine stub = RtsimTests.stub();
        stub.spawnGroup(agent, 1, RibbitBands.BAND, RtsimTests.dimension(helper), c.x, c.y, c.z, List.of(
                StubEngine.MemberSpec.plain(0), StubEngine.MemberSpec.plain(1), StubEngine.MemberSpec.plain(2), StubEngine.MemberSpec.plain(3)));

        helper.startSequence()
                .thenWaitUntil(() -> RtsimTests.embodied(agent))
                .thenWaitUntil(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    RibbitEntity leader = (RibbitEntity) g.leader();
                    for (int i = 0; i < 4; i++) {
                        RibbitEntity r = (RibbitEntity) RtsimTests.member(g, i);
                        check("member " + i + " plays around the band's leader, not " + r.getMasterRibbit(), r.getMasterRibbit() == leader);
                    }
                })
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    record("band", "masters", "all four around " + g.leader() + "; the village band's master was " + local);
                    stub.despawnGroup(agent, 1);
                    local.discard();
                })
                .thenWaitUntil(() -> check("the band left", RtsimTests.group(agent) == null))
                .thenSucceed();
    }

    // ---- C23: ribbit merchants through the treasury -------------------------------------------------

    /**
     * Ribbit merchants trade through their village's treasury (C23), in amethyst, and restock through it
     * (the {@code RibbitEntity.restock} mixin). Ribbits have no gossip, so a player the village dislikes
     * gets the engine's markdown on what it is paid (PLAN.md §8.7).
     */
    @GameTest(template = "arena", timeoutTicks = 300, batch = "mic_rtsim_ribbits_treasury")
    public static void ribbitMerchantsTradeAndRestockThroughTheTreasury(GameTestHelper helper) {
        Village v = RtsimTests.adoptArena(helper, MicRtsimRibbits.CULTURE);
        RibbitEntity merchant = spawn(helper, RtsimTests.centre(helper), RibbitProfessionModule.MERCHANT);
        merchant.setNoAi(true);
        MerchantOffers offers = merchant.getOffers();
        offers.clear(); // one of Ribbits' own shapes (AmethystForItems), fixed instead of rolled
        offers.add(new MerchantOffer(new ItemCost(Items.SLIME_BALL, 10), new ItemStack(Items.AMETHYST_SHARD), 12, 2, 0.05f));
        Player fair = helper.makeMockPlayer(GameType.SURVIVAL), disliked = helper.makeMockPlayer(GameType.SURVIVAL);
        StubEngine stub = RtsimTests.stub();
        stub.market(v.id, 0, "minecraft:amethyst_shard", 2_000, 20_000, 1, 2, List.of(
                new StubEngine.GoodSpec("minecraft:slime_ball", 100_000, 100_000, 100_000, 3, 4)));
        stub.sentiment(v.id, -32, 32, true, Map.of(disliked.getUUID(), -128));
        int[] fairCount = new int[1];

        helper.startSequence()
                .thenWaitUntil(() -> check("market and sentiment arrived", RtsimTests.session().phase1.markets.known(v.id) != null
                        && RtsimApi.sentiment(v.id) != null))
                .thenExecute(() -> {
                    RtsimSession s = RtsimTests.session();
                    s.phase1.beforeTrading(fair, merchant);
                    MerchantOffer o = offers.get(0);
                    fairCount[0] = o.getCostA().getCount();
                    check("repriced from the market: slime for a shard at 0.1 a ball: " + o.getCostA(), o.getCostA().is(Items.SLIME_BALL)
                            && fairCount[0] >= 10 && fairCount[0] <= 12 && o.getResult().is(Items.AMETHYST_SHARD));
                    check("two shards in the treasury: two uses", left(o) == 2);
                    s.phase1.beforeTrading(disliked, merchant);
                    int dislikedCount = offers.get(0).getCostA().getCount();
                    record("ribbitTreasury", "slime per shard (fair, disliked)", fairCount[0] + ", " + dislikedCount);
                    check("a disliked player must bring more for the same shard", dislikedCount > fairCount[0]);
                    // Ribbits' restock resets uses; in an adopted village the treasury decides them.
                    Item before = offers.get(0).getResult().getItem();
                    merchant.restock();
                    check("after the restock still two uses, not twelve", left(offers.get(0)) == 2 && offers.get(0).getResult().is(before));
                    merchant.discard();
                    RtsimTests.forget(v);
                })
                .thenSucceed();
    }

    static List<String> names(List<RibbitEntity> list) {
        List<String> out = new ArrayList<>();
        for (RibbitEntity r : list)
            out.add(Ribbits.professionName(r));
        return out;
    }
}
