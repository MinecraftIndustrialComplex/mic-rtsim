package com.minecraftindustrialcomplex.mic_rtsim.market;

import com.minecraftindustrialcomplex.mic_rtsim.knowledge.Sentiments;
import com.minecraftindustrialcomplex.mic_rtsim.village.Residents;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import net.minecraft.core.component.DataComponentPredicate;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.MerchantMenu;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.trading.ItemCost;
import net.minecraft.world.item.trading.Merchant;
import net.minecraft.world.item.trading.MerchantOffer;
import net.minecraft.world.item.trading.MerchantOffers;

import java.util.HashMap;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;

/**
 * Resident trades through the village treasury (C14 for villagers, C23 for ribbit merchants;
 * PLAN.md §8.5). The sim never adds or removes a resident's offers: it keeps the list vanilla, MoreJS
 * or Ribbits made, reprices the offers that are market-shaped, and sets every such offer's uses from
 * the market's stock (what the village sells) or treasury (what it pays for).
 *
 * <p><b>Market-shaped</b> means one cost and one result, one of them the village's currency and the
 * other a plain good the market trades. Anything else (two costs, enchanted or otherwise
 * component-bearing results) is left exactly as its mod made it.
 *
 * <p><b>The engine's bundles, shown.</b> Every count comes from the MARKET's ladders ({@link MarketBook});
 * the bridge computes no price (Ben's morning review, "Prices"). <b>Reputation is inside the quote</b>
 * (G31, Ben: "Inside the sim quote"): the player's standing picks the MARKET's price level, from
 * vanilla's own reputation with that villager (its gossip, which the sim also writes, C20) or a ribbit
 * village's sentiment; vanilla's {@code updateSpecialPrices} discount is then taken off again on the
 * offers the sim prices ({@link #foldReputation}), so one formula sets the price (choice PR1).
 *
 * <p><b>When.</b> Before a player starts trading (the interaction event), after every trade (the market
 * moved: the counts change and the trading player is sent the new offers), and at every restock (the
 * restock mixins), which only sets uses.
 */
public final class ResidentMarkets {

    private final Map<Long, MarketBook<Item>> books = new HashMap<>();
    /** Residents a player is trading with, per village: a trade at one re-caps the others (one treasury). */
    private final Map<Long, java.util.List<Entity>> trading = new HashMap<>();
    private final Sentiments sentiments;
    long repriced;
    long capped;

    public ResidentMarkets(Sentiments sentiments) {
        this.sentiments = sentiments;
    }

    public MarketBook<Item> book(long village) {
        return books.computeIfAbsent(village, MarketBook::new);
    }

    public MarketBook<Item> known(long village) {
        MarketBook<Item> b = books.get(village);
        return b != null && b.known() ? b : null;
    }

    public int markets() {
        return books.size();
    }

    public long repriced() {
        return repriced;
    }

    /** A market-shaped offer: which good, and which way. */
    public record Shape(Item good, MarketBook.Side side) {}

    /** The offer's shape against a market's currency, or null when it is not market-shaped. */
    public static Shape shape(MerchantOffer o, MarketBook<Item> book) {
        Item currency = book.currency();
        if (currency == null)
            return null;
        if (o.getItemCostB().isPresent() && o.getItemCostB().get().count() > 0)
            return null;
        ItemCost a = o.getItemCostA();
        ItemStack r = o.getResult();
        if (!r.getComponentsPatch().isEmpty() || !a.components().equals(DataComponentPredicate.EMPTY))
            return null;
        Item ai = a.item().value(), ri = r.getItem();
        if (ai == currency && ri != currency && book.trades(ri))
            return new Shape(ri, MarketBook.Side.MARKET_SELLS);
        if (ri == currency && ai != currency && book.trades(ai))
            return new Shape(ai, MarketBook.Side.MARKET_BUYS);
        return null;
    }

    /**
     * The player's standing at this merchant (G31): vanilla's reputation with this villager for a
     * resident with vanilla gossip, else the village's sentiment snapshot (ribbits, PLAN.md §8.7); 0
     * with no player.
     */
    int standing(Entity merchant, Village v, Player player) {
        Residents.Kind kind = Residents.kind(merchant);
        if (player == null || kind == null)
            return 0;
        if (kind.vanillaGossip() && merchant instanceof Villager villager)
            return villager.getPlayerReputation(player);
        Sentiments.Snapshot s = sentiments.get(v.id);
        return s == null ? 0 : s.of(player.getUUID());
    }

