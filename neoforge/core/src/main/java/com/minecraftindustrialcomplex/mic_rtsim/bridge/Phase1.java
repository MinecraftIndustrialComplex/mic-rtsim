package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.base.BaseTracker;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.build.HouseBuilder;
import com.minecraftindustrialcomplex.mic_rtsim.build.ResidentSpawner;
import com.minecraftindustrialcomplex.mic_rtsim.knowledge.HurtObserver;
import com.minecraftindustrialcomplex.mic_rtsim.knowledge.RescueObserver;
import com.minecraftindustrialcomplex.mic_rtsim.knowledge.Sentiments;
import com.minecraftindustrialcomplex.mic_rtsim.market.CargoWatch;
import com.minecraftindustrialcomplex.mic_rtsim.market.MarketBook;
import com.minecraftindustrialcomplex.mic_rtsim.market.PuppetOffers;
import com.minecraftindustrialcomplex.mic_rtsim.market.ResidentMarkets;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Census;
import com.minecraftindustrialcomplex.mic_rtsim.village.Cultures;
import com.minecraftindustrialcomplex.mic_rtsim.village.GossipWriter;
import com.minecraftindustrialcomplex.mic_rtsim.village.Residents;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption;
import com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.gossip.GossipType;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.trading.Merchant;
import net.minecraft.world.item.trading.MerchantOffer;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

/**
 * Phase 1 of the bridge (PLAN.md §8): villages and their census, trades through the treasury and out of
 * the caravans' cargo, gossip, sentiment, damage, rescues, robbery, player bases, and village growth.
 * One per session, owned by {@link RtsimSession}; server thread only. Each part is its own class; this
 * wires them to the tick, the events and the commands.
 */
public final class Phase1 {
    final RtsimSession session;
    public final VillageRegistry villages;
    public final Sentiments sentiments = new Sentiments();
    public final ResidentMarkets markets;
    public final GossipWriter gossip = new GossipWriter();
    public final Census census;
    public final HurtObserver hurts;
    public final RescueObserver rescues;
    public final CargoWatch cargo;
    public final BaseTracker bases;
    public final HouseBuilder houses;
    public final ResidentSpawner residents;

    /** Set when something may have touched a puppet's cargo (a container closed): check every group next pass. */
    boolean cargoCheckSoon;
    private final List<LivingEntity[]> residentDeaths = new ArrayList<>();
    private final List<Entity> residentKillers = new ArrayList<>();

    long trades;
    long puppetTrades;
    long tradesShort;
    long gossipCommands;
    long marketCommands;
    long offerCommands;

    Phase1(RtsimSession session) {
        this.session = session;
        this.villages = VillageRegistry.get(session.server);
        this.markets = new ResidentMarkets(sentiments);
        this.census = new Census(session, villages, gossip);
        this.hurts = new HurtObserver(session, villages);
        this.rescues = new RescueObserver(session, villages);
        this.cargo = new CargoWatch(session);
        this.bases = BaseTracker.get(session.server);
        this.houses = new HouseBuilder(session, villages);
        this.residents = new ResidentSpawner(session, villages, houses);
        VillageAdoption.refresh(session.server);
    }

    // ---- the tick -------------------------------------------------------------------------------

    void endOfTick(long ticks, long now) {
        rescues.settle(now);
        for (int i = 0; i < residentDeaths.size(); i++) {
            LivingEntity dead = residentDeaths.get(i)[0];
            if (dead.isDeadOrDying() || dead.isRemoved())
                hurts.onResidentDeath(dead, residentKillers.get(i), now);
        }
        residentDeaths.clear();
        residentKillers.clear();
        if (ticks % 20 == 0)
            hurts.flush(now, false);
        cargoCheckSoon = false;
        // The world's own work runs in the server's session only: a second session (the journal
        // GameTests run their own) must not adopt, count or build into its journal.
        if (RtsimSession.current() != session)
            return;
        VillageAdoption.drain(session, villages, now);
        census.tick(ticks, now);
        houses.tick(now);
        residents.tick(now, () -> session.spawnBudget, n -> session.spawnBudget -= n);
        if (ticks % 600 == 0)
            bases.sweep(session, now);
    }

