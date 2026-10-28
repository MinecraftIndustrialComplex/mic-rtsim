package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.simibubi.create.content.trains.graph.DimensionPalette;
import com.simibubi.create.content.trains.graph.TrackNodeLocation;
import com.simibubi.create.content.trains.station.GlobalPackagePort;
import com.simibubi.create.content.trains.station.GlobalStation;
import net.createmod.catnip.data.Couple;
import net.createmod.catnip.nbt.NBTHelper;
import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.Level;

import java.util.Arrays;
import java.util.UUID;

/**
 * A Create station port's rings of applied IND_WRITE_PACKAGE op ids (docs/choices-journal-barrier.md RP-206): two
 * {@link AppliedOpRing}s of {@link #CAPACITY} in the port's entry of its station's "Ports" list in
 * create_tracks.dat, the save and load hooks {@code ringmixin.StationPortsRingMixin} calls, and whether the mixins
 * applied.
 *
 * <p>The port's offline buffer is in that entry, so a ring there is saved by the same write as the packages it
 * guards: a crash keeps both or neither. The journal's file is another file, written last (RP-200), so its applied
 * set would let the engine's resend of an op the tracks file already holds write the packages twice.
 * {@link WritePackages} keeps two rings: {@link #KEY} for the packages it put in the offline buffer, and
 * {@link #POSTBOX_KEY} for the ones it put in a loaded postbox, which Create mirrors into the buffer (unprimed)
 * at the postbox's {@code setChanged}. An offline write is refused by either ring; a loaded write by the buffer's
 * ring or the postbox chunk's carried result (not the postbox's ring: the tracks file can be newer than the
 * chunk, and the package is then missing from the postbox the engine's resend finds loaded).
 *
 * <p>An op stays in a port's ring until the engine can no longer send it, T2's rule (docs/choices-mowzie-build.md
 * RP-250, RP-251; docs/choices-journal-barrier.md RP-209), as on a train's ring and through {@link TrainRing}'s
 * hooks: each answer (APPLIED, or DUPLICATE_OP on a hit) is recorded with its sequence, and the engine's
 * acknowledgement of that sequence removes the op at the session hook's end of tick; a ring another session answered
 * is settled at this session's start ({@link #sessionStarted}) or at its load ({@link #read}) against the engine's
 * pages; the industry's announce makes every answer unknown ({@link #announce}). An op the postbox's chunk was found
 * to carry ({@link #refreshed}) joins the postbox's ring with its answer unknown, and leaves only at its resend's
 * answer or by the valve. {@link #CAPACITY} is a valve, counted and logged.
 *
 * <p>The mixin config is optional ({@code mic_rtsim_create_ring.mixins.json}: not required, require 0), so a
 * Create that moved {@code GlobalStation.write} or {@code read} loads without the rings. {@link #available} then
 * finds the round trip broken, logs that once, and WritePackages falls back to the journal's applied set.
 */
public final class PortRing {
    private PortRing() {}

    /** The ops whose packages went into the offline buffer: a long array in the port's entry, oldest first. */
    public static final String KEY = "mic_rtsim:applied_ops";
    /** The ops whose packages went into the loaded postbox. */
    public static final String POSTBOX_KEY = "mic_rtsim:applied_ops_postbox";
    /**
     * The valve, as the train ring's (RP-250): an op leaves its ring when the engine acknowledges its answer, so a
     * ring holds only the ops whose answers the engine has not saved yet. A ring that reaches this many drops its
     * oldest op, counted ({@link #evictions}) and logged for the first few ports: that op is written twice only if
     * the engine sends it again. One IND_WRITE_PACKAGE fills at least one of the buffer's 18 slots, so a port
     * reaches this only if trains empty it as fast for thousands of unacknowledged writes. At most 64 KiB of op ids
     * (and as many answer sequences) a ring.
     */
    public static final int CAPACITY = 4096;
    /** The op the probe writes and reads back: an engine op id no engine hands out (top bit set, F4). */
    private static final long PROBE_OP = 0xFFFF_FFFF_FFFF_FFF2L;

    /** Null until the first {@link #available} after start or a test switch; then the answer. */
    private static Boolean available;
    private static boolean fallbackForTest;
    private static int fallbackLogs;
    private static boolean probing;
    private static long evictions;
    private static int evictionLogs;

