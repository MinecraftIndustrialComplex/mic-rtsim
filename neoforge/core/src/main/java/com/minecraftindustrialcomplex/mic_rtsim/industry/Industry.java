package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.config.Module;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import it.unimi.dsi.fastutil.ints.Int2LongOpenHashMap;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;

import java.nio.ByteBuffer;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.function.BiFunction;

/**
 * The industry module of one bridge session (C15 to C19, INDUSTRY.md, docs/industry-boundary.md): the
 * district index and detection (I2), the boundary stock sampler (I3), ports, the reconciler (I8) and the
 * operator diagnostics (I18). Mod-agnostic (D4): plugins add Create and the rest through
 * {@link IndustryApi}. Server thread only; nothing here waits for the engine.
 *
 * <p>Per tick ({@link #endOfTick}, called by the session after it applied commands and before it hands
 * over): process loads and edits, detect districts after edits, track load states, sweep stock under
 * the per-tick quota, apply held reconciles whose chunks are loaded.
 */
public final class Industry {

    /** Called once from the core mod's constructor. */
    public static void register(IEventBus modBus, ModContainer container) {
        IndustryAttachments.register(modBus);
        IndustryConfig.register(container);
        IndustryEvents.register();
        ThreatConfig.register(container);
        ThreatEvents.register();
    }

    final RtsimSession session;
    final MinecraftServer server;
    final BoundarySession boundary;
    final IndustryData data;
    final Goods goods;
    final Districts districts;
    final Sampler sampler;
    final Reconciler reconciler;
    final PlayerPorts ports;
    final DistrictDiagnostics diagnostics;
    final Conditions conditions;
    final Threats threats;
    private final Map<String, Integer> goodIds = new HashMap<>();
    private long ticks;
    private boolean started;
    /** True once the module has been on and then switched off: switching on again announces everything (U10). */
    private boolean wasOff;

    // counters for /rtsim district perf and the GameTests
    long commandsApplied;
    long commandsFailed;

    public Industry(RtsimSession session, MinecraftServer server) {
        this.session = session;
        this.server = server;
        this.boundary = session.boundary();
        this.data = IndustryData.get(server);
        this.goods = new Goods(data, server.registryAccess());
        this.districts = new Districts(this);
        this.sampler = new Sampler(this);
        this.reconciler = new Reconciler(this);
        this.ports = new PlayerPorts(this);
        this.diagnostics = new DistrictDiagnostics(this);
        this.conditions = new Conditions(this);
        this.threats = new Threats(this);
    }

    /** The industry module switch (C25): off restores vanilla, nothing is observed or applied. */
    public boolean enabled() {
        return RtsimConfig.on(Module.INDUSTRY);
    }

    public MinecraftServer server() {
        return server;
    }

    public Goods goods() {
        return goods;
    }

    public IndustryData data() {
        return data;
    }

    public long gameTime() {
        return session.gameTime();
    }

    // ---- the tick -------------------------------------------------------------------------------

    public void endOfTick(long now) {
        // Only the server's own session observes the world: GameTests run private sessions next to it.
        if (RtsimSession.current() != session)
            return;
        if (!enabled()) {
            // U10: a switched-off industry sends nothing. What it announced is not known to the engine
            // any more once it runs again, so the next start announces everything as at a session start.
            if (started) {
                started = false;
                wasOff = true;
            }
            return;
        }
        if (!started) {
            started = true;
            boolean again = wasOff;
            wasOff = false;
            if (again)
                forgetAnnounced();
            districts.sessionStart(now);
            if (again)
                for (IndustryPlugin p : IndustryApi.PLUGINS) {
                    try {
                        p.announce(this);
                    } catch (RuntimeException e) {
                        fail("plugin announce " + p.getClass().getSimpleName(), e);
                    }
                }
        }
        IndustryEvents.drain(this, now);
        ports.tick(now);
        districts.tick(now, ticks);
        reconciler.tick(now);
        sampler.tick(now);
        conditions.tick(now);
        threats.tick(now, ticks);
        for (IndustryPlugin p : IndustryApi.PLUGINS) {
            try {
                p.tick(this, now);
            } catch (RuntimeException e) {
                fail("plugin tick " + p.getClass().getSimpleName(), e);
            }
        }
        ticks++;
    }

    /**
     * What the engine is told again when the module comes back on (U10): every district with its chunks,
     * inventories and stations, the load states, the calendar, the surveys, the pending results chunks carry.
     * Plugins add theirs through {@link IndustryPlugin#announce}.
     */
    private void forgetAnnounced() {
        districts.forgetAnnounced();
        conditions.resend();
        threats.forgetAnnounced();
        reconciler.forgetAnnounced();
    }