    // ---- trades ---------------------------------------------------------------------------------

    /** A trade happened (MerchantResultSlotMixin, after {@code notifyTrade}). Server side only. */
    public void onTrade(Entity merchant, MerchantOffer offer, Player player) {
        long now = session.gameTime();
        AgentTag tag = RtsimAttachments.tag(merchant);
        PuppetGroup g = tag == null ? null : session.registry.live(tag, merchant.getUUID());
        PuppetGroup.Member member = g == null ? null : g.member(merchant.getUUID());
        Village v = g == null && Residents.kind(merchant) != null ? Residents.villageOf(villages, merchant) : null;
        if (g == null && v == null)
            return; // an ordinary merchant: not the sim's

        int side = Obs.Trade.OTHER;
        String body, profession = null;
        long node;
        if (g != null) {
            PuppetOffers.Spec spec = PuppetOffers.specOf(member, offer);
            if (spec != null)
                side = (spec.flags() & Cmd.MerchantOffers.O_FLAG_SELLS) != 0 ? Obs.Trade.MERCHANT_SELLS
                        : (spec.flags() & Cmd.MerchantOffers.O_FLAG_BUYS) != 0 ? Obs.Trade.MERCHANT_BUYS : Obs.Trade.OTHER;
            body = g.kind.slots().get(member.index).adapter().name();
            Village here = villages.at(merchant.level().dimension(), merchant.blockPosition());
            node = here != null ? here.id : bases.baseAt(session, merchant.level().dimension(), merchant.getX(), merchant.getZ());
        } else {
            MarketBook<net.minecraft.world.item.Item> book = markets.known(v.id);
            ResidentMarkets.Shape s = book == null ? null : ResidentMarkets.shape(offer, book);
            if (s != null)
                side = s.side() == MarketBook.Side.MARKET_SELLS ? Obs.Trade.MERCHANT_SELLS : Obs.Trade.MERCHANT_BUYS;
            body = BuiltInRegistries.ENTITY_TYPE.getKey(merchant.getType()).toString();
            profession = Residents.profession(merchant);
            node = v.id;
        }
        ItemStack a = offer.getCostA(), b = offer.getCostB(), result = offer.getResult();
        int aId = a.isEmpty() ? 0 : session.itemId(a.getItem());
        int bId = b.isEmpty() ? 0 : session.itemId(b.getItem());
        int rId = result.isEmpty() ? 0 : session.itemId(result.getItem());
        int bodyId = session.dictId(Obs.DICT_BODY, body);
        int profId = profession == null ? 0 : session.dictId(Obs.DICT_PROFESSION, profession);

        ByteBuffer r = session.boundary.beginJournaled(Obs.TRADE, Obs.Trade.SIZE, side, now);
        r.putLong(Obs.Trade.NODE, node);
        if (g != null) {
            r.putLong(Obs.Trade.AGENT, g.agent);
            r.putInt(Obs.Trade.EPOCH, g.epoch);
            r.putInt(Obs.Trade.MEMBER, member.index);
        }
        if (player != null) {
            r.putLong(Obs.Trade.PLAYER_MOST, player.getUUID().getMostSignificantBits());
            r.putLong(Obs.Trade.PLAYER_LEAST, player.getUUID().getLeastSignificantBits());
        }
        r.putLong(Obs.Trade.MERCHANT_MOST, merchant.getUUID().getMostSignificantBits());
        r.putLong(Obs.Trade.MERCHANT_LEAST, merchant.getUUID().getLeastSignificantBits());
        r.putInt(Obs.Trade.COST_A_ITEM, aId);
        r.putInt(Obs.Trade.COST_A_COUNT, a.getCount());
        r.putInt(Obs.Trade.COST_B_ITEM, bId);
        r.putInt(Obs.Trade.COST_B_COUNT, b.getCount());
        r.putInt(Obs.Trade.RESULT_ITEM, rId);
        r.putInt(Obs.Trade.RESULT_COUNT, result.getCount());
        r.putInt(Obs.Trade.MERCHANT_BODY, bodyId);
        r.putInt(Obs.Trade.PROFESSION, profId);
        session.boundary.commitWithOpId(r, Obs.Trade.OP_ID);
        long seq = r.getLong(Obs.OBS_SEQ);
        trades++;

        if (g != null) {
            if (!PuppetOffers.applyTrade(g, member, offer)) {
                tradesShort++;
                MicRtsim.LOGGER.warn("[mic_rtsim] agent {} sold {} it no longer carried", g.agent, result);
            }
            CargoWatch.expectAll(g);
            PuppetOffers.recap(g, member);
            ResidentMarkets.resend(merchant);
            puppetTrades++;
        } else {
            markets.traded(merchant, v, offer, player, seq);
        }
    }

