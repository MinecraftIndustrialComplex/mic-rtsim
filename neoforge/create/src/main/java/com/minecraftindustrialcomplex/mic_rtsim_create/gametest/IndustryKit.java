package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryConfig;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.common.util.BlockSnapshot;
import net.neoforged.neoforge.event.level.BlockEvent;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Consumer;
import java.util.function.Predicate;

/**
 * Shared helpers for the industry GameTests ({@code ./gradlew :create:runGameTestServer}). The tests
 * drive the server's real bridge session, whose engine is a {@link ScriptedEngine}: the Rust engine
 * ({@link NativeEngine}, the default, with the industry engine behind it) or the {@link StubEngine}
 * ({@code -PrtsimEngine=stub}). A test scripts industry commands (as the industry engine would send
 * them, wire.rs's layouts: the stub writes them with its own dictionary, the native engine takes them as
 * INDUSTRY_COMMAND orders with Java's dictionary ids, choice W17), waits for the bridge to apply them on a
 * later tick, and reads the observations the engine received.
 *
 * <p>Every test has its own batch, so tests never run at once and one test's districts cannot merge with
 * another's; each starts by forgetting the industry of its structure's chunks.
 */
public final class IndustryKit {
    private IndustryKit() {}

    public static final String YARD = "yard";

    /** Engine op ids: top bit clear, namespace 0x49 in bits 56 to 62 (F4). */
    private static final AtomicLong NEXT_OP = new AtomicLong((0x49L << 56) | (System.nanoTime() & 0xFFFF_FFFFL) << 8);

    public static long engineOp() {
        return NEXT_OP.getAndIncrement();
    }

