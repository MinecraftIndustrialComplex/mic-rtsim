package com.minecraftindustrialcomplex.mic_rtsim.bridge;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.command.RtsimCommands;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.event.config.ModConfigEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.RegisterCommandsEvent;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.EntityLeaveLevelEvent;
import net.neoforged.neoforge.event.entity.living.AnimalTameEvent;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.entity.player.PlayerInteractEvent;
import net.neoforged.neoforge.event.level.LevelEvent;
import net.neoforged.neoforge.event.server.ServerStartedEvent;
import net.neoforged.neoforge.event.server.ServerStoppedEvent;
import net.neoforged.neoforge.event.server.ServerStoppingEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;

/**
 * Every NeoForge hook the core bridge uses (all listed in PATCHES.md when it exists, P7). The handlers
 * are thin: the session does the work.
 */
public final class BridgeEvents {
    private BridgeEvents() {}

    public static void register(IEventBus modBus) {
        modBus.addListener(BridgeEvents::onConfigReload);
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(BridgeEvents::onServerAboutToStart);
        bus.addListener(BridgeEvents::onServerStarted);
        // GameTest servers only: after the session started (the listener above has the normal priority).
        bus.addListener(EventPriority.LOWEST, false, ServerStartedEvent.class,
                com.minecraftindustrialcomplex.mic_rtsim.gametest.GameTestClock::onServerStarted);
        bus.addListener(BridgeEvents::onServerStopping);
        bus.addListener(BridgeEvents::onServerStopped);
        bus.addListener(BridgeEvents::onServerTick);
        bus.addListener(BridgeEvents::onLevelSave);
        bus.addListener(EventPriority.HIGHEST, BridgeEvents::onJoin);
        bus.addListener(BridgeEvents::onLeave);
        bus.addListener(EventPriority.LOWEST, false, LivingDeathEvent.class, BridgeEvents::onDeath);
        bus.addListener(EventPriority.LOWEST, false, AnimalTameEvent.class, BridgeEvents::onTame);
        bus.addListener(EventPriority.LOWEST, false, net.neoforged.neoforge.event.entity.living.LivingDropsEvent.class, BridgeEvents::onDrops);
        bus.addListener(BridgeEvents::onChunkLoad);
        bus.addListener(com.minecraftindustrialcomplex.mic_rtsim.village.WorkCatalog::onAddReloadListeners);
        bus.addListener(EventPriority.LOWEST, false, net.neoforged.neoforge.event.level.ChunkEvent.Unload.class, BridgeEvents::onChunkUnload);
        bus.addListener(BridgeEvents::onEntityInteract);
        bus.addListener(BridgeEvents::onRegisterCommands);
        // Phase 1 (PLAN.md §8)
        bus.addListener(EventPriority.LOWEST, false, net.neoforged.neoforge.event.entity.living.LivingDamageEvent.Post.class, BridgeEvents::onDamage);
        bus.addListener(EventPriority.LOWEST, false, net.neoforged.neoforge.event.level.BlockEvent.EntityPlaceEvent.class, BridgeEvents::onPlace);
        bus.addListener(BridgeEvents::onTagsUpdated);
        bus.addListener(BridgeEvents::onContainerClose);
    }

    /** Damage really dealt to a puppet or a resident (HURT, PLAN.md §8.7). */
    private static void onDamage(net.neoforged.neoforge.event.entity.living.LivingDamageEvent.Post event) {
        if (event.getEntity().level().isClientSide())
            return;
        RtsimSession s = RtsimSession.current();
        if (s != null && !s.failed())
            s.phase1.onDamage(event.getEntity(), event.getSource().getEntity(), event.getNewDamage());
    }

    /** A block a player placed: village growth never builds over it (PLAN.md §8.8). */
    private static void onPlace(net.neoforged.neoforge.event.level.BlockEvent.EntityPlaceEvent event) {
        if (!(event.getEntity() instanceof net.minecraft.world.entity.player.Player) || !(event.getLevel() instanceof ServerLevel level))
            return;
        if (event instanceof net.neoforged.neoforge.event.level.BlockEvent.EntityMultiPlaceEvent multi) {
            for (net.neoforged.neoforge.common.util.BlockSnapshot snap : multi.getReplacedBlockSnapshots())
                mark(level, snap.getPos());
        } else {
            mark(level, event.getPos());
        }
    }

    private static void mark(ServerLevel level, net.minecraft.core.BlockPos pos) {
        net.minecraft.world.level.chunk.LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        if (chunk != null)
            com.minecraftindustrialcomplex.mic_rtsim.growth.PlayerBlocks.mark(chunk, pos);
    }

