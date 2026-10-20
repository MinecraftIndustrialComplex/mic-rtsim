package com.minecraftindustrialcomplex.mic_rtsim.engine;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.SessionStart;

import java.nio.ByteBuffer;
import java.nio.file.Path;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * The Rust engine (crates/rtsim-jni) through JNI, a plugin of mic_native: its steps run on the host's
 * background pool. Every {@link com.minecraftindustrialcomplex.mic_rtsim.boundary.Engine} call maps
 * to one native call; the four buffers are engine memory seen as direct {@code ByteBuffer}s.
 *
 * <p>The test and debug surface ({@link ScriptedEngine}) goes through orders ({@link NativeCodec})
 * and the native inspection calls. With {@link #FLAG_TEST} the engine also keeps a log of the
 * observation records it received, which this class drains into a Java list of the last 10,000.
 *
 * <p>Loading ({@link #ensureLoaded}) has mic_native load the plugin from this mod's jar, then binds
 * the natives below to this mod's class loader with {@code System.load}. There is no fallback: if the
 * library cannot be loaded, opening fails with the reason (A8).
 */
public final class NativeEngine implements ScriptedEngine {

    /** Keep the received log and the inspection data. GameTests and unit tests only. */
    public static final int FLAG_TEST = 1;
    /** Skip fsync when writing pages. GameTests only: a test world is thrown away. */
    public static final int FLAG_NO_FSYNC = 2;

    /** The plugin's library: {@code librtsim_jni.so}, {@code rtsim_jni.dll}, {@code librtsim_jni.dylib}. */
    public static final String LIBRARY = "rtsim_jni";

    // nStats indexes.
    static final int STAT_STEPS_STARTED = 0;
    static final int STAT_STEPS_DONE = 1;
    static final int STAT_STALE_SKIPPED = 2;
    static final int STAT_OP_DUPLICATES = 3;
    static final int STAT_APPLIED_THROUGH = 4;
    static final int STAT_PAGES_OBS_SEQ = 5;
    static final int STAT_ACKED_SENT = 6;
    static final int STAT_NEXT_CMD_SEQ = 7;
    static final int STAT_ORDERS_PENDING = 8;
    static final int STAT_SAVES_DONE = 9;
    static final int STAT_LAST_STEP_NANOS = 10;
    static final int STAT_AGENTS = 11;
    static final int STAT_EMBODIED = 12;
    static final int STAT_GAPS = 13;
    static final int STAT_COMMANDS_WRITTEN = 14;
    static final int STAT_SAVE_FAILURES = 15;
    static final int STATS = 16;

    private static final int RECEIVED_CAP = 10_000;
    private static final int CLOSE_TIMEOUT_MS = 5000;
    private static final long IDLE_TIMEOUT_NANOS = 10_000_000_000L;

    // ---- the natives (crates/rtsim-jni) ---------------------------------------------------------

    /** flags: {@link #FLAG_TEST}, {@link #FLAG_NO_FSYNC}. capacity: record bytes per buffer (0: the engine's default). Throws RuntimeException. */
    private static native long nOpen(String pagesDir, int capacity, int flags);

    /** 0, 1: observation buffers; 2, 3: command buffers. Direct, engine memory. */
    private static native ByteBuffer nBuffer(long handle, int which);

    /** out[0] = pagesObsSeq, out[1] = nextCmdSeq. 0 or a negative status (message: nLastError). */
    private static native int nStart(long handle, long lastAppliedCmdSeq, long[] out);

    /** Queues step {@code handover} on the background pool; returns at once. 0 or negative. */
    private static native int nSubmit(long handle, long handover);

    private static native void nRequestSave(long handle, long throughObsSeq);

    /** save: final save waited for; false: a crash. Waits up to timeoutMs for a step in flight. The handle is dead afterwards. */
    private static native int nClose(long handle, boolean save, int timeoutMs);

    private static native String nDescribe(long handle);

    /** This thread's last failure message, "" if none. */
    private static native String nLastError();

    /** Orders, encoded as in {@link NativeCodec}. 0 or negative (-2: bad encoding). */
    private static native int nOrder(long handle, byte[] orders);

    /** The received log since the last drain ({@link NativeCodec}); an empty array if none, never null. */
    private static native byte[] nDrainReceived(long handle);

    /** out.length >= 16; indexes STAT_*. */
    private static native void nStats(long handle, long[] out);

    private static native long nLedger(long handle, long store);

    private static native boolean nOpApplied(long handle, long opId);

    /** Null when none. */
    private static native String nProtocolError(long handle);

    /** A hold on the engine's buffer memory for one Java buffer; {@link #nReleaseBuffers} gives it back. */
    private static native long nHoldBuffers(long handle);

    private static native void nReleaseBuffers(long token);

    /**
     * Frees the engine's buffer memory only once no Java buffer can reach it: each buffer from
     * {@code nBuffer} (and every duplicate or view of it, which keeps it reachable) holds the memory
     * until this cleaner runs, whatever {@code nClose} did meanwhile.
     */
    private static final java.lang.ref.Cleaner BUFFERS = java.lang.ref.Cleaner.create();

    /** The cleaner's action: it must not refer to the buffer itself. */
    private record ReleaseHold(long token) implements Runnable {
        @Override
        public void run() {
            nReleaseBuffers(token);
        }
    }

    // ---- loading --------------------------------------------------------------------------------

    private static boolean loaded;

    /**
     * Loads the plugin through mic_native once, and binds this class's natives to this mod's class
     * loader. Throws IllegalStateException naming the reason when either step fails.
     */
    public static synchronized void ensureLoaded() {
        if (loaded)
            return;
        Path library;
        try {
            library = MicNativeLink.loadPlugin();
        } catch (LinkageError e) {
            throw new IllegalStateException("mic_native is not installed (" + e + ")", e);
        }
        try {
            System.load(library.toString());
        } catch (UnsatisfiedLinkError | SecurityException e) {
            throw new IllegalStateException("cannot bind the rtsim_jni natives from " + library + ": " + e.getMessage(), e);
        }
        loaded = true;
    }

    /**
     * Every reference to mic_native's classes, in a class of its own: NativeEngine itself then links
     * without them, and a missing mic_native shows as a LinkageError at the call.
     */
    private static final class MicNativeLink {
        static Path loadPlugin() {
            try {
                return com.minecraftindustrialcomplex.micnative.api.MicNative
                        .loadPlugin(MicRtsim.MODID, LIBRARY).libraryPath();
            } catch (com.minecraftindustrialcomplex.micnative.api.MicNativeException e) {
                throw new IllegalStateException(e.getMessage(), e);
            }
        }
    }

    // ---- one engine -----------------------------------------------------------------------------

    private final Path pagesDir;
    private final int capacity;
    private final int flags;
    private volatile long handle;
    private final ByteBuffer[] buffers = new ByteBuffer[4];
    private final ArrayDeque<Received> received = new ArrayDeque<>();
    private final long[] stats = new long[STATS];
    private long submits;

    private NativeEngine(Path pagesDir, int capacity, int flags, long handle) {
        this.pagesDir = pagesDir;
        this.capacity = capacity;
        this.flags = flags;
        this.handle = handle;
        for (int i = 0; i < 4; i++) {
            ByteBuffer b = nBuffer(handle, i);
            if (b == null)
                throw new IllegalStateException("rtsim_jni returned no buffer " + i + ": " + nLastError());
            BUFFERS.register(b, new ReleaseHold(nHoldBuffers(handle)));
            buffers[i] = b.order(Layout.ORDER);
        }
    }

    /**
     * Opens the engine on the pages in {@code pagesDir} (created if missing).
     *
     * @param capacity record bytes per buffer: a multiple of 8, at least {@link Layout#MIN_CAPACITY};
     *                 0 lets the engine size them
     * @param flags    {@link #FLAG_TEST}, {@link #FLAG_NO_FSYNC}, or 0
     * @throws IllegalStateException if the library cannot be loaded or the engine cannot open
     */
    public static NativeEngine open(Path pagesDir, int capacity, int flags) {
        ensureLoaded();
        long h;
        try {
            h = nOpen(pagesDir.toAbsolutePath().toString(), capacity, flags);
        } catch (RuntimeException e) {
            throw new IllegalStateException("the native engine did not open " + pagesDir + ": " + e.getMessage(), e);
        }
        if (h == 0)
            throw new IllegalStateException("the native engine did not open " + pagesDir + ": " + nLastError());
        try {
            return new NativeEngine(pagesDir, capacity, flags, h);
        } catch (RuntimeException e) {
            nClose(h, false, CLOSE_TIMEOUT_MS);
            throw e;
        }
    }

    public Path pagesDir() {
        return pagesDir;
    }

    private long live() {
        long h = handle;
        if (h == 0)
            throw new IllegalStateException("the native engine is closed");
        return h;
    }

    private static void check(int status, String call) {
        if (status < 0) {
            String why = nLastError();
            throw new IllegalStateException("rtsim_jni " + call + " failed with status " + status
                    + (why == null || why.isEmpty() ? "" : ": " + why));
        }
    }

    // ---- Engine ---------------------------------------------------------------------------------

    @Override
    public ByteBuffer observationBuffer(int index) {
        return buffers[index];
    }

    @Override
    public ByteBuffer commandBuffer(int index) {
        return buffers[2 + index];
    }

    // Every native call that takes the handle runs under this object's monitor, and so does close:
    // a call can never use a handle another thread is freeing.

    @Override
    public synchronized SessionStart start(long lastAppliedCmdSeq) {
        long[] out = new long[2];
        check(nStart(live(), lastAppliedCmdSeq, out), "start");
        return new SessionStart(out[0], out[1]);
    }

    @Override
    public synchronized void submit(long handover) {
        check(nSubmit(live(), handover), "submit(" + handover + ")");
        submits++;
        if ((flags & FLAG_TEST) != 0)
            drain();
    }

    @Override
    public synchronized void requestSave(long throughObsSeq) {
        nRequestSave(live(), throughObsSeq);
    }

    @Override
    public synchronized String describe() {
        long h = handle;
        return h == 0 ? "native engine (closed)" : nDescribe(h);
    }

    /** The final save, waited for (up to 5 s for a step in flight). Idempotent. */
    @Override
    public synchronized void close() {
        long h = takeHandle();
        if (h == 0)
            return;
        int status = nClose(h, true, CLOSE_TIMEOUT_MS);
        if (status < 0)
            MicRtsim.LOGGER.error("[mic_rtsim] the native engine's final save failed (status {}): {}; the journal replays what the pages miss",
                    status, nLastError());
    }

    /** Takes the handle once: this object is dead afterwards. Drains the received log first. */
    private synchronized long takeHandle() {
        long h = handle;
        if (h != 0 && (flags & FLAG_TEST) != 0)
            drain();
        handle = 0;
        return h;
    }

    // ---- ScriptedEngine: orders -----------------------------------------------------------------

    private synchronized void order(NativeCodec.Orders orders, String what) {
        check(nOrder(live(), orders.toBytes()), "order " + what);
    }

    @Override
    public void spawnGroup(long agent, int epoch, String groupKind, String dimension, double x, double y, double z,
                           List<MemberSpec> members) {
        spawnTogether(new SpawnSpec(agent, epoch, groupKind, dimension, x, y, z, members));
    }

    @Override
    public void spawnTogether(SpawnSpec... spawns) {
        NativeCodec.Orders orders = new NativeCodec.Orders();
        for (SpawnSpec s : spawns)
            orders.spawnGroup(s);
        order(orders, "SPAWN_GROUP");
    }

    @Override
    public void despawnGroup(long agent, int epoch) {
        order(new NativeCodec.Orders().despawnGroup(agent, epoch), "DESPAWN_GROUP");
    }

    @Override
    public void steerGroup(long agent, int epoch, double x, double y, double z, float speed) {
        order(new NativeCodec.Orders().steerGroup(agent, epoch, x, y, z, speed), "STEER_GROUP");
    }

    @Override
    public void transferToContainer(long opId, String dimension, int x, int y, int z, String item, int count) {
        order(new NativeCodec.Orders().transferToContainer(opId, dimension, x, y, z, item, count), "TRANSFER_TO_CONTAINER");
    }

    @Override
    public void rewindCommandSequence(long next) {
        order(new NativeCodec.Orders().rewindCommandSequence(next), "REWIND_CMD_SEQ");
    }

    @Override
    public synchronized String javaDictName(int id) {
        drain();
        String name = null;
        for (Received r : received) {
            if (r.kind() != com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.DICT)
                continue;
            java.nio.ByteBuffer b = r.view();
            if (b.getInt(com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Dict.ID) != id)
                continue;
            int len = b.getInt(com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Dict.NAME_LEN);
            byte[] bytes = new byte[len];
            b.get(com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Dict.NAME, bytes, 0, len);
            name = new String(bytes, java.nio.charset.StandardCharsets.UTF_8);
        }
        return name;
    }

    @Override
    public void failForTest() {
        order(new NativeCodec.Orders().fail(), "FAIL");
    }

    @Override
    public void requestSaveAtNextStep() {
        order(new NativeCodec.Orders().saveAtNextStep(), "SAVE_AT_NEXT_STEP");
    }

    /** One entry of Java's dictionary that an ordered industry command uses. */
    public record DictEntry(int kind, int id, String name) {}

    /**
     * Orders an industry command (docs/industry-boundary.md §4): {@code record} is the whole record,
     * its cmdSeq ignored, with Java's own dictionary ids (GOOD, NAME, BLOCK_STATE, dimension), and
     * {@code names} the entries it uses. The engine writes it as its industry engine's own: its
     * sequence, the DICT records it needs, and, for a carriage, channel, deposit or package command,
     * its outbox until Java answers. Tests drive the industry module with it; the engine decides its
     * own commands.
     */
    public void industryCommand(List<DictEntry> names, byte[] record) {
        order(new NativeCodec.Orders().industryCommand(names, record), "INDUSTRY_COMMAND");
    }

    // ---- ScriptedEngine: inspection -------------------------------------------------------------

    /** Moves the native received log into the Java list (capped, oldest dropped first). */
    private synchronized void drain() {
        long h = handle;
        if (h == 0)
            return;
        byte[] log = nDrainReceived(h);
        if (log == null || log.length == 0)
            return;
        List<Received> fresh = new ArrayList<>();
        NativeCodec.decodeReceived(log, fresh);
        for (Received r : fresh) {
            if (received.size() >= RECEIVED_CAP)
                received.removeFirst();
            received.addLast(r);
        }
    }

    @Override
    public synchronized List<Received> received() {
        drain();
        return Collections.unmodifiableList(new ArrayList<>(received));
    }

    @Override
    public synchronized List<Received> received(int kind) {
        drain();
        List<Received> out = new ArrayList<>();
        for (Received r : received)
            if (r.kind() == kind)
                out.add(r);
        return out;
    }

    private synchronized long stat(int index) {
        nStats(live(), stats);
        return stats[index];
    }

    @Override
    public synchronized long ledger(long store) {
        return nLedger(live(), store);
    }

    @Override
    public synchronized boolean opApplied(long opId) {
        return nOpApplied(live(), opId);
    }

    @Override
    public long steps() {
        return stat(STAT_STEPS_DONE);
    }

    @Override
    public long staleSkipped() {
        return stat(STAT_STALE_SKIPPED);
    }

    @Override
    public long opDuplicates() {
        return stat(STAT_OP_DUPLICATES);
    }

    @Override
    public long maxObsSeq() {
        return stat(STAT_APPLIED_THROUGH);
    }

    @Override
    public long pagesObsSeq() {
        return stat(STAT_PAGES_OBS_SEQ);
    }

    /** Nanoseconds the last step took on its pool thread (from its start to its publish). */
    public long lastStepNanos() {
        return stat(STAT_LAST_STEP_NANOS);
    }

    @Override
    public int scriptedPending() {
        return (int) stat(STAT_ORDERS_PENDING);
    }

    @Override
    public synchronized String protocolError() {
        return nProtocolError(live());
    }

    @Override
    public void awaitIdle() {
        long want;
        synchronized (this) {
            want = submits;
        }
        long deadline = System.nanoTime() + IDLE_TIMEOUT_NANOS;
        while (stat(STAT_STEPS_DONE) < want) {
            if (System.nanoTime() - deadline > 0)
                throw new IllegalStateException("native engine stuck: " + stat(STAT_STEPS_DONE) + " of " + want + " steps done after 10 s");
            try {
                Thread.sleep(1);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException("interrupted waiting for the native engine", e);
            }
        }
    }

    @Override
    public synchronized NativeEngine crashAndReopen() {
        long h = takeHandle();
        if (h == 0)
            throw new IllegalStateException("the native engine is closed");
        int status = nClose(h, false, CLOSE_TIMEOUT_MS);
        if (status < 0)
            throw new IllegalStateException("rtsim_jni close (crash) failed with status " + status + ": " + nLastError());
        return open(pagesDir, capacity, flags);
    }
}