    /**
     * Reprices every market-shaped offer of a resident: the engine's bundle at the player's price level
     * and uses from the ladder (MARKET). Offers are replaced in place, one for one, so the list keeps its
     * length and order. Returns false when the village has no MARKET yet.
     */
    public boolean reprice(Entity merchant, Village v, Player player) {
        MarketBook<Item> book = known(v.id);
        if (book == null || !(merchant instanceof Merchant m))
            return false;
        int level = book.levelOf(standing(merchant, v, player));
        MerchantOffers offers = m.getOffers();
        for (int i = 0; i < offers.size(); i++) {
            MerchantOffer o = offers.get(i);
            Shape s = shape(o, book);
            if (s == null)
                continue;
            MarketBook.Offer q = book.offer(s.good(), s.side(), level);
            offers.set(i, rebuilt(o, s, q, book.currency()));
            repriced++;
        }
        if (player != null) {
            java.util.List<Entity> list = trading.computeIfAbsent(v.id, k -> new java.util.ArrayList<>(2));
            if (!list.contains(merchant))
                list.add(merchant);
        }
        foldReputation(merchant, v);
        return true;
    }

    /**
     * G31: vanilla's reputation (and Hero of the Village) price change is folded into the sim's quote,
     * the price level the standing picked, so on an offer the sim prices vanilla's own
     * {@code specialPriceDiff} is taken off again. Called after vanilla's {@code updateSpecialPrices}
     * (the reputation mixin) and after every repricing.
     */
    public void foldReputation(Entity merchant, Village v) {
        MarketBook<Item> book = known(v.id);
        if (book == null || !(merchant instanceof Merchant m))
            return;
        for (MerchantOffer o : m.getOffers()) {
            if (shape(o, book) != null && o.getSpecialPriceDiff() != 0) {
                o.setSpecialPriceDiff(0);
                folded++;
            }
        }
    }

    long folded;

    /**
     * The repriced copy of an offer. Counts must fit one stack; a bundle that does not is clamped only
     * in the direction that is worse for the player, else the offer shows crossed out (choice G4).
     */
    static MerchantOffer rebuilt(MerchantOffer o, Shape s, MarketBook.Offer q, Item currency) {
        int max = o.getMaxUses();
        if (q == null) {
            // Refused (no stock, no money, a glut): the counts stay as they were, and it shows crossed out.
            return new MerchantOffer(o.getItemCostA(), o.getItemCostB(), o.getResult().copy(), max, max, o.getXp(), o.getPriceMultiplier(), 0);
        }
        long items = q == null ? 1 : q.items(), coins = q == null ? 1 : q.coins();
        long left = q == null ? 0 : q.uses();
        int goodStack = s.good().getDefaultMaxStackSize(), coinStack = currency.getDefaultMaxStackSize();
        if (s.side() == MarketBook.Side.MARKET_SELLS) {
            if (items > goodStack)
                items = goodStack;          // fewer goods for the same coin: never better than the quote
            if (coins > coinStack) {
                coins = coinStack;
                left = 0;                   // the price does not fit in one stack: crossed out
            }
        } else {
            if (items > goodStack) {
                items = goodStack;
                left = 0;                   // fewer goods for the same coin would favour the player: crossed out
            }
            if (coins > coinStack)
                coins = coinStack;          // fewer coins for the same goods: never better than the quote
        }
        ItemCost costA;
        ItemStack result;
        if (s.side() == MarketBook.Side.MARKET_SELLS) {
            costA = new ItemCost(currency, (int) coins);
            result = new ItemStack(s.good(), (int) items);
        } else {
            costA = new ItemCost(s.good(), (int) items);
            result = new ItemStack(currency, (int) coins);
        }
        int uses = max - (int) Math.min(max, left);
        // The price is the engine's bundle at the player's level (G31): no vanilla discount on top.
        return new MerchantOffer(costA, Optional.empty(), result, uses, max, o.getXp(), o.getPriceMultiplier(), 0);
    }

