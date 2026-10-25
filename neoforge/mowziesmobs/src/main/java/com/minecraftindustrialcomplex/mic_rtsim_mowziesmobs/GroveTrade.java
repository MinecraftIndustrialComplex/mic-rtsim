package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.trade.Trade;
import com.bobmowzie.mowziesmobs.server.inventory.ContainerUmvuthanaTrade;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

/**
 * A masked trade with a grove's minion (MZB-417): what the take of a trade's result slot reports to the camp
 * reconciler. The mixin on the jar's result slot calls {@link #taken}; the reconciler writes TRADED only for a body
 * a camp holds or one standing in a camp's footprint, and a band's puppet or a machine writes nothing.
 */
public final class GroveTrade {
    private GroveTrade() {}

    /** Gold nuggets in a stack, ingots and blocks as the engine reckons them (MZB-419: 9 and 81 nuggets). */
    public static int nuggets(ItemStack stack) {
        if (stack.is(Items.GOLD_NUGGET))
            return stack.getCount();
        if (stack.is(Items.GOLD_INGOT))
            return stack.getCount() * 9;
        if (stack.is(Items.GOLD_BLOCK))
            return stack.getCount() * 81;
        return 0;
    }

    /**
     * The gold of one trade (MZB-600): the larger of the gold on its input side and on its output side, so a trade of
     * gold for gold is not counted twice and a trade with no gold side is worth nothing.
     */
    public static int nuggets(Trade trade) {
        return Math.max(nuggets(trade.input()), nuggets(trade.output()));
    }

    /** The result slot of a minion's trade was taken by {@code player}, whose open menu it is (server side). */
    public static void taken(Player player) {
        if (player.level().isClientSide() || !(player.containerMenu instanceof ContainerUmvuthanaTrade menu))
            return;
        EntityUmvuthanaMinion minion = menu.getUmvuthana();
        if (minion == null || !minion.isOfferingTrade())
            return;
        int n = nuggets(minion.getOfferingTrade());
        if (n > 0)
            RtsimApi.traded(minion, player, n);
    }
}
