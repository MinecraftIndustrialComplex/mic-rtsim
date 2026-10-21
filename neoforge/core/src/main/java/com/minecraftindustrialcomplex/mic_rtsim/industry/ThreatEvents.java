package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.player.Player;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.FinalizeSpawnEvent;
import net.neoforged.neoforge.event.entity.living.LivingChangeTargetEvent;
import net.neoforged.neoforge.event.entity.living.LivingDamageEvent;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.level.ExplosionEvent;

/**
 * The NeoForge hooks of the threat sampler (PATCHES.md): natural spawns, targeting, damage, deaths and
 * explosions near districts, and player edits that make a survey stale. Thin: {@link Threats} does the
 * work, only while the industry module runs.
 */
final class ThreatEvents {
    private ThreatEvents() {}

    static void register() {
        IEventBus bus = NeoForge.EVENT_BUS;
        // Receives cancelled events: a cancelled FinalizeSpawnEvent still spawns the mob (NeoForge's
        // javadoc); only setSpawnCancelled stops it, which is checked.
        bus.addListener(EventPriority.LOWEST, true, FinalizeSpawnEvent.class, ThreatEvents::onFinalizeSpawn);
        bus.addListener(EventPriority.LOWEST, false, LivingChangeTargetEvent.class, ThreatEvents::onTarget);
        bus.addListener(EventPriority.LOWEST, false, LivingDamageEvent.Post.class, ThreatEvents::onDamage);
        bus.addListener(EventPriority.LOWEST, false, LivingDeathEvent.class, ThreatEvents::onDeath);
        bus.addListener(EventPriority.LOWEST, false, ExplosionEvent.Detonate.class, ThreatEvents::onExplosion);
        bus.addListener(EventPriority.LOWEST, false, BlockEvent.EntityPlaceEvent.class, ThreatEvents::onPlace);
        bus.addListener(EventPriority.LOWEST, false, BlockEvent.BreakEvent.class, ThreatEvents::onBreak);
    }

    private static Threats threats() {
        Industry ind = IndustryApi.current();
        return ind == null ? null : ind.threats;
    }

    private static void onFinalizeSpawn(FinalizeSpawnEvent event) {
        if (event.getSpawnType() != MobSpawnType.NATURAL || event.isSpawnCancelled())
            return;
        Mob mob = event.getEntity();
        if (!(event.getLevel().getLevel() instanceof ServerLevel level) || !ThreatClasses.hostile(mob))
            return;
        Threats t = threats();
        if (t != null)
            t.naturalSpawn(level, mob, BlockPos.containing(event.getX(), event.getY(), event.getZ()));
    }

    private static void onTarget(LivingChangeTargetEvent event) {
        var target = event.getNewAboutToBeSetTarget();
        if (target == null || event.getEntity().level().isClientSide())
            return;
        if (!(target instanceof Player) && !ThreatClasses.guard(target))
            return;
        Threats t = threats();
        if (t != null)
            t.engage(event.getEntity());
    }

    private static void onDamage(LivingDamageEvent.Post event) {
        if (event.getEntity().level().isClientSide() || event.getNewDamage() <= 0)
            return;
        Threats t = threats();
        if (t != null)
            t.damaged(event.getEntity(), event.getSource().getEntity());
    }

    private static void onDeath(LivingDeathEvent event) {
        if (event.getEntity().level().isClientSide())
            return;
        Threats t = threats();
        if (t != null)
            t.died(event.getEntity(), event.getSource().getEntity());
    }

    private static void onExplosion(ExplosionEvent.Detonate event) {
        if (!(event.getLevel() instanceof ServerLevel level))
            return;
        Entity source = event.getExplosion().getDirectSourceEntity();
        if (source == null)
            source = event.getExplosion().getIndirectSourceEntity();
        Threats t = threats();
        if (t != null)
            t.exploded(level, source, event.getAffectedBlocks());
    }

    private static void onPlace(BlockEvent.EntityPlaceEvent event) {
        if (!(event.getEntity() instanceof Player player) || player.isFakePlayer() || !(event.getLevel() instanceof ServerLevel level))
            return; // a machine's placement (a Create deployer) changes no defences worth a survey
        Threats t = threats();
        if (t != null)
            t.edited(level, event.getPos());
    }

    private static void onBreak(BlockEvent.BreakEvent event) {
        if (event.getPlayer() == null || event.getPlayer().isFakePlayer() || !(event.getLevel() instanceof ServerLevel level))
            return;
        Threats t = threats();
        if (t != null)
            t.edited(level, event.getPos());
    }
}