    /**
     * Sets one offer's uses from the market without changing its counts (a restock; the counts were
     * set when trading began): the uses the MARKET's ladder has left for exactly these counts. Does
     * nothing for an offer that is not market-shaped.
     */
    public void capUses(Village v, MerchantOffer o) {
        MarketBook<Item> book = known(v.id);
        if (book == null)
            return;
        Shape s = shape(o, book);
        if (s == null)
            return;
        long items = s.side() == MarketBook.Side.MARKET_SELLS ? o.getResult().getCount() : o.getCostA().getCount();
        long coins = s.side() == MarketBook.Side.MARKET_SELLS ? o.getCostA().getCount() : o.getResult().getCount();
        long left = book.usesFor(s.good(), s.side(), items, coins);
        Uses.set(o, o.getMaxUses() - (int) Math.min(o.getMaxUses(), left));
        Uses.noDemand(o);
        capped++;
    }

    /** True when the sim prices this offer of a resident: market-shaped, in a village whose market is known. */
    public boolean prices(Village v, MerchantOffer o) {
        MarketBook<Item> book = known(v.id);
        return book != null && shape(o, book) != null;
    }

    /**
     * A resident's trade was journaled under {@code obsSeq}: the market moves at once (stock and
     * treasury), and the merchant's offers are repriced for the next trade and sent to the player.
     */
    public void traded(Entity merchant, Village v, MerchantOffer offer, Player player, long obsSeq) {
        MarketBook<Item> book = known(v.id);
        if (book == null)
            return;
        Shape s = shape(offer, book);
        if (s != null) {
            ItemStack paid = offer.getCostA();
            int items = s.side() == MarketBook.Side.MARKET_SELLS ? offer.getResult().getCount() : paid.getCount();
            int coins = s.side() == MarketBook.Side.MARKET_SELLS ? paid.getCount() : offer.getResult().getCount();
            book.local(obsSeq, s.good(), s.side(), items, coins);
        } else if (offer.getCostA().is(book.currency()) || offer.getResult().is(book.currency())) {
            // Not market-shaped but paid in or out of the currency (an enchanted book for emeralds):
            // the treasury still moves.
            long in = offer.getCostA().is(book.currency()) ? offer.getCostA().getCount() : 0;
            in += offer.getCostB().is(book.currency()) ? offer.getCostB().getCount() : 0;
            long out = offer.getResult().is(book.currency()) ? offer.getResult().getCount() : 0;
            if (in > 0)
                book.local(obsSeq, null, MarketBook.Side.MARKET_SELLS, 0, in);
            if (out > 0)
                book.local(obsSeq, null, MarketBook.Side.MARKET_BUYS, 0, out);
        }
        reprice(merchant, v, player);
        resend(merchant);
        // The treasury and the stock are the village's: every other screen open on it is re-capped too,
        // or two players at two villagers could each spend the whole treasury.
        java.util.List<Entity> others = trading.get(v.id);
        if (others != null) {
            for (java.util.Iterator<Entity> it = others.iterator(); it.hasNext(); ) {
                Entity other = it.next();
                if (other.isRemoved() || !(other instanceof Merchant om) || om.getTradingPlayer() == null) {
                    it.remove();
                    continue;
                }
                if (other != merchant) {
                    reprice(other, v, om.getTradingPlayer());
                    resend(other);
                }
            }
            if (others.isEmpty())
                trading.remove(v.id);
        }
    }

    /** Sends a merchant's offers to the player trading with it, as vanilla's own resend does. */
    public static void resend(Entity merchant) {
        if (!(merchant instanceof Merchant m))
            return;
        Player p = m.getTradingPlayer();
        if (p instanceof ServerPlayer sp && sp.containerMenu instanceof MerchantMenu menu) {
            int level = merchant instanceof Villager v ? v.getVillagerData().getLevel() : 0;
            sp.sendMerchantOffers(menu.containerId, m.getOffers(), level, m.getVillagerXp(), m.showProgressBar(), m.canRestock());
        }
    }

    /** Who a player is to the market's sentiment, for logs. */
    public static UUID id(Player p) {
        return p == null ? null : p.getUUID();
    }
}