    /** An industry command from the engine. Returns false to leave it for a later tick. */
    public boolean accept(ByteBuffer b, int at, int kind, int size, int aux, BiFunction<Integer, Integer, String> engineDict) {
        this.engineDict = engineDict;
        long cmdSeq = b.getLong(at + com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd.CMD_SEQ);
        if (!enabled())
            return true; // U10: a switched-off industry answers nothing; the engine sends a kept command again once it is on

        switch (kind) {
            case IndRecords.IND_CHUNK_RECONCILE -> {
                return reconciler.accept(b, at, size, aux, cmdSeq, gameTime());
            }
            case IndRecords.IND_DEPOSIT_DELTA -> {
                depositDelta(b, at, size, aux, cmdSeq);
                return true;
            }
            default -> {
                IndustryApi.CommandHandler h = IndustryApi.COMMANDS.get(kind);
                if (h != null)
                    return h.accept(this, b, at, size, aux, cmdSeq);
                // No plugin applies it here (IND_CHANNEL_DELTA since I13 was corrected; a command of a mod not loaded): say so.
                if (size >= 24) {
                    commandResult(cmdSeq, b.getLong(at + 16), Obs.CommandResult.FAILED);
                    commandsFailed++;
                }
                return true;
            }
        }
    }

    private BiFunction<Integer, Integer, String> engineDict = (k, id) -> null;

    /** A name in the engine's dictionary for this session (GOOD, NAME, BLOCK_STATE, dimension, item). */
    public String engineName(int dictKind, int id) {
        return engineDict.apply(dictKind, id);
    }

    /** The level an engine dimension id names, or null. */
    public ServerLevel engineLevel(int dimId) {
        String name = engineName(Obs.DICT_DIMENSION, dimId);
        if (name == null)
            return null;
        ResourceLocation loc = ResourceLocation.tryParse(name);
        return loc == null ? null : server.getLevel(ResourceKey.create(Registries.DIMENSION, loc));
    }

    // ---- writing observations -------------------------------------------------------------------

    /** Starts a journaled record; fill from 24 and {@link #commit} it. */
    public ByteBuffer journal(int kind, int size, int aux) {
        return boundary.beginJournaled(kind, size, aux, gameTime());
    }

    public long commit(ByteBuffer record) {
        return boundary.commit(record);
    }

    /** Reserves a latest-wins record in the open buffer; -1 when dropped for space. Write through {@link #open()}. */
    public int latest(int kind, int size, int aux) {
        return boundary.direct(kind, size, aux, gameTime());
    }

    public ByteBuffer open() {
        return boundary.openBuffer();
    }

    /** Journals a COMMAND_RESULT for an op id the engine sent; returns its sequence. */
    public long commandResult(long cmdSeq, long opId, int status) {
        ByteBuffer r = boundary.beginJournaled(Obs.COMMAND_RESULT, Obs.CommandResult.SIZE, status, gameTime());
        r.putLong(Obs.CommandResult.CMD_SEQ, cmdSeq);
        r.putLong(Obs.CommandResult.OP_ID, opId);
        if (status == Obs.CommandResult.APPLIED)
            commandsApplied++;
        return boundary.commit(r);
    }

    /** True when Java already applied this engine op id and the engine may not know yet. */
    public boolean opApplied(long opId) {
        return boundary.journal().commandOpApplied(opId);
    }

    /** Runs {@code then} once the engine acknowledges sequence {@code seq} in this session. */
    public void whenAcked(long seq, Runnable then) {
        session.afterAck(seq, then);
    }

    public int dimId(ResourceKey<Level> dim) {
        return boundary.dictId(Obs.DICT_DIMENSION, dim.location().toString());
    }

    public int nameId(String name) {
        return boundary.dictId(IndRecords.DICT_NAME, name == null ? "" : name);
    }

    public int goodId(String name) {
        Integer id = goodIds.get(name);
        if (id == null) {
            id = boundary.dictId(IndRecords.DICT_GOOD, name);
            goodIds.put(name, id);
        }
        return id;
    }

    /** Counts a stack under its good, or under its contents when a plugin expands it (a package). */
    public void countStack(ItemStack stack, long count, GoodsCounter out) {
        for (IndustryPlugin p : IndustryApi.PLUGINS) {
            if (p.expand(stack, (inner, n) -> countStack(inner, n * count, out)))
                return;
        }
        out.add(goods.name(stack), count);
    }

    // ---- deposits and level-store commands (overnight agent 14) ---------------------------------