    /**
     * True when the ring mixins applied: {@code GlobalPackagePort} implements {@link PortRingHolder} and a probe
     * station's port rings survive a {@code GlobalStation.write} then {@code read}. Probed once, on the first call.
     */
    public static boolean available(MinecraftServer server) {
        if (available == null) {
            String why = fallbackForTest ? "switched off by a GameTest" : probe(server.registryAccess());
            available = why == null;
            if (why != null) {
                fallbackLogs++;
                MicRtsim.LOGGER.warn("[mic_rtsim_create] the station port's op rings are off ({}): IND_WRITE_PACKAGE's dedup is the "
                        + "journal's applied set, which writes an offline buffer's packages twice after a crash that kept the tracks file "
                        + "and not the journal's (docs/choices-journal-barrier.md RP-206)", why);
            } else {
                MicRtsim.LOGGER.info("[mic_rtsim_create] the station port's op rings are on: IND_WRITE_PACKAGE's dedup is each port's {} and {}",
                        KEY, POSTBOX_KEY);
            }
        }
        return available;
    }

    /** True when {@code port}'s buffer ring holds {@code op}; false for no port or no ring. */
    public static boolean wroteToBuffer(GlobalPackagePort port, long op) {
        AppliedOpRing r = port instanceof PortRingHolder h ? h.mic_rtsim$bufferOpsIfAny() : null;
        return r != null && r.contains(op);
    }

    /** True when {@code port}'s postbox ring holds {@code op}. */
    public static boolean wroteToPostbox(GlobalPackagePort port, long op) {
        AppliedOpRing r = port instanceof PortRingHolder h ? h.mic_rtsim$postboxOpsIfAny() : null;
        return r != null && r.contains(op);
    }

    /**
     * Records an op whose packages went into the offline buffer ({@code postbox} false) or the loaded postbox,
     * answered APPLIED at {@code seq}: it leaves the ring once the engine acknowledges {@code seq}. A full ring
     * drops its oldest op ({@link #CAPACITY}).
     */
    public static void record(GlobalPackagePort port, long op, boolean postbox, long seq, BlockPos pos) {
        if (!(port instanceof PortRingHolder h))
            return;
        AppliedOpRing ring = postbox ? h.mic_rtsim$postboxOps() : h.mic_rtsim$bufferOps();
        TrainRing.settle(ring);
        long before = ring.evictions();
        ring.add(op);
        if (ring.evictions() != before) {
            evictions++;
            if (evictionLogs++ < 8)
                MicRtsim.LOGGER.warn("[mic_rtsim_create] the station port at {} has a full op ring ({}): {} ops whose answers the engine "
                        + "has not acknowledged, so op {} leaves it, and if the engine sends that op again it writes its packages twice "
                        + "(docs/choices-journal-barrier.md RP-209). {} evicted on all ports so far (/rtsim district perf)",
                        pos, postbox ? POSTBOX_KEY : KEY, ring.capacity(), Long.toHexString(ring.lastEvicted()), evictions);
        }
        TrainRing.answered(ring, op, seq);
    }

    /**
     * A resend refused (DUPLICATE_OP at {@code seq}): each of the port's rings that holds the op takes that answer,
     * and the op leaves once the engine acknowledges it.
     */
    public static void refused(GlobalPackagePort port, long op, long seq) {
        if (!(port instanceof PortRingHolder h))
            return;
        for (AppliedOpRing ring : new AppliedOpRing[] {h.mic_rtsim$bufferOpsIfAny(), h.mic_rtsim$postboxOpsIfAny()})
            if (ring != null && ring.contains(op)) {
                TrainRing.settle(ring);
                TrainRing.answered(ring, op, seq);
            }
    }

    /** Settles the port's rings for this session, if another answered them ({@link TrainRing#settle}). */
    public static void settle(GlobalPackagePort port) {
        if (!(port instanceof PortRingHolder h))
            return;
        if (h.mic_rtsim$bufferOpsIfAny() != null)
            TrainRing.settle(h.mic_rtsim$bufferOpsIfAny());
        if (h.mic_rtsim$postboxOpsIfAny() != null)
            TrainRing.settle(h.mic_rtsim$postboxOpsIfAny());
    }

