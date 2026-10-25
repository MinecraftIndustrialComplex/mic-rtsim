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
import net.minecraft.world.level.block.Block;
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

    /**
     * For a {@code ChunkWait.chunks} wait: the chunk is loaded as a level chunk (after a test forced it, before
     * a wait on what its load does).
     */
    public static void loads(ServerLevel level, int cx, int cz) {
        check("chunk " + cx + ", " + cz + " loads", level.getChunkSource().getChunkNow(cx, cz) != null);
    }

    /**
     * A wait on the engine, which takes the journal's records on its own thread: the GameTest server ticks as fast as
     * it can, so on a busy machine (a glass replay's step still running, other jobs) a timeout in ticks passes before
     * the engine has taken a record Java handed over. While the check fails, the tick waits 5 ms, so a timeout of
     * 4,000 ticks is at least 20 seconds of the engine's time.
     */
    public static Runnable paced(Runnable check) {
        return () -> {
            try {
                check.run();
            } catch (RuntimeException e) {
                try {
                    Thread.sleep(5);
                } catch (InterruptedException ie) {
                    Thread.currentThread().interrupt();
                }
                throw e;
            }
        };
    }

    public static void check(String what, boolean ok) {
        if (!ok)
            throw new GameTestAssertException(what);
    }

    public static void record(String test, String what, Object value) {
        MicRtsim.LOGGER.info("[gametest] {}: {} = {}", test, what, value);
    }

    /** Test pacing: short sweeps and detection right after an edit. Clears test switches a failed test left on. */
    public static void fastConfig() {
        com.minecraftindustrialcomplex.mic_rtsim_create.Frontier.ignoreReturnsForTest = false;
        com.minecraftindustrialcomplex.mic_rtsim_create.TrainRing.breakForTest(com.minecraftindustrialcomplex.mic_rtsim_create.TrainRing.BreakForTest.NONE);
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

    // ---- joins across chunk borders (SECTIONS.md §2.1) --------------------------------------------

    /**
     * Two of the core's test markers face to face across the chunk border at x {@code bx}, at (bx - 1, y, z)
     * and (bx, y, z), absolute: the chunks either side are one district by a face touch (J1). Since slice S6
     * chunks side by side are one district only by a join, never by adjacency. A marker is industrial (a seed
     * by tag) with no capability and no ticker: it moves and holds nothing, and the glass box never sees it
     * (its residual is block entities with a capability). Keep the pair clear of the layout.
     */
    public static void joinAcrossX(GameTestHelper helper, int bx, int y, int z) {
        BlockState m = com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines.MARKER.get().defaultBlockState();
        set(helper, new BlockPos(bx - 1, y, z), m);
        set(helper, new BlockPos(bx, y, z), m);
    }

    /** Yard-relative height of {@link #joinChunks}' markers: above every build (the glass corpus builds up to y 14). */
    public static final int JOIN_Y = 17;

    /**
     * Joins into one district the chunks that hold an industrial block entity between the yard-relative corners
     * {@code from} and {@code to}, wherever the yard's random position puts the chunk borders, for a layout that
     * means one district and may have no join of its own across a border it straddles (a chain conveyor joins
     * nothing, nor do two stores side by side or a gap). Each border needed gets a pair of {@link #joinAcrossX}'s
     * markers across it at yard-relative height {@link #JOIN_Y}, inside the footprint of those block entities.
     * Chunks only diagonal to each other join through a chunk beside both (the 8-neighbour rule before S6 joined
     * them). Returns the number of borders joined.
     */
    public static int joinChunks(GameTestHelper helper, BlockPos from, BlockPos to) {
        ServerLevel level = helper.getLevel();
        BlockPos a = helper.absolutePos(from), b = helper.absolutePos(to);
        int x0 = Math.min(a.getX(), b.getX()), x1 = Math.max(a.getX(), b.getX());
        int y0 = Math.min(a.getY(), b.getY()), y1 = Math.max(a.getY(), b.getY());
        int z0 = Math.min(a.getZ(), b.getZ()), z1 = Math.max(a.getZ(), b.getZ());
        int y = helper.absolutePos(new BlockPos(0, JOIN_Y, 0)).getY();
        List<Long> held = new ArrayList<>();
        // The footprint of the industrial block entities: min x, max x, min z, max z.
        int[] fp = {Integer.MAX_VALUE, Integer.MIN_VALUE, Integer.MAX_VALUE, Integer.MIN_VALUE};
        for (int cx = x0 >> 4; cx <= x1 >> 4; cx++)
            for (int cz = z0 >> 4; cz <= z1 >> 4; cz++) {
                net.minecraft.world.level.chunk.LevelChunk c = level.getChunkSource().getChunkNow(cx, cz);
                if (c == null)
                    continue;
                boolean any = false;
                for (net.minecraft.world.level.block.entity.BlockEntity be : c.getBlockEntities().values()) {
                    BlockPos p = be.getBlockPos();
                    if (p.getX() < x0 || p.getX() > x1 || p.getY() < y0 || p.getY() > y1 || p.getZ() < z0 || p.getZ() > z1
                            || !ind().districts().industrialForTest(level, be))
                        continue;
                    any = true;
                    fp[0] = Math.min(fp[0], p.getX());
                    fp[1] = Math.max(fp[1], p.getX());
                    fp[2] = Math.min(fp[2], p.getZ());
                    fp[3] = Math.max(fp[3], p.getZ());
                }
                if (any)
                    held.add(ChunkPos.asLong(cx, cz));
            }
        java.util.Map<Long, Long> root = new java.util.HashMap<>();
        for (long c : held)
            root.put(c, c);
        int joined = 0;
        // Borders between chunks side by side first, then chunks only diagonal to each other.
        for (int pass = 0; pass < 2; pass++)
            for (long c : held)
                for (long n : held) {
                    int dx = ChunkPos.getX(n) - ChunkPos.getX(c), dz = ChunkPos.getZ(n) - ChunkPos.getZ(c);
                    boolean wanted = pass == 0 ? (dx == 1 && dz == 0) || (dx == 0 && dz == 1) : dx == 1 && Math.abs(dz) == 1;
                    if (!wanted || joinRoot(root, c) == joinRoot(root, n))
                        continue;
                    if (pass == 0) {
                        joinPair(helper, c, n, y, fp);
                        joined++;
                    } else {
                        long m = ChunkPos.asLong(ChunkPos.getX(n), ChunkPos.getZ(c));
                        joinPair(helper, c, m, y, fp);
                        joinPair(helper, m, n, y, fp);
                        joined += 2;
                    }
                    root.put(joinRoot(root, c), joinRoot(root, n));
                }
        return joined;
    }

    private static long joinRoot(java.util.Map<Long, Long> root, long c) {
        long r = c;
        while (root.get(r) != r)
            r = root.get(r);
        return r;
    }

    /** A marker pair across the border of chunks {@code c} and {@code n} (side by side), inside the footprint {@code fp}. */
    private static void joinPair(GameTestHelper helper, long c, long n, int y, int[] fp) {
        if (ChunkPos.getX(n) < ChunkPos.getX(c) || ChunkPos.getZ(n) < ChunkPos.getZ(c)) {
            long t = c;
            c = n;
            n = t;
        }
        boolean alongX = ChunkPos.getX(n) != ChunkPos.getX(c);
        int border = (alongX ? ChunkPos.getX(n) : ChunkPos.getZ(n)) << 4;
        // The other axis: the chunks' shared row (or column), inside the footprint.
        int row = (alongX ? ChunkPos.getZ(c) : ChunkPos.getX(c)) << 4;
        int lo = Math.max(row, alongX ? fp[2] : fp[0]), hi = Math.min(row + 15, alongX ? fp[3] : fp[1]);
        ServerLevel level = helper.getLevel();
        Block marker = com.minecraftindustrialcomplex.mic_rtsim.gametest.TestMachines.MARKER.get();
        int mid = (lo + hi) >> 1;
        for (int i = 0; i <= hi - lo; i++) {
            int o = mid + ((i & 1) == 0 ? i / 2 : -(i / 2 + 1));
            if (o < lo || o > hi)
                continue;
            BlockPos p = alongX ? new BlockPos(border - 1, y, o) : new BlockPos(o, y, border - 1);
            BlockPos q = alongX ? new BlockPos(border, y, o) : new BlockPos(o, y, border);
            BlockState sp = level.getBlockState(p), sq = level.getBlockState(q);
            if ((sp.isAir() || sp.is(marker)) && (sq.isAir() || sq.is(marker))) {
                if (alongX)
                    joinAcrossX(helper, border, y, o);
                else {
                    set(helper, p, marker.defaultBlockState());
                    set(helper, q, marker.defaultBlockState());
                }
                return;
            }
        }
        throw new GameTestAssertException("no room for a marker pair across the border of chunks " + new ChunkPos(c) + " and " + new ChunkPos(n));
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

    // ---- commands ---------------------------------------------------------------------------------

    /** What a command said, and what it returned ({@code -1} when it did not run). */
    public record Said(List<String> lines, int ran) {}

    /**
     * Runs {@code command} at permission 2 from {@code at} in {@code level} on the server thread and collects the
     * lines it sends, success and failure alike.
     */
    public static Said say(net.minecraft.server.level.ServerLevel level, BlockPos at, String command) {
        List<String> lines = new ArrayList<>();
        net.minecraft.commands.CommandSource sink = new net.minecraft.commands.CommandSource() {
            @Override
            public void sendSystemMessage(net.minecraft.network.chat.Component c) {
                lines.add(c.getString());
            }

            @Override
            public boolean acceptsSuccess() {
                return true;
            }

            @Override
            public boolean acceptsFailure() {
                return true;
            }

            @Override
            public boolean shouldInformAdmins() {
                return false;
            }
        };
        net.minecraft.commands.CommandSourceStack src = level.getServer().createCommandSourceStack().withSource(sink).withPermission(2).withLevel(level)
                .withPosition(net.minecraft.world.phys.Vec3.atCenterOf(at));
        int ran = -1;
        try {
            ran = level.getServer().getCommands().getDispatcher().execute(command, src);
        } catch (com.mojang.brigadier.exceptions.CommandSyntaxException e) {
            lines.add("syntax error: " + e.getMessage());
        }
        return new Said(lines, ran);
    }
}
