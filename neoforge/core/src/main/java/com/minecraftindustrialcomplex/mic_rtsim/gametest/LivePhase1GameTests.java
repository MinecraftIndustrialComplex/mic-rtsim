package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.market.CargoAccess;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.ai.gossip.GossipType;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.minecraft.world.entity.npc.WanderingTrader;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.trading.MerchantOffer;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.ThreadLocalRandom;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * Phase 1 driven by the engine itself (docs/boundary.md §4, §5): nothing here scripts a command. Three
 * villages join the engine's phase 1 world, its markets and flows run, and a caravan the flows put on a
 * road is materialized at a player with the cargo it really carries in its llamas' chests and the offers
 * the engine prices from the news it carries; the player trades with it and robs it; the engine books
 * both once, and the news of the robbery reaches other villages, which the bridge writes into their
 * gossip.
 *
 * <p>The villages sit at a random place far from any loaded chunk, so earlier runs' villages (the
 * GameTest world and the engine's pages are reused, B37) are never their neighbours; the caravan is
 * materialized at the arena with the test order {@code P1_SPAWN_NEAR}, and days pass with
 * {@code P1_ADVANCE}, so the test takes seconds, not in-game days. Native engine only: the stub
 * decides nothing.
 *
 * <p>Every ask is a quarter day of the world's clock ({@code P1_ADVANCE}), and a caravan walks on that
 * clock: a leg is about 4,000 ticks. So the ask that finds the caravan must be the last one: a second
 * ask sent while the spawn is still on its way (the ticks run unthrottled, a step takes several of
 * them) would carry the world past the end of the caravan's trip, its row would go home and be removed
 * while its puppet still stands in the arena, and the robbery of that puppet is refused by the engine
 * (no row to take the goods from): "the engine booked the robbery" failed in three core rounds that way
 * (the two logs kept saw the group after 11 and 12 asks, where 9 is the usual count; MZB-1105).
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class LivePhase1GameTests {
    private LivePhase1GameTests() {}

    private static int pendingRobberies(Village v, UUID player) {
        int n = 0;
        for (Village.PendingGossip g : v.gossip)
            if (g.player().equals(player) && g.type() == GossipType.MAJOR_NEGATIVE)
                n++;
        return n;
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 24000, batch = "mic_rtsim_p1_live")
    public static void aRealCaravanTradesIsRobbedAndTheNewsTravels(GameTestHelper helper) {
        if (!(RtsimTests.engine() instanceof NativeEngine e)) {
            record("liveCaravan", "skipped", "the stub engine decides nothing; this test needs the native engine's phase 1 world");
            helper.succeed();
            return;
        }
        String dim = RtsimTests.dimension(helper);
        Vec3 at = RtsimTests.centre(helper);
        int ox = 2_000_000 + ThreadLocalRandom.current().nextInt(0, 1_000) * 2_000;
        int oz = -2_000_000 - ThreadLocalRandom.current().nextInt(0, 1_000) * 2_000;
        // A farming village, a shearing and smelting one, and a dry one (catalog V5): they trade.
        Village a = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", new BlockPos(ox, 64, oz), 20, 90);
        Village b = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_taiga", new BlockPos(ox + 400, 64, oz), 20, 90);
        Village c = RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_desert", new BlockPos(ox, 64, oz + 400), 20, 90);
        List<Village> villages = List.of(a, b, c);
        ServerPlayer player = FakePlayers.add(helper.getLevel(), at, "p1_robber");
        long agent = RtsimTests.newAgent();
        long[] base = e.phase1Stats();
        long[] ask = {0, 0}; // ticks waited since the last ask, asks
        long[] askedAt = {Long.MIN_VALUE / 2, 0}; // the engine's steps done at the last ask, spawns it had sent then
        long[] seenAt = {0}; // the phase 1 world's tick when the group was first seen
        boolean[] traded = {false};
        String[] robbed = {null};
        List<String> told = new ArrayList<>();

        helper.startSequence()
                .thenWaitUntil(() -> {
                    check("the engine knows the three villages", e.phase1Sentiment(a.id, player.getUUID()) != Integer.MIN_VALUE
                            && e.phase1Sentiment(b.id, player.getUUID()) != Integer.MIN_VALUE
                            && e.phase1Sentiment(c.id, player.getUUID()) != Integer.MIN_VALUE);
                })
                .thenWaitUntil(() -> {
                    if (RtsimTests.group(agent) != null)
                        return;
                    // One ask at a time, counted in the engine's steps, not in ticks (a tick is not a step
                    // under load). The ask's orders are taken by the first step that starts after them, so
                    // two steps done since the ask is an ask answered. If the answer was a spawn, the group
                    // is on its way (applied within a few steps): no new ask until it has had ten steps, or
                    // it would carry the world on past the caravan's trip (the class comment).
                    long[] s = e.phase1Stats();
                    long steps = e.steps();
                    boolean answered = steps >= askedAt[0] + 2;
                    boolean spawning = answered && s[NativeEngine.P1_SPAWNS_SENT] > askedAt[1] && steps < askedAt[0] + 10;
                    if (answered && !spawning) {
                        check("a caravan with wares within 300 asks (75 days of world time)", ask[1] < 300);
                        // A quarter of a day on, then the caravan with wares nearest the villages, here.
                        e.phase1Advance(6_000);
                        e.phase1SpawnNear(agent, 1, dim, at.x, at.y, at.z, 2_000, ox + 150, oz + 150);
                        ask[1]++;
                        askedAt[0] = steps;
                        askedAt[1] = s[NativeEngine.P1_SPAWNS_SENT];
                    }
                    check("the engine materialized a caravan off its flows (asks " + ask[1] + ", world tick "
                            + s[NativeEngine.P1_WORLD_TICK] + ", spawns sent " + s[NativeEngine.P1_SPAWNS_SENT] + ", steps "
                            + e.steps() + ", orders pending " + e.scriptedPending() + ")", false);
                })
                .thenExecute(() -> {
                    PuppetGroup g = RtsimTests.group(agent);
                    seenAt[0] = e.phase1Stats()[NativeEngine.P1_WORLD_TICK];
                    check("a human caravan (C25)", BodyKinds.HUMAN_CARAVAN.equals(g.kind.name()));
                    WanderingTrader trader = (WanderingTrader) RtsimTests.member(g, 0);
                    List<String> cargo = new ArrayList<>();
                    int stacks = 0;
                    for (int i = 1; i <= 2; i++) {
                        CargoAccess ca = BodyKinds.TRADER_LLAMA.ownCargo((TraderLlama) RtsimTests.member(g, i));
                        if (ca != null) {
                            cargo.add("llama " + i + ": " + ca.totals());
                            stacks += ca.totals().size();
                        }
                    }
                    record("liveCaravan", "materialized after asks", ask[1]);
                    record("liveCaravan", "cargo in the chests (C21)", cargo);
                    check("the caravan carries wares in its chests", stacks > 0);
                    List<String> offers = new ArrayList<>();
                    int sell = -1;
                    for (int i = 0; i < trader.getOffers().size(); i++) {
                        MerchantOffer o = trader.getOffers().get(i);
                        offers.add(o.getCostA().getCount() + " " + o.getCostA().getItem() + " -> " + o.getResult().getCount() + " "
                                + o.getResult().getItem() + " x" + (o.getMaxUses() - o.getUses()));
                        if (sell < 0 && !o.getResult().is(Items.EMERALD) && !o.isOutOfStock())
                            sell = i;
                    }
                    record("liveCaravan", "the engine's offers (MERCHANT_OFFERS)", offers);
                    if (sell >= 0) {
                        MerchantOffer o = trader.getOffers().get(sell);
                        ItemStack got = RtsimTests.trade(trader, player, sell, new ItemStack(o.getCostA().getItem(), o.getCostA().getCount()));
                        record("liveCaravan", "bought", got);
                        traded[0] = !got.isEmpty();
                        check("the trade went through", traded[0]);
                    }
                    // The robbery: a few items out of a chest, by something that is not the bridge, with the
                    // player next to the llama (G10 holds it against the nearest player).
                    for (int i = 1; i <= 2 && robbed[0] == null; i++) {
                        TraderLlama llama = (TraderLlama) RtsimTests.member(g, i);
                        for (int k = 0; k < 3 && robbed[0] == null; k++) {
                            ItemStack s = llama.getSlot(500 + k).get();
                            if (!s.isEmpty()) {
                                int n = Math.min(5, s.getCount());
                                robbed[0] = n + " " + s.getItem();
                                s.shrink(n);
                            }
                        }
                    }
                    record("liveCaravan", "robbed", robbed[0]);
                    check("there was something to rob", robbed[0] != null);
                })
                .thenWaitUntil(() -> {
                    long[] s = e.phase1Stats();
                    // The world's clock since the group was seen: a trip is about 4,000 ticks, and past its end the
                    // engine has no row to take the goods from (the class comment); a clock far past that names the cause.
                    check("the engine booked the robbery (the world's clock ran " + (s[NativeEngine.P1_WORLD_TICK] - seenAt[0])
                            + " ticks since the group was seen, after " + ask[1] + " asks)",
                            s[NativeEngine.P1_ROBBERIES] > base[NativeEngine.P1_ROBBERIES]);
                    check("and the trade", !traded[0] || s[NativeEngine.P1_CARAVAN_TRADES] > base[NativeEngine.P1_CARAVAN_TRADES]);
                })
                .thenWaitUntil(() -> {
                    told.clear();
                    for (Village v : villages)
                        if (pendingRobberies(v, player.getUUID()) > 0)
                            told.add(Long.toHexString(v.id));
                    if (told.size() >= 2)
                        return;
                    if (ask[0]++ % 10 == 0) {
                        check("the news spread within 2,000 asks in all", ask[1] < 2_000);
                        e.phase1Advance(6_000);
                        ask[1]++;
                    }
                    check("the robbery is news in two villages' gossip (C20)", false);
                })
                .thenExecute(() -> {
                    long[] s = e.phase1Stats();
                    List<String> feel = new ArrayList<>();
                    for (Village v : villages)
                        feel.add(Long.toHexString(v.id) + "=" + e.phase1Sentiment(v.id, player.getUUID()));
                    record("liveCaravan", "villages whose gossip holds the robbery", told + " after " + ask[1] + " asks");
                    record("liveCaravan", "their sentiment toward the robber now", feel);
                    record("liveCaravan", "engine phase 1 stats (trades, robberies, gossip sent)",
                            (s[NativeEngine.P1_CARAVAN_TRADES] - base[NativeEngine.P1_CARAVAN_TRADES]) + ", "
                                    + (s[NativeEngine.P1_ROBBERIES] - base[NativeEngine.P1_ROBBERIES]) + ", "
                                    + (s[NativeEngine.P1_GOSSIP_SENT] - base[NativeEngine.P1_GOSSIP_SENT]));
                    e.despawnGroup(agent, 1);
                })
                .thenWaitUntil(() -> check("the caravan despawned", RtsimTests.group(agent) == null))
                .thenExecute(() -> {
                    FakePlayers.remove(player);
                    villages.forEach(RtsimTests::forget);
                })
                .thenSucceed();
    }
}
