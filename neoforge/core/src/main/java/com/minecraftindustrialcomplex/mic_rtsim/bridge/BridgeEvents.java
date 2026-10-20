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
        bus.addListener(BridgeEvents::onServerStarted);
        bus.addListener(BridgeEvents::onServerStopping);
        bus.addListener(BridgeEvents::onServerStopped);
        bus.addListener(BridgeEvents::onServerTick);
        bus.addListener(BridgeEvents::onLevelSave);
        bus.addListener(EventPriority.HIGHEST, BridgeEvents::onJoin);
        bus.addListener(BridgeEvents::onLeave);
        bus.addListener(EventPriority.LOWEST, false, LivingDeathEvent.class, BridgeEvents::onDeath);
        bus.addListener(EventPriority.LOWEST, false, AnimalTameEvent.class, BridgeEvents::onTame);
        bus.addListener(BridgeEvents::onEntityInteract);
        bus.addListener(BridgeEvents::onRegisterCommands);
    }

    private static void onServerStarted(ServerStartedEvent event) {
        MinecraftServer server = event.getServer();
        TraderSpawning.sync(server);
        RtsimSession.startGlobal(server);
    }

    private static void onServerStopping(ServerStoppingEvent event) {
        RtsimSession.stoppingGlobal();
    }

    private static void onServerStopped(ServerStoppedEvent event) {
        RtsimSession.stopGlobal();
    }

    private static void onServerTick(ServerTickEvent.Post event) {
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.endOfTick();
    }

    /**
     * The overworld saved: ask the engine to save its pages, and report when this world save is on
     * disk. The journal was serialized in {@code saveLevelData}, before this event, and NeoForge
     * chains every SavedData write on one future ({@code IOUtilities.withIOWorker}); a marker chained
     * now runs after the journal's write has finished.
     */
    private static void onLevelSave(LevelEvent.Save event) {
        RtsimSession s = RtsimSession.current();
        if (s == null || !(event.getLevel() instanceof ServerLevel level) || level.dimension() != Level.OVERWORLD)
            return;
        s.engine().requestSave();
        long through = s.boundary().journal().nextObsSeq() - 1;
        net.neoforged.neoforge.common.IOUtilities.withIOWorker(() -> s.boundary().worldSaveDurable(through));
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
        if (!RtsimAttachments.isTagged(e))
            return;
        RtsimSession s = RtsimSession.current();
        if (s == null || s.rejectOnJoin(e, level)) {
            event.setCanceled(true);
            // A copy loaded with passengers or a vehicle must not leave them pointing at it.
            e.ejectPassengers();
            e.stopRiding();
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
        if (s != null)
            s.onDeath(event.getEntity(), event.getSource().getEntity());
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
        if (event.getLevel().isClientSide() || !event.getItemStack().is(Items.LEAD))
            return;
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.onLeadInteract(event.getTarget());
    }

    private static void onRegisterCommands(RegisterCommandsEvent event) {
        RtsimCommands.register(event.getDispatcher());
    }

    /** The server config changed: re-sync the trader gamerule and tell the engine the module switches. */
    private static void onConfigReload(ModConfigEvent.Reloading event) {
        if (event.getConfig().getSpec() != RtsimConfig.SPEC)
            return;
        MinecraftServer server = ServerLifecycleHooks.getCurrentServer();
        if (server == null)
            return;
        server.execute(() -> moduleSwitchesChanged(server));
    }

    /** Applies the module switches now. Called on config reload and by the GameTests. */
    public static void moduleSwitchesChanged(MinecraftServer server) {
        TraderSpawning.sync(server);
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.onModulesChanged();
        MicRtsim.LOGGER.info("[mic_rtsim] module mask now {}", Integer.toBinaryString(RtsimConfig.effectiveMask()));
    }
}
