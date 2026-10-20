package com.minecraftindustrialcomplex.mic_rtsim.herd;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.world.item.ShearsItem;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.entity.living.LivingDropsEvent;
import net.neoforged.neoforge.event.entity.player.PlayerInteractEvent;

/**
 * The NeoForge hooks of the vanilla herd adapter (PATCHES.md): deaths, their drops, and shears used on a
 * member. Takings need no hook of their own: a lead or a tamed horse is seen by the pass, taming and Carry
 * On's pickup reach {@link HerdBridge#take} through the core ({@code RtsimSession.takeEntity}).
 */
public final class HerdEvents {
    private HerdEvents() {}

    public static void register() {
        RtsimApi.registerPlugin(HerdBridge.INSTANCE);
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(EventPriority.LOWEST, false, LivingDeathEvent.class, HerdEvents::onDeath);
        bus.addListener(EventPriority.LOWEST, false, LivingDropsEvent.class, HerdEvents::onDrops);
        bus.addListener(EventPriority.LOWEST, false, PlayerInteractEvent.EntityInteract.class, HerdEvents::onInteract);
    }

    /** The live session the herds are part of, or null. */
    private static RtsimSession live() {
        RtsimSession s = RtsimSession.current();
        return s == null || s.failed() || HerdBridge.INSTANCE.session() != s ? null : s;
    }

    private static void onDeath(LivingDeathEvent event) {
        if (event.getEntity().level().isClientSide() || live() == null)
            return;
        HerdBridge.INSTANCE.onDeath(event.getEntity(), event.getSource());
    }

    private static void onDrops(LivingDropsEvent event) {
        if (event.getEntity().level().isClientSide() || live() == null)
            return;
        HerdBridge.INSTANCE.onDrops(event.getEntity(), event.getDrops());
    }

    private static void onInteract(PlayerInteractEvent.EntityInteract event) {
        if (event.getLevel().isClientSide() || live() == null || !(event.getItemStack().getItem() instanceof ShearsItem))
            return;
        HerdBridge.INSTANCE.onShearsUsed(event.getTarget(), event.getEntity());
    }
}
