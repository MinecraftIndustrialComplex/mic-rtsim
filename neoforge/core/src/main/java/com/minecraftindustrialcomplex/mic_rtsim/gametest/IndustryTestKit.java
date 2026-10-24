package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Districts;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryConfig;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.common.util.BlockSnapshot;
import net.neoforged.neoforge.event.level.BlockEvent;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;
import java.util.function.Predicate;

/**
 * Industry GameTest helpers every plugin shares (the PowerGrid, Destroy, Ore Excavation, Diesel, Ender
 * Transmission, climate and radar plugins, and the core's threat tests): the session's engine, whichever
 * it is, the records it received, and industry commands scripted on either engine. The stub writes a
 * scripted command with its own dictionary; the native engine takes it as an INDUSTRY_COMMAND order with
 * Java's dictionary ids (choice W17). The Create plugin keeps its own {@code IndustryKit}; this is the
 * same, for projects without Create.
 */
public final class IndustryTestKit {
    private IndustryTestKit() {}

    /** Engine op ids: top bit clear, namespace 0x49 in bits 56 to 62 (F4), a fresh range per server start. */
    private static final AtomicLong NEXT_OP = new AtomicLong((0x49L << 56) | ((System.nanoTime() & 0xFFFF_FFFFL) << 12));

    public static long engineOp() {
        return NEXT_OP.getAndIncrement();
    }