    /**
     * {@code GlobalStation.write}'s hook: each port's rings into its entry of "Ports", when they hold an op, whatever
     * the industry module's switch (docs/replay-gate.md 9.6, "Rings and the module switch"): the switch gates only
     * whether new ops are applied. A save that left the rings out after a switch off could die before the journal's
     * write, and an op written before the switch would be written again at its resend. With the plugin removed the
     * mixins are gone, and Create's own write leaves the keys out (UR-1).
     */
    public static void write(GlobalStation station, CompoundTag nbt) {
        if (station == null || nbt == null || station.connectedPorts.isEmpty())
            return;
        ListTag ports = nbt.getList("Ports", Tag.TAG_COMPOUND);
        for (int i = 0; i < ports.size(); i++) {
            CompoundTag entry = ports.getCompound(i);
            if (!entry.contains("Pos"))
                continue;
            if (!(station.connectedPorts.get(NBTHelper.readBlockPos(entry, "Pos")) instanceof PortRingHolder h))
                continue;
            AppliedOpRing buffer = h.mic_rtsim$bufferOpsIfAny();
            if (buffer != null && !buffer.isEmpty())
                buffer.writeTo(entry, KEY);
            AppliedOpRing postbox = h.mic_rtsim$postboxOpsIfAny();
            if (postbox != null && !postbox.isEmpty())
                postbox.writeTo(entry, POSTBOX_KEY);
        }
    }

    /**
     * {@code GlobalStation.read}'s hook: the saved rings onto the ports Create made, matched by Pos. Read during a
     * session, each ring is settled or waits for its answers' acknowledgements again ({@link TrainRing#loaded}).
     */
    public static void read(GlobalStation station, CompoundTag nbt) {
        if (station == null || nbt == null || station.connectedPorts.isEmpty())
            return;
        ListTag ports = nbt.getList("Ports", Tag.TAG_COMPOUND);
        for (int i = 0; i < ports.size(); i++) {
            CompoundTag entry = ports.getCompound(i);
            boolean buffer = AppliedOpRing.savedIn(entry, KEY), postbox = AppliedOpRing.savedIn(entry, POSTBOX_KEY);
            if ((!buffer && !postbox) || !entry.contains("Pos"))
                continue;
            if (!(station.connectedPorts.get(NBTHelper.readBlockPos(entry, "Pos")) instanceof PortRingHolder h))
                continue;
            if (buffer) {
                h.mic_rtsim$bufferOps().readFrom(entry, KEY);
                if (!probing)
                    TrainRing.loaded(h.mic_rtsim$bufferOps());
            }
            if (postbox) {
                h.mic_rtsim$postboxOps().readFrom(entry, POSTBOX_KEY);
                if (!probing)
                    TrainRing.loaded(h.mic_rtsim$postboxOps());
            }
        }
    }

    // ---- the session: settle, announce, counters (T2's rule) --------------------------------------

    /** Calls {@code each} with every ring of every station port Create knows now. */
    private static void eachRing(java.util.function.Consumer<AppliedOpRing> each) {
        for (com.simibubi.create.content.trains.graph.TrackGraph graph : com.simibubi.create.Create.RAILWAYS.trackNetworks.values())
            for (GlobalStation st : graph.getPoints(com.simibubi.create.content.trains.graph.EdgePointType.STATION))
                for (GlobalPackagePort port : st.connectedPorts.values())
                    if (port instanceof PortRingHolder h) {
                        if (h.mic_rtsim$bufferOpsIfAny() != null)
                            each.accept(h.mic_rtsim$bufferOpsIfAny());
                        if (h.mic_rtsim$postboxOpsIfAny() != null)
                            each.accept(h.mic_rtsim$postboxOpsIfAny());
                    }
    }

    /** The session started ({@link TrainRing#sessionStarted}): settles every port ring loaded with the world. True when any held an op. */
    static boolean sessionStarted() {
        boolean[] any = {false};
        eachRing(ring -> {
            if (!ring.isEmpty()) {
                TrainRing.settle(ring);
                any[0] = true;
            }
        });
        return any[0];
    }

    /**
     * The industry comes back on (or the engine lost observations), {@code CreatePlugin.announce}: every answer this
     * session gave a port's op becomes unknown, as on the trains' rings ({@link TrainRing#announce}).
     */
    static void announce() {
        com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession s = com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession.current();
        long gen = s == null ? 0 : s.boundary().gen();
        eachRing(ring -> {
            if (ring.gen() == gen)
                ring.forgetAnswers();
        });
    }