    /** A member left a group or its cargo changed: every merchant of the group re-caps its offers' uses. */
    public void recapOffers(PuppetGroup g) {
        for (PuppetGroup.Member m : g.members)
            if (!m.gone && m.offers != null)
                PuppetOffers.recap(g, m);
    }

    /** A player is about to interact with an entity: a resident merchant is repriced for them first. */
    public void beforeTrading(Player player, Entity target) {
        if (!(target instanceof Merchant m) || m.getTradingPlayer() != null || Residents.kind(target) == null
                || RtsimAttachments.isTagged(target))
            return; // a puppet merchant's offers are the engine's (MERCHANT_OFFERS), never the village's
        Village v = Residents.villageOf(villages, target);
        if (v != null)
            markets.reprice(target, v, player);
    }

    /** A restock reset one offer's uses (the restock mixins): the treasury rule decides them instead. */
    public void afterResetUses(Entity merchant, MerchantOffer offer) {
        AgentTag tag = RtsimAttachments.tag(merchant);
        if (tag != null) {
            // A puppet merchant restocked (Ribbits' merchants do, on their own clock): its uses follow its cargo.
            PuppetGroup g = session.registry.live(tag, merchant.getUUID());
            if (g != null)
                PuppetOffers.recap(g, g.member(merchant.getUUID()));
            return;
        }
        if (Residents.kind(merchant) == null)
            return;
        Village v = Residents.villageOf(villages, merchant);
        if (v != null)
            markets.capUses(v, offer);
    }

    /**
     * True when vanilla's demand must not touch this offer (the demand mixins): a puppet merchant's
     * offer (the engine priced it from the news it carries), or a resident's market-shaped offer in an
     * adopted village whose market is known (the market's price is the price).
     */
    public boolean pricesOffer(Entity merchant, MerchantOffer offer) {
        if (RtsimAttachments.isTagged(merchant))
            return true;
        if (Residents.kind(merchant) == null)
            return false;
        Village v = Residents.villageOf(villages, merchant);
        return v != null && markets.prices(v, offer);
    }

    // ---- damage, deaths, rescues ---------------------------------------------------------------

    public void onDamage(LivingEntity victim, Entity attacker, float amount) {
        hurts.onDamage(victim, attacker, amount, session.gameTime());
    }

    public void onDeath(LivingEntity dead, Entity killer) {
        rescues.onDeath(dead, killer);
        if (!RtsimAttachments.isTagged(dead) && Residents.kind(dead) != null) {
            residentDeaths.add(new LivingEntity[] {dead});
            residentKillers.add(killer);
        }
    }

    void observePlayer(ServerPlayer p, long now) {
        bases.sample(session, p, now);
    }

    // ---- commands -------------------------------------------------------------------------------

    void market(long village, long asOf, net.minecraft.world.item.Item currency, long treasury, long treasuryTarget,
                int worstNum, int worstDen, java.util.Map<net.minecraft.world.item.Item, MarketBook.Good> goods) {
        markets.book(village).snapshot(asOf, currency, treasury, treasuryTarget,
                new com.minecraftindustrialcomplex.mic_rtsim.market.Ratio(Math.max(worstNum, 0), Math.max(worstDen, 1)), goods);
        marketCommands++;
    }