    public static ScriptedEngine engine() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        if (!(s.engine() instanceof ScriptedEngine e))
            throw new GameTestAssertException("the session's engine takes no scripted orders: " + s.engine().describe());
        return e;
    }

    public static boolean nativeEngine() {
        return engine() instanceof NativeEngine;
    }

    public static String engineName() {
        return nativeEngine() ? "native" : "stub";
    }

    public static Industry ind() {
        Industry i = IndustryApi.current();
        if (i == null)
            throw new GameTestAssertException("no industry module");
        return i;
    }

    public static void check(String what, boolean ok) {
        if (!ok)
            throw new GameTestAssertException(what);
    }

    /**
     * What a detection's per-tick budget bounds (Districts.Rescan.step): a tick stops at the first chunk that takes it
     * to the budget, so its spend is at most the budget less one plus the costliest look at one chunk. Both are read
     * from the detection, so nothing here depends on the load or on what earlier tests left in the yard's chunks. That
     * the costliest look was one chunk (and not a district or a batch) is checked against the chunks themselves: a look
     * costs one, plus the chunk's block entities, plus at most a candidate inventory for each (the test machines add
     * no more), so it is at most {@code 1 + 2 * (the most block entities one of these chunks holds)}.
     */
    public static void checkTickBudget(Districts ds, ServerLevel level, int budget, BlockPos... in) {
        long longest = ds.lastRescanLongestLooked(), item = ds.lastRescanLongestItem();
        check("no tick looked at more than the budget less one and a chunk (" + (budget - 1 + item) + "): " + longest,
                longest <= budget - 1 + item);
        int biggest = 0;
        for (BlockPos p : in)
            biggest = Math.max(biggest, level.getChunkAt(p).getBlockEntities().size());
        check("the costliest look was one chunk (at most " + (1 + 2 * biggest) + "): " + item, item <= 1 + 2L * biggest);
    }

    public static void record(String test, String what, Object value) {
        MicRtsim.LOGGER.info("[gametest] {}: {} = {}", test, what, value);
    }

    /** Short sweeps and detection right after an edit. */
    public static void fastConfig() {
        IndustryConfig.Test.set(IndustryConfig.SWEEP_PERIOD, 20);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_INTERVAL, 1);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_BLOCK_ENTITIES_PER_TICK, null); // a budget test sets its own
        IndustryConfig.Test.set(IndustryConfig.RESCAN_SLICE_MICROS, null); // and a time-limit test its own
    }

    /** Forgets every district of the level (each test has its own batch, so none runs beside it). */
    public static void forgetAll(GameTestHelper helper) {
        BlockPos a = helper.absolutePos(BlockPos.ZERO), b = helper.absolutePos(new BlockPos(47, 0, 47));
        ind().districts().forgetAllForTest(helper.getLevel());
        ind().districts().forgetArea(helper.getLevel(), (Math.min(a.getX(), b.getX()) >> 4) - 1, (Math.min(a.getZ(), b.getZ()) >> 4) - 1,
                (Math.max(a.getX(), b.getX()) >> 4) + 1, (Math.max(a.getZ(), b.getZ()) >> 4) + 1);
        ind().threats().forgetAll();
    }

    /** What a player placing the block at {@code abs} fires (with a server player: a NeoForge FakePlayer is a machine). */
    public static void playerPlaced(GameTestHelper helper, BlockPos abs) {
        ServerLevel level = helper.getLevel();
        NeoForge.EVENT_BUS.post(new BlockEvent.EntityPlaceEvent(BlockSnapshot.create(level.dimension(), level, abs),
                level.getBlockState(abs.below()), FakePlayers.editor(level)));
    }

    public static int floorY(GameTestHelper helper) {
        return helper.absolutePos(BlockPos.ZERO).getY() + 1;
    }

    /** A position {@code dx, dz} into the first whole chunk inside the structure, so blocks near it share a chunk. */
    public static BlockPos inChunk(GameTestHelper helper, int dx, int dz) {
        BlockPos o = helper.absolutePos(BlockPos.ZERO);
        return new BlockPos((((o.getX() >> 4) + 1) << 4) + dx, floorY(helper), (((o.getZ() >> 4) + 1) << 4) + dz);
    }

    public static net.minecraft.world.level.block.Block block(String id) {
        return net.minecraft.core.registries.BuiltInRegistries.BLOCK.get(net.minecraft.resources.ResourceLocation.parse(id));
    }

    public static void set(GameTestHelper helper, BlockPos abs, net.minecraft.world.level.block.state.BlockState state) {
        helper.getLevel().setBlock(abs, state, 3);
    }

    /** The goods of a record's Entry16 list at {@code at}, {@code n} entries, by Java's GOOD names. */
    public static Map<String, Long> entries(ByteBuffer r, int at, int n) {
        Map<String, Long> out = new java.util.TreeMap<>();
        for (int i = 0; i < n; i++)
            out.merge(javaName(r.getInt(at + i * IndRecords.ENTRY16)), r.getLong(at + i * IndRecords.ENTRY16 + 8), Long::sum);
        return out;
    }

    public static District districtAt(GameTestHelper helper, BlockPos abs) {
        return ind().districtAt(helper.getLevel(), abs);
    }

    public static long chunk(BlockPos abs) {
        return ChunkPos.asLong(abs.getX() >> 4, abs.getZ() >> 4);
    }

    // ---- received records -----------------------------------------------------------------------

    public static List<ByteBuffer> received(int kind, Predicate<ByteBuffer> p) {
        List<ByteBuffer> out = new ArrayList<>();
        for (var r : engine().received(kind))
            if (!r.skipped() && p.test(r.view()))
                out.add(r.view());
        return out;
    }

    /** Records of one kind whose u64 at {@code offset} is {@code value}. */
    public static List<ByteBuffer> received(int kind, int offset, long value) {
        return received(kind, v -> v.capacity() >= offset + 8 && v.getLong(offset) == value);
    }

    public static String javaName(int id) {
        return engine().javaDictName(id);
    }

    public static List<Integer> commandResults(long opId) {
        List<Integer> out = new ArrayList<>();
        for (var r : engine().received(Obs.COMMAND_RESULT))
            if (!r.skipped() && r.view().getLong(Obs.CommandResult.OP_ID) == opId)
                out.add(r.aux());
        return out;
    }

    public static int aux(ByteBuffer record) {
        return record.getInt(Layout.R_AUX);
    }

    // ---- scripting industry commands ------------------------------------------------------------

    /** Scripts one industry command on the session's engine (one record per call). */
    public static void script(Consumer<StubEngine.CommandOut> command) {
        ScriptedEngine e = engine();
        if (e instanceof StubEngine stub) {
            stub.script(command);
            return;
        }
        if (!(e instanceof NativeEngine n))
            throw new GameTestAssertException("no scripted industry commands on " + e.describe());
        RecordOut out = new RecordOut();
        command.accept(out);
        if (out.b == null)
            throw new GameTestAssertException("the script wrote no record");
        n.industryCommand(out.names, out.b.array());
    }

    private static final class RecordOut implements StubEngine.CommandOut {
        private final List<NativeEngine.DictEntry> names = new ArrayList<>();
        private ByteBuffer b;

        @Override
        public int dict(int dictKind, String name) {
            int id = RtsimSession.current().dictId(dictKind, name);
            names.add(new NativeEngine.DictEntry(dictKind, id, name));
            return id;
        }

        @Override
        public int reserve(int kind, int size, int aux) {
            if (b != null)
                throw new IllegalStateException("one record per INDUSTRY_COMMAND order");
            int aligned = (size + 7) & ~7;
            b = ByteBuffer.allocate(aligned).order(Layout.ORDER);
            b.putShort(Layout.R_KIND, (short) kind);
            b.putShort(Layout.R_SIZE, (short) aligned);
            b.putInt(Layout.R_AUX, aux);
            return 0;
        }

        @Override
        public ByteBuffer buffer() {
            return b;
        }
    }

    /** One reconcile delta: inventory (DEPOSIT_INVENTORY for a deposit), good, position, delta. */
    public record Delta(int inventory, String good, BlockPos pos, long delta) {}

    /** Scripts an IND_CHUNK_RECONCILE with deltas only. */
    public static void reconcile(long opId, long district, int epoch, ServerLevel level, int cx, int cz, List<Delta> deltas) {
        String dim = level.dimension().location().toString();
        script(out -> {
            int dimId = out.dict(Obs.DICT_DIMENSION, dim);
            int[] goods = new int[deltas.size()];
            for (int i = 0; i < deltas.size(); i++)
                goods[i] = out.dict(IndRecords.DICT_GOOD, deltas.get(i).good());
            int at = out.reserve(IndRecords.IND_CHUNK_RECONCILE, IndRecords.ChunkReconcile.size(deltas.size(), 0), deltas.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + IndRecords.ChunkReconcile.OP_ID, opId);
            b.putLong(at + IndRecords.ChunkReconcile.DISTRICT, district);
            b.putInt(at + IndRecords.ChunkReconcile.EPOCH, epoch);
            b.putInt(at + IndRecords.ChunkReconcile.DIM, dimId);
            b.putInt(at + IndRecords.ChunkReconcile.CHUNK_X, cx);
            b.putInt(at + IndRecords.ChunkReconcile.CHUNK_Z, cz);
            b.putInt(at + IndRecords.ChunkReconcile.DESTROY_COUNT, 0);
            for (int i = 0; i < deltas.size(); i++) {
                int e = at + IndRecords.ChunkReconcile.DELTAS + i * IndRecords.ChunkReconcile.DELTA_SIZE;
                Delta d = deltas.get(i);
                b.putInt(e + IndRecords.ChunkReconcile.D_INVENTORY, d.inventory());
                b.putInt(e + IndRecords.ChunkReconcile.D_GOOD, goods[i]);
                b.putInt(e + IndRecords.ChunkReconcile.D_X, d.pos().getX());
                b.putInt(e + IndRecords.ChunkReconcile.D_Y, d.pos().getY());
                b.putInt(e + IndRecords.ChunkReconcile.D_Z, d.pos().getZ());
                b.putLong(e + IndRecords.ChunkReconcile.D_DELTA, d.delta());
            }
        });
    }

    /** Scripts an IND_DEPOSIT_DELTA. */
    public static void depositDelta(long opId, int plugin, ServerLevel level, int cx, int cz, String good, long delta) {
        String dim = level.dimension().location().toString();
        script(out -> {
            int dimId = out.dict(Obs.DICT_DIMENSION, dim);
            int g = out.dict(IndRecords.DICT_GOOD, good);
            int at = out.reserve(IndRecords.IND_DEPOSIT_DELTA, IndRecords.DepositDelta.SIZE, plugin);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + IndRecords.DepositDelta.OP_ID, opId);
            b.putInt(at + IndRecords.DepositDelta.DIM, dimId);
            b.putInt(at + IndRecords.DepositDelta.CHUNK_X, cx);
            b.putInt(at + IndRecords.DepositDelta.CHUNK_Z, cz);
            b.putInt(at + IndRecords.DepositDelta.GOOD, g);
            b.putLong(at + IndRecords.DepositDelta.DELTA, delta);
        });
    }

    /** Scripts an IND_WRITE_PACKAGE: {@code count} packages of one content each, to a postbox. */
    public static void writePackage(long opId, long district, int inventory, BlockPos pos, String address, int count, Map<String, Long> contents) {
        List<Map.Entry<String, Long>> list = new ArrayList<>(contents.entrySet());
        script(out -> {
            int addr = out.dict(IndRecords.DICT_NAME, address);
            int[] ids = new int[list.size()];
            for (int i = 0; i < list.size(); i++)
                ids[i] = out.dict(IndRecords.DICT_GOOD, list.get(i).getKey());
            int at = out.reserve(IndRecords.IND_WRITE_PACKAGE, IndRecords.WritePackage.size(list.size()), list.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + IndRecords.WritePackage.OP_ID, opId);
            b.putLong(at + IndRecords.WritePackage.DISTRICT, district);
            b.putInt(at + IndRecords.WritePackage.INVENTORY, inventory);
            b.putInt(at + IndRecords.WritePackage.X, pos.getX());
            b.putInt(at + IndRecords.WritePackage.Y, pos.getY());
            b.putInt(at + IndRecords.WritePackage.Z, pos.getZ());
            b.putInt(at + IndRecords.WritePackage.ADDRESS, addr);
            b.putInt(at + IndRecords.WritePackage.COUNT, count);
            for (int i = 0; i < list.size(); i++) {
                int e = at + IndRecords.WritePackage.ENTRIES + i * IndRecords.ENTRY16;
                b.putInt(e, ids[i]);
                b.putLong(e + 8, list.get(i).getValue());
            }
        });
    }

    /** Scripts an IND_WRITE_CHAIN: {@code count} packages of one content each, to a package store's frogport. */
    public static void writeChain(long opId, long store, long district, int inventory, int frogport, BlockPos pos, int count,
            Map<String, Long> contents) {
        List<Map.Entry<String, Long>> list = new ArrayList<>(contents.entrySet());
        script(out -> {
            int[] ids = new int[list.size()];
            for (int i = 0; i < list.size(); i++)
                ids[i] = out.dict(IndRecords.DICT_GOOD, list.get(i).getKey());
            int at = out.reserve(IndRecords.IND_WRITE_CHAIN, IndRecords.WriteChain.size(list.size()), list.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + IndRecords.WriteChain.OP_ID, opId);
            b.putLong(at + IndRecords.WriteChain.STORE, store);
            b.putLong(at + IndRecords.WriteChain.DISTRICT, district);
            b.putInt(at + IndRecords.WriteChain.INVENTORY, inventory);
            b.putInt(at + IndRecords.WriteChain.FROGPORT, frogport);
            b.putInt(at + IndRecords.WriteChain.X, pos.getX());
            b.putInt(at + IndRecords.WriteChain.Y, pos.getY());
            b.putInt(at + IndRecords.WriteChain.Z, pos.getZ());
            b.putInt(at + IndRecords.WriteChain.COUNT, count);
            for (int i = 0; i < list.size(); i++) {
                int e = at + IndRecords.WriteChain.ENTRIES + i * IndRecords.ENTRY16;
                b.putInt(e, ids[i]);
                b.putLong(e + 8, list.get(i).getValue());
            }
        });
    }
}