    /** Ops pushed out of full port rings this server run (each a possible double write). */
    public static long evictions() {
        return evictions;
    }

    /** For {@code /rtsim district perf}. */
    static String perfLine() {
        long[] held = {0, 0, 0};
        eachRing(ring -> {
            if (!ring.isEmpty()) {
                held[0]++;
                held[1] += ring.size();
                held[2] += ring.unknownCount();
            }
        });
        return "port rings: " + held[1] + " ops in " + held[0] + " rings (" + held[2] + " answers unknown), " + evictions + " evicted (valve "
                + CAPACITY + ")";
    }

    /** Ops held now in every port ring whose answers are unknown (GameTests). */
    public static long unknownCountForTest() {
        long[] n = {0};
        eachRing(ring -> n[0] += ring.unknownCount());
        return n[0];
    }

    // ---- the postbox's chunk (P1 of the slice U review) -------------------------------------------

    /** Ops a postbox's chunk was made to carry at a restore ({@link #beforeRestore}). */
    static long carriedAtRestore;
    /** Offline writes held for a read of the postbox's chunk; reads that refreshed the buffer; carried ops found. */
    static long refreshHolds, refreshes, refreshedFromChunk, carriedFound;
    private static int refreshFailLogs;
    /** How long an offline write waits for its postbox's chunk to be read before it goes on without it. */
    static final long REFRESH_WAIT_NANOS = 10_000_000_000L;
    private static final java.util.Map<GlobalPackagePort, PendingRead> READS = new java.util.WeakHashMap<>();
    /** Reads of a port's chunk that failed or timed out; at {@link #REFRESH_TRIES} the port is taken as fresh. */
    private static final java.util.Map<GlobalPackagePort, Integer> FAILS = new java.util.WeakHashMap<>();
    static final int REFRESH_TRIES = 3;
    /** GameTests: the restore's carry and the chunk read switched off, as before them. */
    private static boolean chunkChecksOff;

    private record PendingRead(java.util.concurrent.CompletableFuture<java.util.Optional<CompoundTag>> read, long since) {}

    /**
     * Create's restore of a primed offline buffer into the postbox ({@code ringmixin.StationPortRestoreMixin}): the
     * postbox's chunk is made to carry each op of the buffer's ring whose packages the restore brings in and whose
     * result the engine has not acknowledged ({@code Industry.carryAppliedInChunk}). The chunk is saved on its own
     * clock (an unload, a world save), so it can reach disk before create_tracks.dat and the journal: a crash then
     * keeps the packages in the postbox and loses the op from the buffer's ring and the journal, and the resend must
     * find the op in the chunk, loaded ({@code chunkCarriesOp}) or not ({@link #refreshed}).
     */
    public static void beforeRestore(com.simibubi.create.content.logistics.packagePort.PackagePortBlockEntity ppbe, GlobalPackagePort port) {
        if (chunkChecksOff || port == null || !port.primed || ppbe == null || !(ppbe.getLevel() instanceof net.minecraft.server.level.ServerLevel level))
            return;
        AppliedOpRing ring = port instanceof PortRingHolder h ? h.mic_rtsim$bufferOpsIfAny() : null;
        if (ring == null || ring.isEmpty())
            return;
        com.minecraftindustrialcomplex.mic_rtsim.industry.Industry ind = com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi.current();
        if (ind == null || !available(level.getServer()))
            return;
        net.minecraft.world.level.chunk.LevelChunk chunk = level.getChunkSource().getChunkNow(ppbe.getBlockPos().getX() >> 4, ppbe.getBlockPos().getZ() >> 4);
        if (chunk == null)
            return;
        for (long op : ring.toArray()) {
            if (ind.opApplied(op) && !ind.chunkCarriesOp(chunk, op)) {
                ind.carryAppliedInChunk(chunk, op);
                carriedAtRestore++;
            }
        }
    }

