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
import net.neoforged.neoforge.event.tick.ServerTickEvent;

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
    /** Every server chunk load since the start (R2-11): a detection reads it around its looks, which must never load one. */
    private static final java.util.concurrent.atomic.AtomicLong CHUNK_LOADS = new java.util.concurrent.atomic.AtomicLong();

    /** Server chunk loads so far ({@code ChunkEvent.Load} of a server level's full chunk), whether or not a module runs. */
    public static long chunkLoads() {
        return CHUNK_LOADS.get();
    }

    /** R2-11: set while a detection step runs; the first chunk load meanwhile keeps its stack (what made the server thread load it). */
    static volatile boolean watchLoads;
    static volatile Throwable loadInDetection;

    static void register() {
        IEventBus bus = NeoForge.EVENT_BUS;
        bus.addListener(IndustryEvents::onChunkLoad);
        bus.addListener(IndustryEvents::onServerTickPre);
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
        drainLoads(ind, now);
        for (Placed p; (p = PLACED.poll()) != null; )
            ind.districts.playerPlaced(p.level, p.pos, p.player, now);
    }

    /**
     * Chunk loads, at the start of the tick (loads between ticks, which is where the chunk system
     * finishes most) and at its end (loads inside it): held reconciles land in each chunk before its
     * IND_CHUNK_LOADED, then the tick gates lift (R2).
     */
    static void drainLoads(Industry ind, long now) {
        for (LevelChunk c; (c = LOADED.poll()) != null; )
            if (c.getLevel() instanceof ServerLevel level && level.getChunkSource().getChunkNow(c.getPos().x, c.getPos().z) == c)
                ind.districts.onChunkLoad(level, c, now);
        TickGate.releaseAll();
    }

    private static void onChunkLoad(ChunkEvent.Load event) {
        // Only while a module will drain the queue: chunks loaded before the session (spawn chunks) are
        // found at the session start instead, and nothing is retained when the module is off.
        if (event.getLevel().isClientSide() || !(event.getChunk() instanceof LevelChunk chunk) || !(event.getLevel() instanceof ServerLevel level))
            return;
        CHUNK_LOADS.incrementAndGet();
        if (watchLoads && loadInDetection == null)
            loadInDetection = new Throwable("a chunk loaded inside a district detection step (R2-11)");
        Industry ind = IndustryApi.current();
        if (ind == null)
            return;
        LOADED.add(chunk);
        // A reconcile held for this chunk lands at the next drain, at most a tick away; until then no block
        // entity of the chunk, or of the district's other loaded chunks, ticks (R2). The event comes inside
        // the chunk system's main-thread task, so the reconcile itself waits: applying it could load another
        // chunk synchronously from there.
        if (ind.reconciler.holdsFor(level.dimension(), chunk.getPos().toLong()))
            ind.districts.holdTicking(level, chunk);
    }

    private static void onServerTickPre(ServerTickEvent.Pre event) {
        long t0 = System.nanoTime();
        Industry ind = IndustryApi.current();
        if (ind != null)
            ind.startOfTick();
        com.minecraftindustrialcomplex.mic_rtsim.bridge.TickCost.pre(System.nanoTime() - t0);
    }

    private static void onChunkUnload(ChunkEvent.Unload event) {
        if (event.getLevel().isClientSide() || !(event.getChunk() instanceof LevelChunk chunk) || !(event.getLevel() instanceof ServerLevel level))
            return;
        Industry ind = IndustryApi.current();
        if (ind != null)
            ind.districts.onChunkUnload(level, chunk);
    }

    /*
     * A FakePlayer (a Create deployer, a mod's block breaker) is a machine: its placements and breaks are
     * no edit and no seed (INDUSTRY.md §3.3). A busy district's deployers otherwise restart its glass
     * compile on every swing and journal an IND_EDIT for each.
     */
    private static void onPlace(BlockEvent.EntityPlaceEvent event) {
        if (!(event.getEntity() instanceof Player player) || player.isFakePlayer() || !(event.getLevel() instanceof ServerLevel level)
                || IndustryApi.current() == null)
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
        ind.ports.breaking(event.getPlayer(), level, event.getPos()); // a machine breaking boundary stock still moves goods
        if (!event.getPlayer().isFakePlayer())
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
        com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassGraphCommand.register(event.getDispatcher());
        com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassSectionsCommand.register(event.getDispatcher());
    }
}
