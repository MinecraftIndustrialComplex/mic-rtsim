package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.iafenvoy.iceandfire.event.DragonFireEvent;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.mojang.logging.LogUtils;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.attachment.AttachmentType;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.EntityMountEvent;
import net.neoforged.neoforge.event.entity.living.AnimalTameEvent;
import net.neoforged.neoforge.event.entity.living.FinalizeSpawnEvent;
import net.neoforged.neoforge.event.entity.living.LivingDamageEvent;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.entity.player.PlayerContainerEvent;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;
import org.slf4j.Logger;

import java.util.function.Supplier;

/**
 * mic_rtsim_iceandfire: Ice and Fire's dragons as the engine's (phase 2, PLAN.md §9; docs/boundary.md
 * §6). The sim owns every dragon (P13's recommendation): a dragon the engine materializes is a puppet the
 * plugin steers and captures; every other dragon that joins a level is reported untagged, and the engine
 * adopts or discards it. Wounds, deaths, meals, lairs and robbed hoards go back as the dragons' records.
 *
 * <p>Every hook is a NeoForge event (no mixin): the reaches into Ice and Fire are in {@link IafDragons}
 * and PATCHES.md. The In Control changes P13 needs are the pack's, applied with field flights on (OGB-6;
 * {@code pack-proposal/} keeps a copy for OG-10's harness).
 */
@Mod(MicRtsimIceAndFire.MODID)
public final class MicRtsimIceAndFire {
    public static final String MODID = "mic_rtsim_iceandfire";
    public static final Logger LOGGER = LogUtils.getLogger();

    private static final DeferredRegister<AttachmentType<?>> ATTACHMENTS = DeferredRegister.create(NeoForgeRegistries.Keys.ATTACHMENT_TYPES, MODID);

    /**
     * {@code mic_rtsim_iceandfire:spawn_type}: the spawn type a dragon's {@code FinalizeSpawnEvent} carried,
     * read when it joins (DRAGON_UNTAGGED's spawn kind). Never saved: a dragon loaded from disk has none.
     */
    public static final Supplier<AttachmentType<MobSpawnType>> SPAWN_TYPE =
            ATTACHMENTS.register("spawn_type", () -> AttachmentType.builder(() -> MobSpawnType.NATURAL).build());

    public MicRtsimIceAndFire(IEventBus modBus, ModContainer container) {
        ATTACHMENTS.register(modBus);
        DragonConfig.register(container);
        RtsimApi.registerPlugin(DragonBridge.INSTANCE);
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(EventPriority.LOWEST, false, FinalizeSpawnEvent.class, MicRtsimIceAndFire::onFinalizeSpawn);
        bus.addListener(EventPriority.LOWEST, false, DragonFireEvent.class, MicRtsimIceAndFire::onFire);
        bus.addListener(EventPriority.LOWEST, false, EntityJoinLevelEvent.class, MicRtsimIceAndFire::onJoin);
        bus.addListener(EventPriority.LOWEST, false, LivingDamageEvent.Post.class, MicRtsimIceAndFire::onDamage);
        bus.addListener(EventPriority.LOWEST, false, LivingDeathEvent.class, MicRtsimIceAndFire::onDeath);
        bus.addListener(EventPriority.LOWEST, false, AnimalTameEvent.class, MicRtsimIceAndFire::onTame);
        bus.addListener(EventPriority.LOWEST, false, EntityMountEvent.class, MicRtsimIceAndFire::onMount);
        bus.addListener(MicRtsimIceAndFire::onChunkLoad);
        bus.addListener(EventPriority.LOWEST, false, BlockEvent.BreakEvent.class, MicRtsimIceAndFire::onBreak);
        bus.addListener(MicRtsimIceAndFire::onContainerOpen);
        bus.addListener(MicRtsimIceAndFire::onContainerClose);
        // GameTest servers only: after the core's session started (its listener has the normal priority).
        bus.addListener(EventPriority.LOWEST, false, net.neoforged.neoforge.event.server.ServerStartedEvent.class,
                com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest.GameTestClock::onServerStarted);
    }