    /**
     * Before an offline write into a port this server life does not know to match its postbox
     * ({@link PortRingHolder#mic_rtsim$mirrorFresh}), whose postbox's chunk is not loaded: the chunk as saved is read
     * ({@code ChunkStorage.read}, off the server thread) and the write held, false, until it is in (at most
     * {@link #REFRESH_WAIT_NANOS}). Then the ops the chunk carries results for go in the port's postbox ring (their
     * packages are in that chunk), and an unprimed buffer takes the postbox's inventory from it. Create keeps an
     * unprimed buffer only as a copy of the postbox, and a crash can leave the copy in create_tracks.dat older than
     * the chunk: priming it would put back, at the postbox's next load, what was taken out since. A primed buffer
     * holds what Create or a write put there for the postbox and is left as it is. True when the write can go on.
     */
    public static boolean refreshed(GlobalPackagePort port, net.minecraft.server.level.ServerLevel level, BlockPos pos,
            com.minecraftindustrialcomplex.mic_rtsim.industry.Industry ind) {
        if (chunkChecksOff || !(port instanceof PortRingHolder h) || h.mic_rtsim$mirrorFresh())
            return true;
        PendingRead r = READS.get(port);
        if (r == null) {
            java.util.concurrent.CompletableFuture<java.util.Optional<CompoundTag>> read;
            try {
                read = level.getChunkSource().chunkMap.read(new net.minecraft.world.level.ChunkPos(pos));
            } catch (RuntimeException e) {
                refreshFailed(port, h, pos, "the read threw " + e);
                return true;
            }
            READS.put(port, new PendingRead(read, System.nanoTime()));
            refreshHolds++;
            return false;
        }
        if (!r.read.isDone()) {
            if (System.nanoTime() - r.since < REFRESH_WAIT_NANOS) {
                refreshHolds++;
                return false;
            }
            READS.remove(port);
            refreshFailed(port, h, pos, "the read took longer than " + REFRESH_WAIT_NANOS / 1_000_000_000L + " s");
            return true;
        }
        READS.remove(port);
        java.util.Optional<CompoundTag> tag;
        try {
            tag = r.read.join();
        } catch (RuntimeException e) {
            refreshFailed(port, h, pos, "the read failed: " + e);
            return true;
        }
        refreshes++;
        FAILS.remove(port);
        if (tag.isPresent())
            refreshFrom(port, h, tag.get(), pos, level.registryAccess(), ind);
        h.mic_rtsim$markMirrorFresh();
        return true;
    }

    /** The postbox's chunk is loaded: a read of it begun before is stale. */
    static void dropRead(GlobalPackagePort port) {
        READS.remove(port);
    }

    private static void refreshFrom(GlobalPackagePort port, PortRingHolder h, CompoundTag chunk, BlockPos pos,
            HolderLookup.Provider registries, com.minecraftindustrialcomplex.mic_rtsim.industry.Industry ind) {
        long found = carriedFound;
        for (long op : ind.opsCarriedInChunkTag(chunk)) {
            if (h.mic_rtsim$postboxOps().add(op))
                carriedFound++;
        }
        if (carriedFound != found)
            com.simibubi.create.Create.RAILWAYS.markTracksDirty();
        // A chunk of another data version is read by vanilla through its fixers; the inventory is left alone then.
        if (port.primed || net.minecraft.world.level.chunk.storage.ChunkStorage.getVersion(chunk)
                != net.minecraft.SharedConstants.getCurrentVersion().getDataVersion().getVersion())
            return;
        ListTag bes = chunk.getList("block_entities", Tag.TAG_COMPOUND);
        for (int i = 0; i < bes.size(); i++) {
            CompoundTag be = bes.getCompound(i);
            if (be.getInt("x") != pos.getX() || be.getInt("y") != pos.getY() || be.getInt("z") != pos.getZ())
                continue;
            if (!be.getString("id").equals("create:package_postbox") || !be.contains("Inventory", Tag.TAG_COMPOUND))
                return;
            net.neoforged.neoforge.items.ItemStackHandler saved = new net.neoforged.neoforge.items.ItemStackHandler(port.offlineBuffer.getSlots());
            saved.deserializeNBT(registries, be.getCompound("Inventory"));
            boolean changed = false;
            for (int s = 0; s < port.offlineBuffer.getSlots(); s++) {
                net.minecraft.world.item.ItemStack want = s < saved.getSlots() ? saved.getStackInSlot(s) : net.minecraft.world.item.ItemStack.EMPTY;
                if (!net.minecraft.world.item.ItemStack.matches(want, port.offlineBuffer.getStackInSlot(s))) {
                    port.offlineBuffer.setStackInSlot(s, want.copy());
                    changed = true;
                }
            }
            if (changed) {
                refreshedFromChunk++;
                com.simibubi.create.Create.RAILWAYS.markTracksDirty();
            }
            return;
        }
    }

