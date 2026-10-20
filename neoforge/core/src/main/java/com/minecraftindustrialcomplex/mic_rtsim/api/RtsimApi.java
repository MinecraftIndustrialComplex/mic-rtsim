package com.minecraftindustrialcomplex.mic_rtsim.api;

import com.minecraftindustrialcomplex.mic_rtsim.body.BodyAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.body.GroupKind;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;

/**
 * What plugins ({@code mic_rtsim_<mod>}, PLAN.md §1.3) call. Plugins carry Java adapters and data for
 * one mod and no Rust; this is their whole entry point into the core.
 */
public final class RtsimApi {
    private RtsimApi() {}

    /** Registers a body adapter (also usable as a group of one). Call during mod construction. */
    public static void registerBody(BodyAdapter adapter) {
        BodyKinds.registerBody(adapter);
    }

    /** Registers a group kind (C25). Call during mod construction. */
    public static void registerGroup(GroupKind kind) {
        BodyKinds.registerGroup(kind);
    }

    /** True when the entity is a puppet: tagged with {@code mic_rtsim:agent}. */
    public static boolean isPuppet(Entity entity) {
        return RtsimAttachments.isTagged(entity);
    }

    /**
     * Takes a puppet out of the sim (A10, PLAN.md §6.2): it becomes an ordinary entity, saved by vanilla
     * from now on, and the engine removes the agent's member with a TAKEN observation. Returns false
     * when the entity is not a live puppet.
     *
     * <p><b>The Carry On hook point.</b> {@code mic_rtsim_carryon} listens to Carry On's cancellable
     * {@code tschipp.carryon.events.EntityPickupEvent(ServerPlayer, Entity)} on
     * {@code NeoForge.EVENT_BUS} and calls this with {@link TakeReason#PICKUP} before the pickup goes
     * ahead. Carry On serializes the entity it carries through {@code Entity.save}, which refuses a
     * puppet (EntitySaveMixin), so the take has to come first.
     */
    public static boolean takeFromSim(Entity entity, TakeReason reason, Player player) {
        RtsimSession s = RtsimSession.current();
        return s != null && s.takeEntity(entity, reason.code, player == null ? null : player.getUUID());
    }
}
