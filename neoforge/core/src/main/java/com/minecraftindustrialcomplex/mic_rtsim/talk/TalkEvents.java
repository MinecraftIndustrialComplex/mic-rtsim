package com.minecraftindustrialcomplex.mic_rtsim.talk;

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
        Talkers.registerBuiltin(ResourceLocation.withDefaultNamespace("villager"), new VillagerTalker(), true);
        Talkers.registerBuiltin(ResourceLocation.withDefaultNamespace("wandering_trader"), new TraderTalker(), true);
    }

    /** Who talked in which tick: their off-hand item use in that tick is cancelled too. */
    private static final OffHandGuard GUARD = new OffHandGuard();

    public static void register() {
        registerBuiltins();
        com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi.registerPlugin(TalkAsks.INSTANCE);
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(EventPriority.HIGHEST, false, PlayerInteractEvent.EntityInteract.class, TalkEvents::onInteract);
        bus.addListener(EventPriority.HIGHEST, false, PlayerInteractEvent.RightClickItem.class, TalkEvents::onUseItem);
        bus.addListener(net.neoforged.neoforge.event.tick.ServerTickEvent.Post.class, e -> Talk.tickLooks());
        bus.addListener(PlayerEvent.PlayerLoggedOutEvent.class, e -> Talk.forget(e.getEntity().getUUID()));
        bus.addListener(ServerStartedEvent.class, e -> clear());
        bus.addListener(ServerStoppedEvent.class, e -> clear());
    }

    /** Mowzie's Mobs' freeze and Cataclysm's stun cancel interactions at NORMAL priority: a held player does not talk. */
    private static final String[] HELD = {"mowziesmobs:frozen", "cataclysm:stun"};
    private static volatile java.util.List<net.minecraft.core.Holder<net.minecraft.world.effect.MobEffect>> heldEffects;

    private static boolean held(Player player) {
        java.util.List<net.minecraft.core.Holder<net.minecraft.world.effect.MobEffect>> effects = heldEffects;
        if (effects == null) {
            java.util.List<net.minecraft.core.Holder<net.minecraft.world.effect.MobEffect>> found = new java.util.ArrayList<>();
            for (String id : HELD) {
                ResourceLocation rl = ResourceLocation.parse(id);
                net.minecraft.core.registries.BuiltInRegistries.MOB_EFFECT.getHolder(rl).ifPresent(found::add);
            }
            heldEffects = effects = java.util.List.copyOf(found);
        }
        for (net.minecraft.core.Holder<net.minecraft.world.effect.MobEffect> e : effects)
            if (player.hasEffect(e))
                return true;
        return false;
    }

    private static void onInteract(PlayerInteractEvent.EntityInteract event) {
        Player player = event.getEntity();
        Entity target = event.getTarget();
        if (!player.isSecondaryUseActive() || !player.getMainHandItem().isEmpty() || !target.isAlive() || target.isRemoved())
            return;
        if (held(player))
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
        // Machines (a Create deployer is a fake player) never talk.
        if (!(player instanceof ServerPlayer sp) || sp.isFakePlayer())
            return;
        Talker talker = Talkers.find(target, sp);
        if (talker == null)
            return;
        event.setCanceled(true);
        GUARD.talked(sp.getUUID(), sp.serverLevel().getGameTime());
        if (!main) {
            event.setCancellationResult(InteractionResult.CONSUME);
            return;
        }
        event.setCancellationResult(InteractionResult.SUCCESS);
        try {
            Talk.speak(talker, target, sp);
        } catch (Throwable e) {
            Talk.failed(talker, "speak", e);
        }
    }

    /**
     * When the type does not always talk, the client does not cancel: after the two interact packets it
     * sends a separate use of the off-hand item. That use is cancelled for a player who just talked, so
     * a pearl, food or a bow in the off hand does not fire with the talk.
     */
    private static void onUseItem(PlayerInteractEvent.RightClickItem event) {
        if (event.getLevel().isClientSide() || event.getHand() != InteractionHand.OFF_HAND || !(event.getEntity() instanceof ServerPlayer sp))
            return;
        if (GUARD.justTalked(sp.getUUID(), sp.serverLevel().getGameTime())) {
            event.setCanceled(true);
            event.setCancellationResult(InteractionResult.CONSUME);
        }
    }

    private static void clear() {
        GUARD.clear();
        TalkAsks.INSTANCE.reset();
        Talk.clear();
    }
}
