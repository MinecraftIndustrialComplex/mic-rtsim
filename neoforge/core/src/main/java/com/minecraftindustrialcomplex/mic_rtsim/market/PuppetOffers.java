package com.minecraftindustrialcomplex.mic_rtsim.market;

import com.minecraftindustrialcomplex.mic_rtsim.body.BodyAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.trading.ItemCost;
import net.minecraft.world.item.trading.Merchant;
import net.minecraft.world.item.trading.MerchantOffer;
import net.minecraft.world.item.trading.MerchantOffers;

import java.util.List;
import java.util.Optional;

/**
 * A puppet merchant's offers (MERCHANT_OFFERS, PLAN.md §8.5): set by the engine from the news its
 * caravan carries, and capped by its cargo (C21). Every result comes out of the cargo and every payment
 * goes into it, so an offer can be taken only while its result is there and its payment fits.
 *
 * <p><b>{@code overrideOffers} is not the hook.</b> On the server, {@code AbstractVillager.overrideOffers}
 * and {@code RibbitEntity.overrideOffers} are both empty (checked in the 1.21.1 source and the Ribbits
 * 4.1.6 bytecode; it exists for the client's copy). The bridge edits the list {@code getOffers()}
 * returns, which is the merchant's own (choice G3).
 */
public final class PuppetOffers {
    private PuppetOffers() {}

    /** One offer as the engine sent it. */
    public record Spec(Item costA, int costACount, Item costB, int costBCount, Item result, int resultCount,
                       int maxUses, int xp, float priceMultiplier, int flags) {}

    /** maxUses when the engine leaves it at 0 (PLACEHOLDER). */
    public static final int DEFAULT_MAX_USES = 64;

    /** Replaces the member's offers with the engine's, capped by its cargo. */
    public static void install(PuppetGroup group, PuppetGroup.Member member, List<Spec> specs) {
        member.offers = List.copyOf(specs);
        if (!(member.entity instanceof Merchant merchant))
            return;
        MerchantOffers offers = merchant.getOffers();
        offers.clear();
        for (Spec s : specs) {
            int max = s.maxUses() > 0 ? s.maxUses() : DEFAULT_MAX_USES;
            Optional<ItemCost> b = s.costB() == null || s.costBCount() <= 0 ? Optional.empty() : Optional.of(new ItemCost(s.costB(), s.costBCount()));
            offers.add(new MerchantOffer(new ItemCost(s.costA(), s.costACount()), b, new ItemStack(s.result(), s.resultCount()),
                    0, max, s.xp(), s.priceMultiplier()));
        }
        recap(group, member);
    }

    /** The cargo a member trades from, or null. */
    public static CargoAccess cargo(PuppetGroup group, PuppetGroup.Member member) {
        if (member.entity == null)
            return null;
        BodyAdapter a = group.kind.slots().get(member.index).adapter();
        return a.tradeCargo(member.entity, group);
    }

    /** Sets each offer's uses from what the cargo holds and has room for. */
    public static void recap(PuppetGroup group, PuppetGroup.Member member) {
        if (member.offers == null || !(member.entity instanceof Merchant merchant))
            return;
        MerchantOffers offers = merchant.getOffers();
        CargoAccess cargo = cargo(group, member);
        for (int i = 0; i < offers.size() && i < member.offers.size(); i++) {
            Spec s = member.offers.get(i);
            MerchantOffer o = offers.get(i);
            int left = remaining(cargo, s);
            Uses.set(o, o.getMaxUses() - Math.min(o.getMaxUses(), left));
        }
    }

    /** How many more times an offer can be taken: its result in the cargo, its payment's room. */
    static int remaining(CargoAccess cargo, Spec s) {
        if (cargo == null || s.resultCount() <= 0)
            return 0;
        long left = cargo.count(s.result()) / s.resultCount();
        if (s.costACount() > 0 && s.costA() != s.result())
            left = Math.min(left, cargo.room(s.costA()) / s.costACount());
        if (s.costB() != null && s.costBCount() > 0 && s.costB() != s.result())
            left = Math.min(left, cargo.room(s.costB()) / s.costBCount());
        return (int) Math.min(left, Integer.MAX_VALUE);
    }

    /**
     * A trade happened: the result leaves the cargo and the payment goes in. Returns false when the
     * cargo did not hold what was sold (a race with a robbery); the trade still stands and is reported.
     */
    public static boolean applyTrade(PuppetGroup group, PuppetGroup.Member member, MerchantOffer offer) {
        CargoAccess cargo = cargo(group, member);
        if (cargo == null)
            return false;
        ItemStack result = offer.getResult();
        ItemStack a = offer.getCostA();
        ItemStack b = offer.getCostB();
        int took = cargo.take(result.getItem(), result.getCount());
        if (!a.isEmpty())
            cargo.put(a.getItem(), a.getCount());
        if (!b.isEmpty())
            cargo.put(b.getItem(), b.getCount());
        return took == result.getCount();
    }

    /** The spec behind an offer object of this member, or null. */
    public static Spec specOf(PuppetGroup.Member member, MerchantOffer offer) {
        if (member.offers == null || !(member.entity instanceof Merchant merchant))
            return null;
        int i = merchant.getOffers().indexOf(offer);
        return i >= 0 && i < member.offers.size() ? member.offers.get(i) : null;
    }

    static boolean is(Entity e) {
        return e instanceof Merchant;
    }
}
