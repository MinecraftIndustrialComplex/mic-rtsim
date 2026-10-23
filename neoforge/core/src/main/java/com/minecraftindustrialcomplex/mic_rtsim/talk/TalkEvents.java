package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.player.PlayerEvent;
import net.neoforged.neoforge.event.entity.player.PlayerInteractEvent;
import net.neoforged.neoforge.event.server.ServerStartedEvent;
import net.neoforged.neoforge.event.server.ServerStoppedEvent;

/**
 * The gesture (TALK.md §2, TK7): crouch and right-click a talker with an empty main hand. Cancels at
 * HIGHEST on both sides so no screen opens and no mod's {@code mobInteract} runs; the main hand speaks
 * (server only), the off hand is cancelled silently.
 */
public final class TalkEvents {
    private TalkEvents() {}

    /** The talkers the core brings itself (vanilla's villager and wandering trader). */
    public static void registerBuiltins() {
        RtsimApi.registerTalker(ResourceLocation.withDefaultNamespace("villager"), new VillagerTalker(), true);
        RtsimApi.registerTalker(ResourceLocation.withDefaultNamespace("wandering_trader"), new TraderTalker(), true);
    }

    public static void register() {
        registerBuiltins();
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(EventPriority.HIGHEST, false, PlayerInteractEvent.EntityInteract.class, TalkEvents::onInteract);
        bus.addListener(PlayerEvent.PlayerLoggedOutEvent.class, e -> Talk.forget(e.getEntity().getUUID()));
        bus.addListener(ServerStartedEvent.class, e -> Talk.clear());
        bus.addListener(ServerStoppedEvent.class, e -> Talk.clear());
    }

    private static void onInteract(PlayerInteractEvent.EntityInteract event) {
        Player player = event.getEntity();
        Entity target = event.getTarget();
        if (!player.isSecondaryUseActive() || !player.getMainHandItem().isEmpty() || !target.isAlive() || target.isRemoved())
            return;
        boolean main = event.getHand() == InteractionHand.MAIN_HAND;
        if (event.getLevel().isClientSide()) {
            // Only types that always talk: the client cannot know the rest. Never speaks.
            if (Talkers.alwaysTalks(target.getType())) {
                event.setCanceled(true);
                event.setCancellationResult(main ? InteractionResult.SUCCESS : InteractionResult.CONSUME);
            }
            return;
        }
        if (!(player instanceof ServerPlayer sp))
            return;
        Talker talker = Talkers.find(target, sp);
        if (talker == null)
            return;
        event.setCanceled(true);
        if (!main) {
            event.setCancellationResult(InteractionResult.CONSUME);
            return;
        }
        event.setCancellationResult(InteractionResult.SUCCESS);
        Talk.speak(talker, target, sp);
    }
}
