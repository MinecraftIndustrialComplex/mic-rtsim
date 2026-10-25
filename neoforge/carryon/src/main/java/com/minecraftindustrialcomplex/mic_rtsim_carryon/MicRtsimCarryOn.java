package com.minecraftindustrialcomplex.mic_rtsim_carryon;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.TakeReason;
import com.mojang.logging.LogUtils;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.common.NeoForge;
import org.slf4j.Logger;
import tschipp.carryon.events.EntityPickupEvent;

/**
 * mic_rtsim_carryon: a creature a player picks up with Carry On leaves the sim and becomes an ordinary
 * entity (A10, PLAN.md §6.2).
 *
 * <p><b>The hook</b> (verified in the Carry On 2.2.6.13 jar): {@code CommonEvents.onEntityRightClick}
 * passes {@code PickupHandler.tryPickupEntity} a function that posts the cancellable
 * {@code tschipp.carryon.events.EntityPickupEvent(ServerPlayer, Entity)} on {@code NeoForge.EVENT_BUS}
 * and goes ahead only if it was not cancelled. That post comes before Carry On saves the entity
 * ({@code CarryOnData.setEntity} calls {@code Entity.save}) and removes it. Carry On's own blacklist
 * is per entity type, so it cannot tell a puppet from an ordinary trader llama; the event can.
 *
 * <p>The listener runs at the lowest priority and only for an event nobody cancelled, then takes the
 * puppet out of the sim ({@code RtsimApi.takeFromSim}, TAKEN with reason PICKUP): its tag goes, so
 * {@code Entity.save} works on it again, and the rest of its group stays one unit (C25). A listener of
 * the same lowest priority that cancels after this one would leave an ordinary entity on the ground;
 * the creature is out of the sim either way (PATCHES.md). A body a camp holds (MOWZIE.md §4.9) leaves its
 * camp the same way ({@code RtsimApi.takeCampBody}: CAMP_DEATH with killer kind taken) and is put down a stray.
 */
@Mod(MicRtsimCarryOn.MODID)
public final class MicRtsimCarryOn {
    public static final String MODID = "mic_rtsim_carryon";
    public static final Logger LOGGER = LogUtils.getLogger();

    public MicRtsimCarryOn(IEventBus modBus) {
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, false, EntityPickupEvent.class, MicRtsimCarryOn::onPickup);
    }

    private static void onPickup(EntityPickupEvent event) {
        if (event.target == null || event.target.level().isClientSide())
            return;
        if (RtsimApi.isPuppet(event.target) && RtsimApi.takeFromSim(event.target, TakeReason.PICKUP, event.player))
            LOGGER.debug("[mic_rtsim_carryon] {} picked up {}: it left the sim", event.player.getName().getString(), event.target);
        else if (RtsimApi.isCampBody(event.target) && RtsimApi.takeCampBody(event.target, event.player))
            LOGGER.debug("[mic_rtsim_carryon] {} picked up {}: it left its camp", event.player.getName().getString(), event.target);
    }
}
