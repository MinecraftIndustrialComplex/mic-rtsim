package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
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
import net.neoforged.neoforge.common.util.FakePlayerFactory;
import net.neoforged.neoforge.event.level.BlockEvent;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Predicate;

/**
 * Shared helpers for the industry GameTests ({@code ./gradlew :create:runGameTestServer}). The tests
 * drive the server's real bridge session, whose engine is the {@link StubEngine}: a test scripts
 * industry commands into the stub (as the Rust engine would send them, wire.rs's layouts), waits for the
 * bridge to apply them on a later tick, and reads the observations the stub received.
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
        BlockPos a = helper.absolutePos(BlockPos.ZERO), b = helper.absolutePos(new BlockPos(33, 0, 33));
        int minCx = Math.min(a.getX(), b.getX()) >> 4, maxCx = Math.max(a.getX(), b.getX()) >> 4;
        int minCz = Math.min(a.getZ(), b.getZ()) >> 4, maxCz = Math.max(a.getZ(), b.getZ()) >> 4;
        ind().districts().forgetArea(helper.getLevel(), minCx - 1, minCz - 1, maxCx + 1, maxCz + 1);
    }

    /** Sets a block at an absolute position. */
    public static void set(GameTestHelper helper, BlockPos abs, BlockState state) {
        helper.getLevel().setBlock(abs, state, 3);
    }

    /** What a player placing the block at {@code abs} fires (BlockEvent.EntityPlaceEvent), with a fake player. */
    public static void playerPlaced(GameTestHelper helper, BlockPos abs) {
        ServerLevel level = helper.getLevel();
        NeoForge.EVENT_BUS.post(new BlockEvent.EntityPlaceEvent(BlockSnapshot.create(level.dimension(), level, abs),
                level.getBlockState(abs.below()), FakePlayerFactory.getMinecraft(level)));
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
        for (var r : stub().received(kind)) {
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
        for (var r : stub().received(kind))
            if (!r.skipped() && p.test(r.view()))
                out.add(r.view());
        return out;
    }

    /** A name in Java's dictionary (what the stub received as DICT), or null. */
    public static String javaName(int id) {
        return stub().javaDictName(id);
    }

    // ---- scripting the engine side --------------------------------------------------------------

    public record Delta(int inventory, String good, BlockPos pos, long delta) {}

    public record Destroy(BlockPos pos, String expectedState) {}

    /** Scripts an IND_CHUNK_RECONCILE as the engine would write it (docs/industry-boundary.md §4). */
    public static void reconcile(long opId, long district, int epoch, ServerLevel level, int cx, int cz, List<Delta> deltas, List<Destroy> destroys) {
        String dim = level.dimension().location().toString();
        stub().script(out -> {
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
        stub().script(out -> {
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

    public record LedgerGood(String good, long perHour, long sinceStart) {}

    /** Scripts an IND_LEDGER reply. */
    public static void ledger(long district, int state, int freeze, long nextBreakpoint, int nextKind, String nextGood, long since, List<LedgerGood> goods) {
        stub().script(out -> {
            int[] ids = new int[goods.size()];
            for (int i = 0; i < goods.size(); i++)
                ids[i] = out.dict(IndRecords.DICT_GOOD, goods.get(i).good());
            int next = nextGood == null ? 0 : out.dict(IndRecords.DICT_GOOD, nextGood);
            int at = out.reserve(IndRecords.IND_LEDGER, IndRecords.Ledger.size(goods.size()), goods.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + IndRecords.Ledger.DISTRICT, district);
            b.putInt(at + IndRecords.Ledger.STATE, state);
            b.putInt(at + IndRecords.Ledger.FREEZE, freeze);
            b.putLong(at + IndRecords.Ledger.NEXT_BREAKPOINT, nextBreakpoint);
            b.putInt(at + IndRecords.Ledger.NEXT_KIND, nextKind);
            b.putInt(at + IndRecords.Ledger.NEXT_GOOD, next);
            b.putLong(at + IndRecords.Ledger.SINCE, since);
            for (int i = 0; i < goods.size(); i++) {
                int g = at + IndRecords.Ledger.GOODS + i * IndRecords.Ledger.GOOD;
                b.putInt(g + IndRecords.Ledger.G_GOOD, ids[i]);
                b.putLong(g + IndRecords.Ledger.G_PER_HOUR, goods.get(i).perHour());
                b.putLong(g + IndRecords.Ledger.G_SINCE_START, goods.get(i).sinceStart());
            }
        });
    }

    /** COMMAND_RESULTs for an op id, statuses in order. */
    public static List<Integer> commandResults(long opId) {
        List<Integer> out = new ArrayList<>();
        for (var r : stub().received(Obs.COMMAND_RESULT))
            if (!r.skipped() && r.view().getLong(Obs.CommandResult.OP_ID) == opId)
                out.add(r.aux());
        return out;
    }

    public static int aux(ByteBuffer record) {
        return record.getInt(Layout.R_AUX);
    }
}