    /** Datapacks reloaded: which structures are villages may have changed. */
    private static void onTagsUpdated(net.neoforged.neoforge.event.TagsUpdatedEvent event) {
        if (event.getUpdateCause() == net.neoforged.neoforge.event.TagsUpdatedEvent.UpdateCause.SERVER_DATA_LOAD)
            com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption.refresh(event.getRegistryAccess());
    }

    /** A horse-like inventory closed: something may have taken a puppet's cargo; check every group next pass. */
    private static void onContainerClose(net.neoforged.neoforge.event.entity.player.PlayerContainerEvent.Close event) {
        if (event.getEntity().level().isClientSide() || !(event.getContainer() instanceof net.minecraft.world.inventory.HorseInventoryMenu))
            return;
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.phase1.cargoCheckSoon = true;
    }

    /**
     * Before the server prepares the spawn area: which structures are villages must be known by then, or
     * the village starts generated (and the spawn chunks loaded) during preparation are missed (C22).
     */
    private static void onServerAboutToStart(net.neoforged.neoforge.event.server.ServerAboutToStartEvent event) {
        com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption.refresh(event.getServer());
    }

    private static void onServerStarted(ServerStartedEvent event) {
        MinecraftServer server = event.getServer();
        TraderSpawning.announce(true);
        RtsimSession.startGlobal(server);
    }

    private static void onServerStopping(ServerStoppingEvent event) {
        RtsimSession.stoppingGlobal();
    }

    private static void onServerStopped(ServerStoppedEvent event) {
        RtsimSession.stopGlobal();
        com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption.clear();
    }

    /**
     * Holders of pending records that loaded, waiting for the server thread (ChunkEvent.Load may come
     * early in a chunk's promotion) or for the session (spawn chunks load before it exists).
     */
    private static final java.util.concurrent.ConcurrentLinkedQueue<net.minecraft.world.level.chunk.LevelChunk> LOADED_CHUNKS = new java.util.concurrent.ConcurrentLinkedQueue<>();
    private static final java.util.concurrent.ConcurrentLinkedQueue<Entity> LOADED_ENTITIES = new java.util.concurrent.ConcurrentLinkedQueue<>();

    private static void onServerTick(ServerTickEvent.Post event) {
        long t0 = System.nanoTime();
        RtsimSession s = RtsimSession.current();
        boolean live = s != null && !s.failed();
        for (net.minecraft.world.level.chunk.LevelChunk c; (c = LOADED_CHUNKS.poll()) != null; )
            if (live)
                s.reassert(c);
        for (Entity e; (e = LOADED_ENTITIES.poll()) != null; )
            if (live && !e.isRemoved())
                s.reassert(e);
        if (s != null)
            s.endOfTick();
        TickCost.post(System.nanoTime() - t0, event.getServer().overworld().getGameTime());
    }

    private static void onChunkLoad(net.neoforged.neoforge.event.level.ChunkEvent.Load event) {
        if (event.getLevel().isClientSide() || !(event.getChunk() instanceof net.minecraft.world.level.chunk.LevelChunk chunk))
            return;
        if (chunk.hasData(RtsimAttachments.PENDING))
            LOADED_CHUNKS.add(chunk);
        // A village start the registry does not know (a world made before the mod): adopted late (C22).
        com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption.loaded(chunk);
    }

    private static void onDrops(net.neoforged.neoforge.event.entity.living.LivingDropsEvent event) {
        if (event.getEntity().level().isClientSide())
            return;
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.onDrops(event.getEntity(), event.getDrops());
    }

    /**
     * The overworld saved: ask the engine to save its pages, and report when this world save is on
     * disk. The journal was serialized in {@code saveLevelData}, before this event, and NeoForge
     * chains every SavedData write on one future ({@code IOUtilities.withIOWorker}); a marker chained
     * now runs after the journal's write task has finished. That task may have held the file back
     * (its chunks not on disk, docs/choices-journal-barrier.md JB1) or failed to write it, and records
     * journaled during the chunk save (an unload's IND_CHUNK_UNLOADED at a flush save) are not in it:
     * the marker reports what the file on disk holds, read on the chain.
     */
    private static void onLevelSave(LevelEvent.Save event) {
        RtsimSession s = RtsimSession.current();
        if (s == null || !(event.getLevel() instanceof ServerLevel level) || level.dimension() != Level.OVERWORLD)
            return;
        long through = s.boundary().journal().nextObsSeq() - 1;
        s.engine().requestSave(through);
        com.minecraftindustrialcomplex.mic_rtsim.journal.JournalSavedData file = s.journal();
        net.neoforged.neoforge.common.IOUtilities.withIOWorker(() -> s.boundary().worldSaveDurable(Math.min(through, file.writtenThrough())));
    }