    /** The live session this plugin is part of, or null. */
    private static RtsimSession live() {
        RtsimSession s = RtsimSession.current();
        return s == null || s.failed() || DragonBridge.INSTANCE.session() != s ? null : s;
    }

    /** A dragon about to spawn through NeoForge's finalize hook (spawn eggs, commands, spawners, In Control). */
    private static void onFinalizeSpawn(FinalizeSpawnEvent event) {
        if (event.getEntity() instanceof DragonBaseEntity d && !event.isSpawnCancelled())
            d.setData(SPAWN_TYPE, event.getSpawnType());
    }

    private static void onJoin(EntityJoinLevelEvent event) {
        if (event.getLevel().isClientSide())
            return;
        DragonBaseEntity shooter = IafDragons.chargeOwner(event.getEntity());
        if (shooter != null) {
            if (!event.loadedFromDisk() && live() != null)
                DragonBridge.INSTANCE.onCharge(shooter, event);
            return;
        }
        if (!(event.getEntity() instanceof DragonBaseEntity d) || RtsimAttachments.isTagged(d))
            return;
        // Queued even before the session exists (the spawn chunks load first); reported once it does.
        DragonBridge.INSTANCE.onUntaggedJoin(d);
    }

    /** A dragon's breath of fire (Ice and Fire's own event, posted before the fire does anything). */
    private static void onFire(DragonFireEvent event) {
        DragonBaseEntity d = event.getDragon();
        if (d == null || d.level().isClientSide() || live() == null)
            return;
        DragonBridge.INSTANCE.onFire(d, event);
    }

    private static void onDamage(LivingDamageEvent.Post event) {
        if (event.getEntity().level().isClientSide() || !(event.getEntity() instanceof DragonBaseEntity) || live() == null)
            return;
        DragonBridge.INSTANCE.onDamage(event.getEntity(), event.getSource(), event.getNewDamage());
    }

    private static void onDeath(LivingDeathEvent event) {
        if (event.getEntity().level().isClientSide() || live() == null)
            return;
        DragonBridge.INSTANCE.onDeath(event.getEntity(), event.getSource());
    }

    private static void onTame(AnimalTameEvent event) {
        RtsimSession s = live();
        if (s != null && event.getAnimal() instanceof DragonBaseEntity)
            DragonBridge.INSTANCE.onTame(event.getAnimal(), s.gameTime());
    }

    private static void onMount(EntityMountEvent event) {
        if (event.getLevel().isClientSide() || !event.isMounting() || live() == null)
            return;
        DragonBridge.INSTANCE.onMount(event.getEntityMounting(), event.getEntityBeingMounted());
    }

    private static void onChunkLoad(ChunkEvent.Load event) {
        if (event.getLevel().isClientSide() || !(event.getChunk() instanceof LevelChunk chunk))
            return;
        DragonBridge.INSTANCE.lairs.loaded(chunk, event.isNewChunk()); // drained once the session exists
        DragonBridge.INSTANCE.sampler.loaded(chunk, event.isNewChunk());
    }

    private static void onBreak(BlockEvent.BreakEvent event) {
        RtsimSession s = live();
        if (s == null || !(event.getLevel() instanceof ServerLevel level) || event.getPlayer() == null)
            return;
        DragonBridge.INSTANCE.lairs.onBreak(level, event.getPos(), event.getState(), event.getPlayer(), s.gameTime());
    }

    private static void onContainerOpen(PlayerContainerEvent.Open event) {
        RtsimSession s = live();
        if (s != null && !event.getEntity().level().isClientSide())
            DragonBridge.INSTANCE.lairs.onOpen(event.getEntity(), event.getContainer(), s.gameTime());
    }

    private static void onContainerClose(PlayerContainerEvent.Close event) {
        if (live() != null && !event.getEntity().level().isClientSide())
            DragonBridge.INSTANCE.lairs.onClose(event.getEntity());
    }
}
