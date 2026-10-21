package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.iafenvoy.iceandfire.entity.DragonChargeEntity;
import net.minecraft.tags.DamageTypeTags;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityMountEvent;
import net.neoforged.neoforge.event.entity.living.LivingIncomingDamageEvent;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

/**
 * GameTest players that take no damage: every blow aimed at one is written down (who, what kind, how
 * much, when) and cancelled, and its fire put out, so a test can watch a dragon fight a player for as
 * long as it likes. The danger probes of OILGATE.md §4.8 read the same log.
 */
public final class Guard {
    private Guard() {}

    /** One blow a guarded entity was spared. */
    public record Blow(long tick, UUID attacker, String attackerType, boolean dragon, boolean fire, String type, float amount) {}

    private static final Map<UUID, List<Blow>> GUARDED = new ConcurrentHashMap<>();
    /**
     * Entities that never ride, guarded now or once: a dragon that carries a GameTest player off makes it
     * a passenger, and a passenger ticks, and Ice and Fire's attachment sync on that tick sends a payload
     * the mock connection never negotiated (the server dies). A probe is held at its site anyway.
     */
    private static final java.util.Set<UUID> GROUNDED = ConcurrentHashMap.newKeySet();
    private static boolean registered;

    /** Guards an entity from now on; its log starts empty. */
    public static void guard(LivingEntity e) {
        if (!registered) {
            NeoForge.EVENT_BUS.addListener(EventPriority.HIGHEST, false, LivingIncomingDamageEvent.class, Guard::onIncoming);
            NeoForge.EVENT_BUS.addListener(EventPriority.HIGHEST, false, EntityMountEvent.class, Guard::onMount);
            registered = true;
        }
        GUARDED.put(e.getUUID(), new ArrayList<>());
        GROUNDED.add(e.getUUID());
    }

    public static void release(LivingEntity e) {
        GUARDED.remove(e.getUUID());
    }

    /** The blows a guarded entity was spared, oldest first (a copy). */
    public static List<Blow> blows(LivingEntity e) {
        List<Blow> b = GUARDED.get(e.getUUID());
        return b == null ? List.of() : List.copyOf(b);
    }

    /** True for fire: a fire damage type, or a dragon's charge. */
    public static boolean isFire(DamageSource s) {
        return s.is(DamageTypeTags.IS_FIRE) || s.getDirectEntity() instanceof DragonChargeEntity
                || s.getMsgId().contains("fire") || s.getMsgId().contains("dragon_fire");
    }

    private static void onMount(EntityMountEvent event) {
        if (event.isMounting() && GROUNDED.contains(event.getEntityMounting().getUUID()))
            event.setCanceled(true);
    }

    private static void onIncoming(LivingIncomingDamageEvent event) {
        LivingEntity e = event.getEntity();
        if (e.level().isClientSide())
            return;
        List<Blow> log = GUARDED.get(e.getUUID());
        if (log == null)
            return;
        DamageSource s = event.getSource();
        Entity by = s.getEntity();
        log.add(new Blow(e.level().getGameTime(), by == null ? null : by.getUUID(),
                by == null ? "" : net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.getKey(by.getType()).toString(),
                by instanceof DragonBaseEntity, isFire(s), s.getMsgId(), event.getAmount()));
        event.setCanceled(true);
        e.clearFire();
    }
}