    /** A chunk's last write is on its way: the journal's barrier stops waiting for its unload (JB5). */
    private static void onChunkUnload(net.neoforged.neoforge.event.level.ChunkEvent.Unload event) {
        if (event.getLevel().isClientSide() || !(event.getChunk() instanceof net.minecraft.world.level.chunk.LevelChunk chunk))
            return;
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.journal().chunks().onUnload(chunk);
    }

    /**
     * Stale copies are discarded on join (PLAN.md §6.1 item 2): a tagged entity whose (id, epoch) and
     * UUID are not a live puppet of this session never enters the level. With no session at all (the
     * server is still starting), every tagged entity is stale: puppets never outlive a session.
     */
    private static void onJoin(EntityJoinLevelEvent event) {
        Level level = event.getLevel();
        if (level.isClientSide())
            return;
        Entity e = event.getEntity();
        if (!RtsimAttachments.isTagged(e)) {
            // An ordinary entity back from disk that still carries records the engine may never have
            // seen (a taken creature, a dropped cargo item): journal them again at the end of the tick.
            if (event.loadedFromDisk() && e.hasData(RtsimAttachments.PENDING))
                LOADED_ENTITIES.add(e);
            return;
        }
        RtsimSession s = RtsimSession.current();
        if (s == null || s.rejectOnJoin(e, level)) {
            event.setCanceled(true);
            // A copy loaded with passengers or a vehicle must not leave them pointing at it, and it is
            // marked removed, so a vehicle that keeps it (another mod cancelled the dismount) does not
            // tick it. It never joined, so its level callback is the null one and nothing else runs.
            e.ejectPassengers();
            e.stopRiding();
            e.discard();
        }
    }

    private static void onLeave(EntityLeaveLevelEvent event) {
        if (event.getLevel().isClientSide())
            return;
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.onLeave(event.getEntity());
    }

    private static void onDeath(LivingDeathEvent event) {
        if (event.getEntity().level().isClientSide())
            return;
        RtsimSession s = RtsimSession.current();
        if (s != null) {
            s.onDeath(event.getEntity(), event.getSource().getEntity());
            if (!s.failed())
                s.phase1.onDeath(event.getEntity(), event.getSource().getEntity());
        }
    }

    /** Taming takes a creature out of the sim (A10, PLAN.md §6.2). Lowest priority: only a tame that happens. */
    private static void onTame(AnimalTameEvent event) {
        if (event.getAnimal().level().isClientSide())
            return;
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.onTame(event.getAnimal(), event.getTamer());
    }

    /**
     * NeoForge 21.1 has no leash event (PLAN.md §6.2). A lead used on a puppet marks its group for a
     * check on this tick's pass; the pass's leash-holder check decides whether it was really taken.
     */
    private static void onEntityInteract(PlayerInteractEvent.EntityInteract event) {
        if (event.getLevel().isClientSide())
            return;
        RtsimSession s = RtsimSession.current();
        if (s == null)
            return;
        if (event.getItemStack().is(Items.LEAD))
            s.onLeadInteract(event.getTarget());
        if (!s.failed() && event.getTarget() instanceof net.minecraft.world.item.trading.Merchant)
            s.phase1.beforeTrading(event.getEntity(), event.getTarget()); // reprice before vanilla opens the trade
    }

    private static void onRegisterCommands(RegisterCommandsEvent event) {
        RtsimCommands.register(event.getDispatcher());
    }

    /** The server config changed: tell the engine the module switches (and the log who owns the traders). */
    private static void onConfigReload(ModConfigEvent.Reloading event) {
        if (event.getConfig().getSpec() != RtsimConfig.SPEC)
            return;
        MinecraftServer server = ServerLifecycleHooks.getCurrentServer();
        if (server == null)
            return;
        server.execute(() -> moduleSwitchesChanged(server));
    }

    /**
     * Applies the module switches now. Called on config reload and by the GameTests. The trader veto
     * needs no sync (it reads the switches at every spawner tick); this only logs it.
     */
    public static void moduleSwitchesChanged(MinecraftServer server) {
        TraderSpawning.announce(false);
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.onModulesChanged();
        MicRtsim.LOGGER.info("[mic_rtsim] module mask now {}", Integer.toBinaryString(RtsimConfig.effectiveMask()));
    }
}