    /**
     * Applies a deposit delta through the plugin that owns the good (I11). Returns what was applied (the
     * sign of {@code delta}); 0 when no plugin owns it, so the engine keeps it owed.
     */
    public long applyDeposit(ServerLevel level, int chunkX, int chunkZ, String good, long delta) {
        if (good == null || delta == 0)
            return 0;
        for (IndustryPlugin p : IndustryApi.PLUGINS) {
            try {
                Long applied = p.applyDeposit(level, chunkX, chunkZ, good, delta);
                if (applied != null)
                    return applied;
            } catch (RuntimeException e) {
                fail("applying " + delta + " of deposit " + good + " in " + chunkX + ", " + chunkZ, e);
                return 0;
            }
        }
        return 0;
    }

    /**
     * IND_DEPOSIT_DELTA (docs/industry-boundary.md §4.3): a deposit in level SavedData (Diesel's
     * {@code OilChunksSavedData}), applied at once, loaded or not. The store and the journal reach disk at
     * the same save, so the journal's applied set is the dedup (as H17 for train cargo). Answered APPLIED
     * when a plugin owns the good, FAILED when none does.
     */
    private void depositDelta(ByteBuffer b, int at, int size, int plugin, long cmdSeq) {
        if (size < IndRecords.DepositDelta.SIZE) {
            if (size >= 24)
                commandResult(cmdSeq, b.getLong(at + IndRecords.DepositDelta.OP_ID), Obs.CommandResult.FAILED);
            return;
        }
        long opId = b.getLong(at + IndRecords.DepositDelta.OP_ID);
        if (opApplied(opId)) {
            commandResult(cmdSeq, opId, Obs.CommandResult.DUPLICATE_OP);
            return;
        }
        ServerLevel level = engineLevel(b.getInt(at + IndRecords.DepositDelta.DIM));
        String good = engineName(IndRecords.DICT_GOOD, b.getInt(at + IndRecords.DepositDelta.GOOD));
        long delta = b.getLong(at + IndRecords.DepositDelta.DELTA);
        int cx = b.getInt(at + IndRecords.DepositDelta.CHUNK_X), cz = b.getInt(at + IndRecords.DepositDelta.CHUNK_Z);
        Long applied = null;
        if (level != null && good != null) {
            for (IndustryPlugin p : IndustryApi.PLUGINS) {
                try {
                    applied = p.applyDeposit(level, cx, cz, good, delta);
                } catch (RuntimeException e) {
                    fail("IND_DEPOSIT_DELTA " + delta + " of " + good, e);
                    applied = null;
                    break;
                }
                if (applied != null)
                    break;
            }
        }
        depositDeltas++;
        if (applied == null) {
            commandsFailed++;
            commandResult(cmdSeq, opId, Obs.CommandResult.FAILED);
        } else {
            lastDepositApplied = applied;
            commandResult(cmdSeq, opId, Obs.CommandResult.APPLIED);
        }
    }

    long depositDeltas;
    long lastDepositApplied;

    public long lastDepositApplied() {
        return lastDepositApplied;
    }

    /**
     * Journals a COMMAND_RESULT and carries it in {@code chunk} (the {@code mic_rtsim:pending} record of
     * docs/boundary.md §3.1) until the engine acknowledges it: for a command applied into a block entity,
     * whose chunk can reach disk before the journal does. A later session journals it again at the chunk's
     * load, and {@link #chunkCarriesOp} refuses the same op id meanwhile.
     */
    public void commandResultInChunk(net.minecraft.world.level.chunk.LevelChunk chunk, long cmdSeq, long opId, int status) {
        ByteBuffer r = boundary.beginJournaled(Obs.COMMAND_RESULT, Obs.CommandResult.SIZE, status, gameTime());
        r.putLong(Obs.CommandResult.CMD_SEQ, cmdSeq);
        r.putLong(Obs.CommandResult.OP_ID, opId);
        if (status == Obs.CommandResult.APPLIED)
            commandsApplied++;
        long seq = boundary.commit(r);
        com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords old =
                chunk.getData(com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments.PENDING);
        chunk.setData(com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments.PENDING, old.plus(java.util.List.of(r.array().clone())));
        chunk.setUnsaved(true);
        ResourceKey<Level> dim = chunk.getLevel().dimension();
        ChunkPos pos = chunk.getPos();
        whenAcked(seq, () -> {
            ServerLevel level = server.getLevel(dim);
            net.minecraft.world.level.chunk.LevelChunk c = level == null ? null : level.getChunkSource().getChunkNow(pos.x, pos.z);
            if (c == null)
                return; // journaled again when it loads
            com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords p =
                    c.getExistingDataOrNull(com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments.PENDING);
            if (p == null)
                return;
            java.util.List<byte[]> keep = new java.util.ArrayList<>();
            for (byte[] rec : p.records())
                if (ByteBuffer.wrap(rec).order(com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout.ORDER).getLong(Obs.CommandResult.OP_ID) != opId)
                    keep.add(rec);
            if (keep.isEmpty())
                c.removeData(com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments.PENDING);
            else
                c.setData(com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments.PENDING,
                        new com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords(java.util.List.copyOf(keep)));
            c.setUnsaved(true);
        });
    }

