package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.packs.PackLocationInfo;
import net.minecraft.server.packs.PackSelectionConfig;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.PathPackResources;
import net.minecraft.server.packs.repository.Pack;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModList;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.AddPackFindersEvent;
import net.neoforged.neoforge.event.entity.living.LivingChangeTargetEvent;
import net.neoforged.neoforge.event.entity.living.LivingDeathEvent;
import net.neoforged.neoforge.event.entity.living.LivingDropsEvent;
import net.neoforged.neoforge.event.entity.living.LivingIncomingDamageEvent;
import net.neoforged.neoforge.event.entity.player.PlayerInteractEvent;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.server.ServerStoppedEvent;

import java.nio.file.Path;
import java.util.Optional;

/**
 * The NeoForge hooks of the conflict bridge: deaths and their drops, the envoy party's guard (§4.5:
 * targets and hits), payment by right-click, and the territory signals (a log or ore broken, a block
 * placed). Takings (Carry On) need no hook of their own: they reach {@link ConflictBridge#take} through
 * the core. With the conflict module off, or no live session, each hook returns at once.
 */
public final class ConflictEvents {
    private ConflictEvents() {}

    public static void register(IEventBus modBus) {
        RtsimApi.registerPlugin(ConflictBridge.INSTANCE);
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(EventPriority.LOWEST, false, LivingDeathEvent.class, ConflictEvents::onDeath);
        bus.addListener(EventPriority.LOWEST, false, LivingDropsEvent.class, ConflictEvents::onDrops);
        bus.addListener(EventPriority.LOW, false, LivingChangeTargetEvent.class, ConflictEvents::onTarget);
        bus.addListener(EventPriority.LOW, false, LivingIncomingDamageEvent.class, ConflictEvents::onHit);
        bus.addListener(EventPriority.HIGHEST, false, PlayerInteractEvent.EntityInteract.class, ConflictEvents::onInteract);
        bus.addListener(EventPriority.LOWEST, false, BlockEvent.BreakEvent.class, ConflictEvents::onBreak);
        bus.addListener(EventPriority.LOWEST, false, BlockEvent.EntityPlaceEvent.class, ConflictEvents::onPlace);
        bus.addListener(ServerStoppedEvent.class, e -> ConflictBridge.INSTANCE.serverStopped());
        if (RtsimConfig.Test.ENABLED)
            modBus.addListener(ConflictEvents::onPackFinders);
    }

    /** The live session the conflict bridge is part of, with the module on, or null. */
    private static RtsimSession live() {
        RtsimSession s = RtsimSession.current();
        return s == null || s.failed() || ConflictBridge.INSTANCE.session() != s || !RtsimConfig.on(Module.CONFLICT) ? null : s;
    }

    private static void onDeath(LivingDeathEvent event) {
        RtsimSession s;
        if (event.getEntity().level().isClientSide() || (s = live()) == null)
            return;
        ConflictBridge.INSTANCE.onDeath(event.getEntity(), event.getSource(), s.gameTime());
    }

    private static void onDrops(LivingDropsEvent event) {
        if (event.getEntity().level().isClientSide() || live() == null)
            return;
        ConflictBridge.INSTANCE.onDrops(event.getEntity(), event.getDrops());
    }

    private static void onTarget(LivingChangeTargetEvent event) {
        if (event.getEntity().level().isClientSide() || live() == null)
            return;
        if (ConflictBridge.INSTANCE.blockTarget(event.getEntity(), event.getNewAboutToBeSetTarget()))
            event.setCanceled(true);
    }

    private static void onHit(LivingIncomingDamageEvent event) {
        RtsimSession s;
        if (event.getEntity().level().isClientSide() || (s = live()) == null)
            return;
        if (ConflictBridge.INSTANCE.blockHit(event.getEntity(), event.getSource(), s.gameTime()))
            event.setCanceled(true);
    }

    private static void onInteract(PlayerInteractEvent.EntityInteract event) {
        RtsimSession s;
        if (event.getLevel().isClientSide() || (s = live()) == null)
            return;
        if (ConflictBridge.INSTANCE.interact(event.getEntity(), event.getTarget(), event.getHand(), s.gameTime())) {
            event.setCancellationResult(InteractionResult.SUCCESS);
            event.setCanceled(true);
        }
    }

    private static void onBreak(BlockEvent.BreakEvent event) {
        RtsimSession s;
        if (!(event.getLevel() instanceof ServerLevel level) || (s = live()) == null)
            return;
        ConflictBridge.INSTANCE.onBreak(level, event.getPlayer(), event.getPos(), event.getState(), s.gameTime());
    }

    private static void onPlace(BlockEvent.EntityPlaceEvent event) {
        RtsimSession s;
        if (!(event.getLevel() instanceof ServerLevel level) || !(event.getEntity() instanceof Player player) || (s = live()) == null)
            return;
        ConflictBridge.INSTANCE.onPlace(level, player, event.getPos(), s.gameTime());
    }

    /**
     * The core's GameTest server only: a datapack adding {@code minecraft:endermite} to
     * {@code mic_rtsim:threat/ignored}, the stand-in for Mowzie's foliaath in test 18 (MZB-114).
     */
    private static void onPackFinders(AddPackFindersEvent event) {
        if (event.getPackType() != PackType.SERVER_DATA)
            return;
        Path root = ModList.get().getModFileById(MicRtsim.MODID).getFile().findResource("conflict_test_pack");
        PackLocationInfo info = new PackLocationInfo("mic_rtsim_conflict_test", Component.literal("mic_rtsim: conflict GameTest stand-ins"),
                PackSource.BUILT_IN, Optional.empty());
        Pack pack = Pack.readMetaAndCreate(info, new PathPackResources.PathResourcesSupplier(root), PackType.SERVER_DATA,
                new PackSelectionConfig(true, Pack.Position.TOP, false));
        if (pack != null)
            event.addRepositorySource(consumer -> consumer.accept(pack));
    }
}