    /** The session's engine, whichever it is. */
    public static ScriptedEngine engine() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        if (!(s.engine() instanceof ScriptedEngine e))
            throw new GameTestAssertException("the session's engine takes no scripted orders: " + s.engine().describe());
        return e;
    }

    /** True when the session runs the native engine, whose industry engine answers on its own. */
    public static boolean nativeEngine() {
        return engine() instanceof NativeEngine;
    }

    public static StubEngine stub() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            throw new GameTestAssertException("no mic_rtsim session on the server");
        if (!(s.engine() instanceof StubEngine stub))
            throw new GameTestAssertException("the engine is not the stub: " + s.engine().describe());
        return stub;
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

    public static void record(String test, String what, Object value) {
        MicRtsim.LOGGER.info("[gametest] {}: {} = {}", test, what, value);
    }

    /** Test pacing: short sweeps and detection right after an edit. */
    public static void fastConfig() {
        IndustryConfig.Test.set(IndustryConfig.SWEEP_PERIOD, 20);
        IndustryConfig.Test.set(IndustryConfig.RESCAN_INTERVAL, 1);
        // The glass box: compiles finish within a tick or two and resend often enough for a short test.
        com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.Test.set(
                com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.COMPILE_SLICE_MICROS, 20_000);
        com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.Test.set(
                com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.RESEND_TICKS, 200);
        com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.Test.set(
                com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.FINGERPRINT_PER_TICK, 4096);
        com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.Test.set(
                com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig.SEND_BYTES_PER_TICK, 256 * 1024);
    }

    /** The first chunk border inside the yard along x: the absolute x of a chunk's first column. */
    public static int borderX(GameTestHelper helper) {
        int x0 = helper.absolutePos(BlockPos.ZERO).getX();
        return ((x0 >> 4) + 1) << 4;
    }

    public static int floorY(GameTestHelper helper) {
        return helper.absolutePos(BlockPos.ZERO).getY() + 1;
    }

    /** The middle row of the yard in z, absolute. */
    public static int midZ(GameTestHelper helper) {
        return helper.absolutePos(new BlockPos(0, 0, 16)).getZ();
    }

    /** Forgets the industry of the yard's chunks (and one ring around them). */
    public static void forget(GameTestHelper helper) {
        TrainWatch.reportUnwatchedForTest(true); // a test that switched the report off and failed must not leave it off
        BlockPos a = helper.absolutePos(BlockPos.ZERO), b = helper.absolutePos(new BlockPos(33, 0, 33));
        int minCx = Math.min(a.getX(), b.getX()) >> 4, maxCx = Math.max(a.getX(), b.getX()) >> 4;
        int minCz = Math.min(a.getZ(), b.getZ()) >> 4, maxCz = Math.max(a.getZ(), b.getZ()) >> 4;
        // Load the ring around the yard first: an unloaded chunk would keep an earlier test's seeds on disk and
        // bring them back when it loads mid-test.
        for (int cx = minCx - 1; cx <= maxCx + 1; cx++)
            for (int cz = minCz - 1; cz <= maxCz + 1; cz++)
                helper.getLevel().getChunk(cx, cz);
        ind().districts().forgetAllForTest(helper.getLevel());
        ind().districts().forgetArea(helper.getLevel(), minCx - 1, minCz - 1, maxCx + 1, maxCz + 1);
        // The marker machines playerPlaced left above an earlier test's blocks would seed this one.
        ServerLevel level = helper.getLevel();
        net.minecraft.world.level.block.Block marker = com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines.MARKER.get();
        int y0 = floorY(helper);
        for (int cx = minCx - 1; cx <= maxCx + 1; cx++)
            for (int cz = minCz - 1; cz <= maxCz + 1; cz++) {
                if (level.getChunkSource().getChunkNow(cx, cz) == null)
                    continue;
                // Machines, stations and shafts an earlier test left on the same ground (the yard template does not
                // clear them): they would join this test's districts as members, and a station is an anchor (DS-10).
                for (net.minecraft.world.level.block.entity.BlockEntity be : new java.util.ArrayList<>(level.getChunk(cx, cz).getBlockEntities().values()))
                    if (be.getBlockPos().getY() >= y0 - 1 && be.getBlockPos().getY() < y0 + 16)
                        level.removeBlock(be.getBlockPos(), false);
                for (int x = 0; x < 16; x++)
                    for (int z = 0; z < 16; z++)
                        for (int up = 8; up < 14; up++) {
                            BlockPos m = new BlockPos(cx * 16 + x, y0 + up, cz * 16 + z);
                            if (level.getBlockState(m).is(marker))
                                level.setBlock(m, net.minecraft.world.level.block.Blocks.AIR.defaultBlockState(), 3);
                        }
            }
    }

    /** Sets a block at an absolute position. */
    public static void set(GameTestHelper helper, BlockPos abs, BlockState state) {
        helper.getLevel().setBlock(abs, state, 3);
    }

    /** What a player placing the block at {@code abs} fires (BlockEvent.EntityPlaceEvent), with a server player (a NeoForge FakePlayer is a machine). */
    public static void playerPlaced(GameTestHelper helper, BlockPos abs) {
        playerPlacedBare(helper, abs);
        ServerLevel level = helper.getLevel();
        // DS-10: storage and vanilla blocks never seed a district. A block that is no seed itself (a chest, a vault, a
        // hopper) gets the core's marker machine in the same chunk, high above the test, so the district it stands for
        // still exists; the marker has no inventory and no ticker, so it changes nothing else.
        // Not while the industry module is off (a test of the switch), and not into a chunk that already has a seed
        // (its machine, or an earlier marker): one is enough, and a second would be one more IND_EDIT.
        Industry industry = com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi.current();
        if (industry == null)
            return;
        net.minecraft.world.level.block.entity.BlockEntity be = level.getBlockEntity(abs);
        if (be != null && industry.districts().seed(level, be))
            return;
        if (level.getChunkAt(abs).getData(com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryAttachments.SEEDS.get()).positions().length > 0)
            return;
        for (int up = 8; up < 14; up++) {
            BlockPos m = abs.above(up);
            if (!level.getBlockState(m).isAir())
                continue;
            level.setBlock(m, com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines.MARKER.get().defaultBlockState(), 3);
            NeoForge.EVENT_BUS.post(new BlockEvent.EntityPlaceEvent(BlockSnapshot.create(level.dimension(), level, m),
                    level.getBlockState(m.below()), com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.editor(level)));
            return;
        }
    }

    /** The place event alone, with no marker machine: for the tests of the seed rule itself. */
    public static void playerPlacedBare(GameTestHelper helper, BlockPos abs) {
        ServerLevel level = helper.getLevel();
        NeoForge.EVENT_BUS.post(new BlockEvent.EntityPlaceEvent(BlockSnapshot.create(level.dimension(), level, abs),
                level.getBlockState(abs.below()), com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers.editor(level)));
    }

    public static District districtAt(GameTestHelper helper, BlockPos abs) {
        return ind().districtAt(helper.getLevel(), abs);
    }

    public static long chunk(BlockPos abs) {
        return ChunkPos.asLong(abs.getX() >> 4, abs.getZ() >> 4);
    }

    // ---- the stub's received records ------------------------------------------------------------

    /** Records of one kind whose u64 at {@code offset} equals {@code value}, not skipped, as views. */
    public static List<ByteBuffer> received(int kind, int offset, long value) {
        List<ByteBuffer> out = new ArrayList<>();
        for (var r : engine().received(kind)) {
            if (r.skipped())
                continue;
            ByteBuffer v = r.view();
            if (v.capacity() >= offset + 8 && v.getLong(offset) == value)
                out.add(v);
        }
        return out;
    }

    public static List<ByteBuffer> received(int kind, Predicate<ByteBuffer> p) {
        List<ByteBuffer> out = new ArrayList<>();
        for (var r : engine().received(kind))
            if (!r.skipped() && p.test(r.view()))
                out.add(r.view());
        return out;
    }

    /** A name in Java's dictionary (what the engine received as DICT), or null. */
    public static String javaName(int id) {
        return engine().javaDictName(id);
    }

    // ---- scripting the engine side --------------------------------------------------------------

    /**
     * Scripts one industry command on whichever engine runs the session. The stub writes it with its own
     * dictionary ({@code out.dict} sends the DICT first); the native engine takes the record as an
     * INDUSTRY_COMMAND order, its ids Java's own, and writes it as its industry engine's (W17).
     */
    private static void script(Consumer<StubEngine.CommandOut> command) {
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

    /** One command record with Java's dictionary ids, for the native engine's INDUSTRY_COMMAND order. */
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

    public record Delta(int inventory, String good, BlockPos pos, long delta) {}

    public record Destroy(BlockPos pos, String expectedState) {}

    /** Scripts an IND_CHUNK_RECONCILE as the engine would write it (docs/industry-boundary.md §4). */
    public static void reconcile(long opId, long district, int epoch, ServerLevel level, int cx, int cz, List<Delta> deltas, List<Destroy> destroys) {
        String dim = level.dimension().location().toString();
        script(out -> {
            int dimId = out.dict(Obs.DICT_DIMENSION, dim);
            int[] goods = new int[deltas.size()];
            for (int i = 0; i < deltas.size(); i++)
                goods[i] = out.dict(IndRecords.DICT_GOOD, deltas.get(i).good());
            int[] states = new int[destroys.size()];
            for (int i = 0; i < destroys.size(); i++)
                states[i] = out.dict(IndRecords.DICT_BLOCK_STATE, destroys.get(i).expectedState());
            int at = out.reserve(IndRecords.IND_CHUNK_RECONCILE, IndRecords.ChunkReconcile.size(deltas.size(), destroys.size()), deltas.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + IndRecords.ChunkReconcile.OP_ID, opId);
            b.putLong(at + IndRecords.ChunkReconcile.DISTRICT, district);
            b.putInt(at + IndRecords.ChunkReconcile.EPOCH, epoch);
            b.putInt(at + IndRecords.ChunkReconcile.DIM, dimId);
            b.putInt(at + IndRecords.ChunkReconcile.CHUNK_X, cx);
            b.putInt(at + IndRecords.ChunkReconcile.CHUNK_Z, cz);
            b.putInt(at + IndRecords.ChunkReconcile.DESTROY_COUNT, destroys.size());
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
            int b0 = at + IndRecords.ChunkReconcile.DELTAS + deltas.size() * IndRecords.ChunkReconcile.DELTA_SIZE;
            for (int i = 0; i < destroys.size(); i++) {
                int e = b0 + i * IndRecords.ChunkReconcile.DESTROY_SIZE;
                BlockPos p = destroys.get(i).pos();
                b.putInt(e + IndRecords.ChunkReconcile.B_X, p.getX());
                b.putInt(e + IndRecords.ChunkReconcile.B_Y, p.getY());
                b.putInt(e + IndRecords.ChunkReconcile.B_Z, p.getZ());
                b.putInt(e + IndRecords.ChunkReconcile.B_STATE, states[i]);
            }
        });
    }

    /** Scripts an IND_CARRIAGE_DELTA: (carriage, good, delta into the carriage) entries. */
    public static void carriageDelta(long opId, long train, int reason, List<Object[]> entries) {
        script(out -> {
            int[] goods = new int[entries.size()];
            for (int i = 0; i < entries.size(); i++)
                goods[i] = out.dict(IndRecords.DICT_GOOD, (String) entries.get(i)[1]);
            int at = out.reserve(IndRecords.IND_CARRIAGE_DELTA, IndRecords.CarriageDelta.size(entries.size()), entries.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + IndRecords.CarriageDelta.OP_ID, opId);
            b.putLong(at + IndRecords.CarriageDelta.TRAIN, train);
            b.putInt(at + IndRecords.CarriageDelta.REASON, reason);
            for (int i = 0; i < entries.size(); i++) {
                int e = at + IndRecords.CarriageDelta.ENTRIES + i * IndRecords.CarriageDelta.ENTRY;
                b.putInt(e + IndRecords.CarriageDelta.E_CARRIAGE, (Integer) entries.get(i)[0]);
                b.putInt(e + IndRecords.CarriageDelta.E_GOOD, goods[i]);
                b.putLong(e + IndRecords.CarriageDelta.E_DELTA, (Long) entries.get(i)[2]);
            }
        });
    }

    /** COMMAND_RESULTs for an op id, statuses in order. */
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
}