    /** True when {@code chunk} still carries the COMMAND_RESULT of this op id (applied into it, not yet acknowledged). */
    public boolean chunkCarriesOp(net.minecraft.world.level.chunk.LevelChunk chunk, long opId) {
        return RtsimSession.chunkCarriesOp(chunk, opId);
    }

    public Conditions conditions() {
        return conditions;
    }

    public Threats threats() {
        return threats;
    }

    // ---- lookups --------------------------------------------------------------------------------

    public District district(long id) {
        return data.districts.get(id);
    }

    public District districtAt(Level level, BlockPos pos) {
        return districts.at(level.dimension(), ChunkPos.asLong(pos.getX() >> 4, pos.getZ() >> 4));
    }

    public District districtAt(ResourceKey<Level> dim, long chunk) {
        return districts.at(dim, chunk);
    }

    public Districts districts() {
        return districts;
    }

    public Sampler sampler() {
        return sampler;
    }

    public Reconciler reconciler() {
        return reconciler;
    }

    public DistrictDiagnostics diagnostics() {
        return diagnostics;
    }

    public PlayerPorts ports() {
        return ports;
    }

    public ServerLevel level(ResourceKey<Level> dim) {
        return server.getLevel(dim);
    }

    /**
     * Journals an IND_PORT: goods that crossed {@code d}'s boundary while it was loaded, + into the
     * district. Split over several records when it does not fit one. Entries that net to zero are left out.
     */
    public void port(District d, int portKind, int inventory, long counterpart, long opId, GoodsCounter goods) {
        int[] ids = goods.nonZeroIds();
        if (ids.length == 0)
            return;
        for (int from = 0; from < ids.length; from += IndRecords.Port.MAX_ENTRIES) {
            int n = Math.min(IndRecords.Port.MAX_ENTRIES, ids.length - from);
            ByteBuffer r = journal(IndRecords.IND_PORT, IndRecords.Port.size(n), n);
            r.putLong(IndRecords.Port.DISTRICT, d.id);
            r.putInt(IndRecords.Port.PORT_KIND, portKind);
            r.putInt(IndRecords.Port.INVENTORY, inventory);
            r.putLong(IndRecords.Port.COUNTERPART, counterpart);
            r.putLong(IndRecords.Port.OP_ID, opId);
            for (int i = 0; i < n; i++) {
                int e = IndRecords.Port.ENTRIES + i * IndRecords.ENTRY16;
                r.putInt(e, ids[from + i]);
                r.putLong(e + 8, goods.get(ids[from + i]));
            }
            commit(r);
        }
    }

    void fail(String what, Throwable t) {
        MicRtsim.LOGGER.error("[mic_rtsim] industry: {}", what, t);
    }

    /** Goods by GOOD dictionary id; amounts may be negative (deltas). */
    public final class GoodsCounter {
        private final Int2LongOpenHashMap counts = new Int2LongOpenHashMap();

        public void add(String goodName, long amount) {
            if (amount != 0)
                counts.addTo(goodId(goodName), amount);
        }

        public void addId(int id, long amount) {
            if (amount != 0)
                counts.addTo(id, amount);
        }

        public void addAll(GoodsCounter other, long sign) {
            for (Int2LongOpenHashMap.Entry e : other.counts.int2LongEntrySet())
                addId(e.getIntKey(), sign * e.getLongValue());
        }

        public long get(int id) {
            return counts.get(id);
        }

        public long get(String goodName) {
            return counts.get(goodId(goodName));
        }

        public int size() {
            return counts.size();
        }

        public boolean isEmpty() {
            return nonZeroIds().length == 0;
        }

        public void clear() {
            counts.clear();
        }

        /** The ids with a non-zero amount, ascending. */
        public int[] nonZeroIds() {
            int[] ids = counts.int2LongEntrySet().stream().filter(e -> e.getLongValue() != 0).mapToInt(Int2LongOpenHashMap.Entry::getIntKey).toArray();
            Arrays.sort(ids);
            return ids;
        }

        public long total() {
            long t = 0;
            for (long v : counts.values())
                t += v;
            return t;
        }
    }

    public GoodsCounter counter() {
        return new GoodsCounter();
    }
}
