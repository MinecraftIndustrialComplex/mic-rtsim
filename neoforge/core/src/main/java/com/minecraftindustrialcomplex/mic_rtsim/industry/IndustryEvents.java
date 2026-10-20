package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.common.util.BlockSnapshot;
import net.neoforged.neoforge.event.RegisterCommandsEvent;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.item.ItemTossEvent;
import net.neoforged.neoforge.event.entity.player.ItemEntityPickupEvent;
import net.neoforged.neoforge.event.entity.player.PlayerContainerEvent;
import net.neoforged.neoforge.event.entity.player.PlayerInteractEvent;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.level.ChunkEvent;

import java.util.UUID;
import java.util.concurrent.ConcurrentLinkedQueue;

/**
 * Every NeoForge hook the industry module uses (PATCHES.md lists them). The handlers are thin: the
 * module does the work, and events that may come before the session exists (chunk loads of spawn
 * chunks) or before the block entity is in place (placements) wait for the end of the tick.
 */
final class IndustryEvents {
    private IndustryEvents() {}

    private record Placed(ServerLevel level, BlockPos pos, UUID player) {}

    private static final ConcurrentLinkedQueue<LevelChunk> LOADED = new ConcurrentLinkedQueue<>();
    private static final ConcurrentLinkedQueue<Placed> PLACED = new ConcurrentLinkedQueue<>();

    static void register() {
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(IndustryEvents::onChunkLoad);
        bus.addListener(IndustryEvents::onChunkUnload);
        bus.addListener(EventPriority.LOWEST, false, BlockEvent.EntityPlaceEvent.class, IndustryEvents::onPlace);
        bus.addListener(EventPriority.LOWEST, false, BlockEvent.BreakEvent.class, IndustryEvents::onBreak);
        bus.addListener(EventPriority.LOWEST, false, PlayerInteractEvent.RightClickBlock.class, IndustryEvents::onRightClick);
        bus.addListener(IndustryEvents::onContainerOpen);
        bus.addListener(IndustryEvents::onContainerClose);
        bus.addListener(EventPriority.LOWEST, false, ItemTossEvent.class, IndustryEvents::onToss);
        bus.addListener(IndustryEvents::onPickup);
        bus.addListener(EventPriority.LOWEST, false, EntityJoinLevelEvent.class, IndustryEvents::onJoin);
        bus.addListener(IndustryEvents::onRegisterCommands);
    }

    /** At the end of the tick, with the session: chunk loads and placements in the order they came. */
    static void drain(Industry ind, long now) {
        for (LevelChunk c; (c = LOADED.poll()) != null; )
            if (c.getLevel() instanceof ServerLevel level && level.getChunkSource().getChunkNow(c.getPos().x, c.getPos().z) == c)
                ind.districts.onChunkLoad(level, c, now);
        for (Placed p; (p = PLACED.poll()) != null; )
            ind.districts.playerPlaced(p.level, p.pos, p.player, now);
    }

    private static void onChunkLoad(ChunkEvent.Load event) {
        // Only while a module will drain the queue: chunks loaded before the session (spawn chunks) are
        // found at the session start instead, and nothing is retained when the module is off.
        if (!event.getLevel().isClientSide() && event.getChunk() instanceof LevelChunk chunk && IndustryApi.current() != null)
            LOADED.add(chunk);
    }

    private static void onChunkUnload(ChunkEvent.Unload event) {
        if (event.getLevel().isClientSide() || !(event.getChunk() instanceof LevelChunk chunk) || !(event.getLevel() instanceof ServerLevel level))
            return;
        Industry ind = IndustryApi.current();
        if (ind != null)
            ind.districts.onChunkUnload(level, chunk);
    }

    private static void onPlace(BlockEvent.EntityPlaceEvent event) {
        if (!(event.getEntity() instanceof Player player) || !(event.getLevel() instanceof ServerLevel level) || IndustryApi.current() == null)
            return;
        if (event instanceof BlockEvent.EntityMultiPlaceEvent multi) {
            for (BlockSnapshot snap : multi.getReplacedBlockSnapshots())
                PLACED.add(new Placed(level, snap.getPos().immutable(), player.getUUID()));
        } else {
            PLACED.add(new Placed(level, event.getPos().immutable(), player.getUUID()));
        }
    }

    private static void onBreak(BlockEvent.BreakEvent event) {
        if (!(event.getLevel() instanceof ServerLevel level) || event.getPlayer() == null)
            return;
        Industry ind = IndustryApi.current();
        if (ind == null)
            return;
        ind.ports.breaking(event.getPlayer(), level, event.getPos());
        ind.districts.playerBroke(level, event.getPos(), event.getPlayer().getUUID());
    }

    private static void onRightClick(PlayerInteractEvent.RightClickBlock event) {
        if (!(event.getLevel() instanceof ServerLevel level))
            return;
        Industry ind = IndustryApi.current();
        if (ind != null)
            ind.ports.rightClicked(event.getEntity(), level, event.getPos());
    }

    private static void onContainerOpen(PlayerContainerEvent.Open event) {
        if (!(event.getEntity() instanceof ServerPlayer player))
            return;
        Industry ind = IndustryApi.current();
        if (ind != null)
            ind.ports.opened(player, event.getContainer());
    }

    private static void onContainerClose(PlayerContainerEvent.Close event) {
        if (!(event.getEntity() instanceof ServerPlayer player))
            return;
        Industry ind = IndustryApi.current();
        if (ind != null)
            ind.ports.closed(player);
    }

    private static void onToss(ItemTossEvent event) {
        if (event.getPlayer().level().isClientSide())
            return;
        Industry ind = IndustryApi.current();
        if (ind != null)
            ind.ports.tossed(event.getPlayer(), event.getEntity());
    }

    private static void onPickup(ItemEntityPickupEvent.Post event) {
        if (event.getPlayer().level().isClientSide())
            return;
        Industry ind = IndustryApi.current();
        if (ind != null)
            ind.ports.pickedUp(event.getPlayer(), event.getItemEntity(), event.getOriginalStack(), event.getCurrentStack());
    }

    private static void onJoin(EntityJoinLevelEvent event) {
        if (event.getLevel().isClientSide() || !(event.getEntity() instanceof ItemEntity item) || event.loadedFromDisk())
            return;
        Industry ind = IndustryApi.current();
        if (ind != null)
            ind.ports.itemJoined(item);
    }

    private static void onRegisterCommands(RegisterCommandsEvent event) {
        IndustryCommands.register(event.getDispatcher());
    }
}