    void merchantOffers(long agent, int epoch, int memberIndex, List<PuppetOffers.Spec> specs) {
        PuppetGroup g = session.registry.byAgent(agent);
        if (g == null || g.epoch != epoch || memberIndex < 0 || memberIndex >= g.members.length || g.members[memberIndex].gone)
            return;
        PuppetOffers.install(g, g.members[memberIndex], specs);
        offerCommands++;
    }

    int writeGossip(long cmdSeq, long op, long villageId, UUID player, int wireType, int amount, int report, int flags, long now) {
        gossipCommands++;
        if (session.boundary.journal().commandOpApplied(op))
            return result(cmdSeq, op, Obs.CommandResult.DUPLICATE_OP, now);
        Village v = villages.get(villageId);
        GossipType type = GossipWriter.type(wireType);
        if (v == null || type == null || amount <= 0 || !Cultures.HUMAN.equals(v.culture))
            return result(cmdSeq, op, Obs.CommandResult.FAILED, now);
        for (Village.PendingGossip pg : v.gossip)
            if (pg.op() == op)
                return result(cmdSeq, op, Obs.CommandResult.DUPLICATE_OP, now);
        if ((flags & Cmd.WriteGossip.FLAG_FIRST_HAND) != 0 && GossipWriter.vanillaSeesFirstHand(report))
            return result(cmdSeq, op, Obs.CommandResult.VANILLA_KNOWS, now);
        ServerLevel level = session.server.getLevel(v.dimension);
        if (level != null)
            gossip.apply(level, villages, v, op, player, type, amount, now);
        return result(cmdSeq, op, Obs.CommandResult.APPLIED, now);
    }

    private int result(long cmdSeq, long op, int status, long now) {
        session.journalCommandResult(cmdSeq, op, status, now);
        return status;
    }

    void sentiment(long holder, int dislike, int friend, boolean replace, List<UUID> players, List<Integer> values) {
        Sentiments.Snapshot s = sentiments.update(holder, dislike, friend, replace);
        for (int i = 0; i < players.size(); i++)
            sentiments.put(s, players.get(i), values.get(i));
    }

    void placeHouse(VillageRegistry.Build b, long now) {
        if (session.boundary.journal().commandOpApplied(b.op) || villages.houseOwner(b.op) != null) {
            result(b.cmdSeq, b.op, Obs.CommandResult.DUPLICATE_OP, now);
            return;
        }
        if (houses.building(b.op))
            return; // in progress: the first command's result answers it
        if (villages.get(b.village) == null || session.server.getLevel(b.dimension) == null || b.template == null) {
            result(b.cmdSeq, b.op, Obs.CommandResult.FAILED, now);
            return;
        }
        houses.accept(b);
    }

    void spawnResident(VillageRegistry.ResidentOrder o, long now) {
        if (session.boundary.journal().commandOpApplied(o.op)) {
            result(o.cmdSeq, o.op, Obs.CommandResult.DUPLICATE_OP, now);
            return;
        }
        if (villages.residentOrders.containsKey(o.op))
            return;
        if (villages.get(o.village) == null || session.server.getLevel(o.dimension) == null || o.body == null) {
            result(o.cmdSeq, o.op, Obs.CommandResult.FAILED, now);
            return;
        }
        residents.accept(o);
    }

    // ---- counters -------------------------------------------------------------------------------

    public long trades() { return trades; }
    public long puppetTrades() { return puppetTrades; }
    public long gossipCommands() { return gossipCommands; }

    public String describe() {
        return "villages " + villages.size() + " (adoption queue " + VillageAdoption.queued() + "), markets " + markets.markets()
                + ", trades " + trades + " (" + puppetTrades + " with caravans, " + tradesShort + " short), repriced " + markets.repriced()
                + ", gossip commands " + gossipCommands + " (" + gossip.written() + " writes, " + gossip.skippedHasOp() + " already had it)"
                + ", censuses " + census.taken() + ", hurt reports " + hurts.journaled() + ", rescues " + rescues.journaled()
                + ", cargo reports " + cargo.reports() + ", bases marked " + bases.marks()
                + ", houses " + houses.completed() + " built / " + houses.refused() + " refused, residents " + residents.spawned();
    }
}