    /**
     * A read that failed or timed out: this write goes on without it, and the next offline write into the port reads
     * the chunk again, up to {@link #REFRESH_TRIES} reads, after which the port is taken as fresh for the life.
     */
    private static void refreshFailed(GlobalPackagePort port, PortRingHolder h, BlockPos pos, String why) {
        int n = FAILS.merge(port, 1, Integer::sum);
        if (n >= REFRESH_TRIES) {
            FAILS.remove(port);
            h.mic_rtsim$markMirrorFresh();
        }
        if (refreshFailLogs++ < 8)
            MicRtsim.LOGGER.warn("[mic_rtsim_create] IND_WRITE_PACKAGE: the postbox's chunk at {} was not read ({}, {} of {}): the offline "
                    + "buffer is written as create_tracks.dat left it (docs/choices-journal-barrier.md RP-206)", pos, why, n, REFRESH_TRIES);
    }

    /** Switches the restore's carry and the chunk read off (true), as before them, or back on. */
    public static void chunkChecksForTest(boolean off) {
        chunkChecksOff = off;
    }

    /** Ops carried at restores; writes held for chunk reads; reads done; buffers refreshed; carried ops found. */
    public static long[] chunkCountsForTest() {
        return new long[] {carriedAtRestore, refreshHolds, refreshes, refreshedFromChunk, carriedFound};
    }

    /** Null when the round trip works, else what broke. */
    private static String probe(HolderLookup.Provider registries) {
        try {
            GlobalPackagePort port = new GlobalPackagePort();
            if (!((Object) port instanceof PortRingHolder h))
                return "GlobalPackagePort does not implement PortRingHolder: the mixin config did not apply";
            h.mic_rtsim$bufferOps().add(PROBE_OP);
            h.mic_rtsim$postboxOps().add(PROBE_OP);
            // What TrackEdgePoint.write and SingleBlockEntityEdgePoint.write need besides the ports.
            GlobalStation station = new GlobalStation();
            station.id = UUID.randomUUID();
            station.setLocation(Couple.create(new TrackNodeLocation(0, 0, 0).in(Level.OVERWORLD),
                    new TrackNodeLocation(1, 0, 0).in(Level.OVERWORLD)), 0);
            station.blockEntityPos = BlockPos.ZERO;
            station.blockEntityDimension = Level.OVERWORLD;
            // A port off the origin: the read must key it by its own "Pos", which a zero would not tell apart.
            BlockPos at = new BlockPos(1, 2, 3);
            station.connectedPorts.put(at, port);
            DimensionPalette dimensions = new DimensionPalette();
            CompoundTag tag = new CompoundTag();
            station.write(tag, registries, dimensions);
            CompoundTag entry = tag.getList("Ports", Tag.TAG_COMPOUND).getCompound(0);
            long[] probe = {PROBE_OP};
            if (!Arrays.equals(entry.getLongArray(KEY), probe) || !Arrays.equals(entry.getLongArray(POSTBOX_KEY), probe))
                return "GlobalStation.write wrote no " + KEY + ": its injection did not apply";
            GlobalStation back = new GlobalStation();
            probing = true;
            try {
                back.read(tag, registries, false, dimensions);
            } finally {
                probing = false;
            }
            GlobalPackagePort read = back.connectedPorts.get(at);
            if (read == null || !wroteToBuffer(read, PROBE_OP) || !wroteToPostbox(read, PROBE_OP))
                return "GlobalStation.read did not read " + KEY + ": its injection did not apply";
            return null;
        } catch (RuntimeException | LinkageError e) {
            return "the probe threw " + e;
        }
    }

    // ---- GameTests ----------------------------------------------------------------------------

    /**
     * Switches the rings off (true) as if the mixins had not applied, or back to the probe (false). A change
     * probes again at the next {@link #available}, and logs again when off.
     */
    public static void fallbackForTest(boolean on) {
        if (fallbackForTest == on)
            return;
        fallbackForTest = on;
        available = null;
    }

    /** How many times the fallback was logged: once per probe that failed. */
    public static int fallbackLogsForTest() {
        return fallbackLogs;
    }
}
