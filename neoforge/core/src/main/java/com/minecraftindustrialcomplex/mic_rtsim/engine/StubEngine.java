package com.minecraftindustrialcomplex.mic_rtsim.engine;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Buffers;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Engine;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.RecordReader;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.RecordWriter;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Seq;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.SessionStart;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;

/**
 * A pure-Java engine that speaks the boundary protocol exactly as the Rust engine will, so the bridge
 * is testable before JNI exists. Scaffolding: it is deleted once rtsim-jni lands.
 *
 * <p>What it does:
 * <ul>
 *   <li>allocates the four buffers (64-aligned, little-endian) and runs each step on its own worker
 *       thread, the way the host's background pool will;</li>
 *   <li>reads observations: skips journaled records its pages or memory already include, applies
 *       TRANSFER op ids at most once into a ledger, and remembers op ids Java reports as applied;</li>
 *   <li>writes the commands a test (or {@code /rtsim stub}) scripts, assigning command sequences and
 *       its own dictionary ids, as many as fit, the rest next step;</li>
 *   <li>keeps "pages": a snapshot taken at a save request, written to {@code pagesFile} when there
 *       is one. The next command header acknowledges what the snapshot includes. A "crash" is simply
 *       dropping the instance and building a new one from the last pages.</li>
 * </ul>
 * It simulates nothing.
 */
public final class StubEngine implements Engine {

    /** Record bytes per buffer. PLACEHOLDER: the real engine sizes buffers from caps and quotas (PLAN.md §4.5). */
    public static final int DEFAULT_CAPACITY = 256 * 1024;

    /** What the stub's saved pages hold. */
    public record Pages(long obsSeq, long nextCmdSeq, Set<Long> appliedOps, Map<Long, Long> ledger) {
        public static final Pages EMPTY = new Pages(0, 1, Set.of(), Map.of());

        public Pages {
            appliedOps = Set.copyOf(appliedOps);
            ledger = Map.copyOf(ledger);
        }

        public byte[] encode() {
            try {
                ByteArrayOutputStream bytes = new ByteArrayOutputStream();
                DataOutputStream out = new DataOutputStream(bytes);
                out.writeInt(0x52545350); // "RTSP"
                out.writeLong(obsSeq);
                out.writeLong(nextCmdSeq);
                out.writeInt(appliedOps.size());
                for (long op : appliedOps)
                    out.writeLong(op);
                out.writeInt(ledger.size());
                for (Map.Entry<Long, Long> e : ledger.entrySet()) {
                    out.writeLong(e.getKey());
                    out.writeLong(e.getValue());
                }
                out.flush();
                return bytes.toByteArray();
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }

        public static Pages decode(byte[] data) throws IOException {
            DataInputStream in = new DataInputStream(new ByteArrayInputStream(data));
            if (in.readInt() != 0x52545350)
                throw new IOException("not stub pages");
            long obsSeq = in.readLong();
            long nextCmdSeq = in.readLong();
            Set<Long> ops = new HashSet<>();
            for (int i = in.readInt(); i > 0; i--)
                ops.add(in.readLong());
            Map<Long, Long> ledger = new HashMap<>();
            for (int i = in.readInt(); i > 0; i--)
                ledger.put(in.readLong(), in.readLong());
            return new Pages(obsSeq, nextCmdSeq, ops, ledger);
        }
    }

    /** One observation record the stub received, for tests and {@code /rtsim stub}. */
    public record Received(long handover, int kind, int aux, long obsSeq, boolean skipped, byte[] bytes) {
        public ByteBuffer view() {
            return ByteBuffer.wrap(bytes).order(Layout.ORDER);
        }
    }

    /** Writes one scripted command; the stub assigns its sequence. */
    public interface CommandOut {
        /** The stub's dictionary id for a name, sending the DICT command first if it is new. */
        int dict(int dictKind, String name);

        /** Reserves a command record; returns its offset, or -1 when the buffer is full. */
        int reserve(int kind, int size, int aux);

        ByteBuffer buffer();
    }

    private static final int RECEIVED_CAP = 10_000;

    private final ByteBuffer[] obs = new ByteBuffer[2];
    private final ByteBuffer[] cmd = new ByteBuffer[2];
    private final ExecutorService worker;
    private final Path pagesFile;
    private final RecordReader reader = new RecordReader();
    private final RecordWriter writer = new RecordWriter();

    private volatile Pages disk;
    private volatile boolean saveRequested;
    private volatile String protocolError;

    // Engine memory. Touched on the worker thread; read by tests through synchronized getters.
    private long maxObsSeq;
    private long nextCmdSeq = 1;
    private long ackedObsSeq;
    private final Set<Long> appliedOps = new HashSet<>();
    private final Map<Long, Long> ledger = new HashMap<>();
    private final Map<Integer, String> javaDict = new HashMap<>();
    private final Map<String, Integer> cmdDict = new HashMap<>();
    private final Set<Integer> cmdDictSent = new HashSet<>();
    private final ArrayDeque<Received> received = new ArrayDeque<>();
    private long steps;
    private long staleSkipped;
    private long opDuplicates;
    private boolean started;

    private final ConcurrentLinkedQueue<Consumer<CommandOut>> script = new ConcurrentLinkedQueue<>();

    public StubEngine(Pages pages, Path pagesFile, int capacity) {
        this.disk = pages;
        this.pagesFile = pagesFile;
        for (int i = 0; i < 2; i++) {
            obs[i] = Buffers.allocate(capacity);
            cmd[i] = Buffers.allocate(capacity);
        }
        this.worker = Executors.newSingleThreadExecutor(r -> {
            Thread t = new Thread(r, "mic_rtsim stub engine");
            t.setDaemon(true);
            return t;
        });
    }

    /** A stub with no pages: a fresh world. */
    public StubEngine() {
        this(Pages.EMPTY, null, DEFAULT_CAPACITY);
    }

    /** A stub that keeps its pages in {@code file} (loaded now if it exists). */
    public static StubEngine open(Path file) {
        Pages pages = Pages.EMPTY;
        if (file != null && Files.isRegularFile(file)) {
            try {
                pages = Pages.decode(Files.readAllBytes(file));
            } catch (IOException e) {
                throw new IllegalStateException("cannot read stub engine pages " + file, e);
            }
        }
        return new StubEngine(pages, file, DEFAULT_CAPACITY);
    }

    // ---- Engine ---------------------------------------------------------------------------------

    @Override
    public ByteBuffer observationBuffer(int index) {
        return obs[index];
    }

    @Override
    public ByteBuffer commandBuffer(int index) {
        return cmd[index];
    }

    @Override
    public synchronized SessionStart start(long lastAppliedCmdSeq) {
        if (started)
            throw new IllegalStateException("start called twice");
        started = true;
        Pages p = disk;
        maxObsSeq = p.obsSeq();
        ackedObsSeq = p.obsSeq();
        nextCmdSeq = Math.max(p.nextCmdSeq(), lastAppliedCmdSeq + 1);
        appliedOps.clear();
        appliedOps.addAll(p.appliedOps());
        ledger.clear();
        ledger.putAll(p.ledger());
        return new SessionStart(p.obsSeq(), nextCmdSeq);
    }

    @Override
    public void submit(long handover) {
        worker.execute(() -> runStep(handover));
    }

    private volatile java.util.concurrent.CountDownLatch stepGate;

    /** Test helper: steps wait on {@code gate} before running (null lets them run). */
    public void gate(java.util.concurrent.CountDownLatch gate) {
        this.stepGate = gate;
    }

    private void runStep(long handover) {
        java.util.concurrent.CountDownLatch g = stepGate;
        if (g != null) {
            try {
                g.await();
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                return;
            }
        }
        step(handover);
    }

    @Override
    public void requestSave() {
        saveRequested = true;
    }

    @Override
    public String describe() {
        return "stub engine (pure Java, simulates nothing)";
    }

    @Override
    public void close() {
        worker.shutdown();
        try {
            worker.awaitTermination(5, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        synchronized (this) {
            if (pagesFile != null)
                savePages();
        }
    }

    // ---- the step -------------------------------------------------------------------------------

    private synchronized void step(long h) {
        steps++;
        ByteBuffer o = obs[(int) (h & 1)];
        if (Seq.getAcquire(o, Layout.H_PUBLISHED) != h) {
            protocolError = "step " + h + ": observation buffer not published";
            return;
        }
        long gameTime = o.getLong(Layout.H_WORD_B);
        reader.reset(o, Layout.HEADER_BYTES, o.getInt(Layout.H_USED));
        while (reader.next())
            observe(h, reader);
        if (reader.error() != null)
            protocolError = "step " + h + ": " + reader.error();
        Seq.setRelease(o, Layout.H_CONSUMED, h);

        if (saveRequested) {
            saveRequested = false;
            savePages();
        }

        ByteBuffer c = cmd[(int) (h & 1)];
        long consumed = Seq.getAcquire(c, Layout.H_CONSUMED);
        if (h > 2 && consumed != h - 2)
            protocolError = "step " + h + ": command buffer still held by Java (consumed=" + consumed + ")";
        writer.reset(c);
        long highestCmd = 0;
        boolean more = false;
        Consumer<CommandOut> next;
        while ((next = script.peek()) != null) {
            long mark = writer.mark();
            long seqMark = nextCmdSeq;
            Set<Integer> dictBefore = new HashSet<>(cmdDictSent);
            boolean[] full = {false};
            next.accept(new CommandOut() {
                @Override
                public int dict(int dictKind, String name) {
                    String key = dictKind + ":" + name;
                    Integer id = cmdDict.get(key);
                    if (id == null) {
                        id = cmdDict.size() + 1;
                        cmdDict.put(key, id);
                    }
                    if (!cmdDictSent.contains(id)) {
                        byte[] bytes = name.getBytes(StandardCharsets.UTF_8);
                        int at = reserve(Cmd.DICT, Cmd.Dict.size(bytes.length), dictKind);
                        if (at >= 0) {
                            c.putInt(at + Cmd.Dict.ID, id);
                            c.putInt(at + Cmd.Dict.NAME_LEN, bytes.length);
                            writer.putBytes(at + Cmd.Dict.NAME, bytes);
                            cmdDictSent.add(id);
                        }
                    }
                    return id;
                }

                @Override
                public int reserve(int kind, int size, int aux) {
                    if (full[0])
                        return -1;
                    int at = writer.reserve(kind, size, aux);
                    if (at < 0) {
                        full[0] = true;
                        return -1;
                    }
                    c.putLong(at + Cmd.CMD_SEQ, nextCmdSeq++);
                    return at;
                }

                @Override
                public ByteBuffer buffer() {
                    return c;
                }
            });
            if (full[0]) {
                writer.rollback(mark);
                nextCmdSeq = seqMark;
                cmdDictSent.retainAll(dictBefore);
                more = true;
                break;
            }
            script.poll();
            highestCmd = nextCmdSeq - 1;
        }
        c.putInt(Layout.H_USED, writer.used());
        c.putInt(Layout.H_COUNT, writer.count());
        c.putInt(Layout.H_FLAGS, more ? Layout.CMD_FLAG_MORE : 0);
        c.putLong(Layout.H_WORD_A, ackedObsSeq);
        c.putLong(Layout.H_WORD_B, gameTime);
        c.putLong(Layout.H_WORD_C, highestCmd);
        c.putLong(Layout.H_RESERVED, 0L);
        Seq.setRelease(c, Layout.H_PUBLISHED, h);
    }

    private void observe(long h, RecordReader r) {
        ByteBuffer b = r.buffer();
        int at = r.at();
        int kind = r.kind();
        if (kind == Obs.DICT) {
            int len = b.getInt(at + Obs.Dict.NAME_LEN);
            byte[] name = new byte[len];
            b.get(at + Obs.Dict.NAME, name, 0, len);
            javaDict.put(b.getInt(at + Obs.Dict.ID), new String(name, StandardCharsets.UTF_8));
        }
        if (kind == Obs.TICK)
            return;
        long seq = 0;
        boolean skipped = false;
        if (Obs.isJournaled(kind)) {
            seq = b.getLong(at + Obs.OBS_SEQ);
            if (seq <= maxObsSeq) {
                staleSkipped++;
                skipped = true;
            } else {
                maxObsSeq = seq;
                if (kind == Obs.TRANSFER) {
                    long op = b.getLong(at + Obs.Transfer.OP_ID);
                    if (!appliedOps.add(op)) {
                        opDuplicates++;
                    } else {
                        ledger.merge(b.getLong(at + Obs.Transfer.TO), b.getLong(at + Obs.Transfer.AMOUNT), Long::sum);
                    }
                } else if (kind == Obs.COMMAND_RESULT) {
                    int status = r.aux();
                    if (status == Obs.CommandResult.APPLIED || status == Obs.CommandResult.DUPLICATE_OP)
                        appliedOps.add(b.getLong(at + Obs.CommandResult.OP_ID));
                }
            }
        }
        if (received.size() >= RECEIVED_CAP)
            received.removeFirst();
        received.addLast(new Received(h, kind, r.aux(), seq, skipped, r.copyRecord()));
    }

    private void savePages() {
        disk = new Pages(maxObsSeq, nextCmdSeq, appliedOps, ledger);
        ackedObsSeq = maxObsSeq;
        if (pagesFile != null) {
            try {
                Files.createDirectories(pagesFile.getParent());
                Path tmp = pagesFile.resolveSibling(pagesFile.getFileName() + ".tmp");
                Files.write(tmp, disk.encode());
                Files.move(tmp, pagesFile, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
            } catch (IOException e) {
                protocolError = "cannot write stub pages: " + e;
            }
        }
    }

    // ---- scripting (tests, /rtsim stub) ---------------------------------------------------------

    /** Queues a command for the next step that has room. */
    public void script(Consumer<CommandOut> command) {
        script.add(command);
    }

    /** One member of a scripted SPAWN_GROUP. Items are (slot, item name, count). */
    public record MemberSpec(int index, float health, int bodyA, int bodyB, List<ItemSpec> items) {
        public static MemberSpec plain(int index) {
            return new MemberSpec(index, 0f, -1, -1, List.of());
        }
    }

    public record ItemSpec(int slot, String item, int count) {}

    public void spawnGroup(long agent, int epoch, String groupKind, String dimension, double x, double y, double z,
                           List<MemberSpec> members) {
        script(spawnGroupCommand(agent, epoch, groupKind, dimension, x, y, z, members));
    }

    /** A SPAWN_GROUP command, for {@link #scriptTogether}. */
    public Consumer<CommandOut> spawnGroupCommand(long agent, int epoch, String groupKind, String dimension, double x, double y, double z,
                                                  List<MemberSpec> members) {
        return out -> {
            int kind = out.dict(Obs.DICT_BODY, groupKind);
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int[][] itemIds = new int[members.size()][];
            int size = Cmd.SpawnGroup.MEMBERS;
            for (int m = 0; m < members.size(); m++) {
                List<ItemSpec> items = members.get(m).items();
                itemIds[m] = new int[items.size()];
                for (int i = 0; i < items.size(); i++)
                    itemIds[m][i] = out.dict(Obs.DICT_ITEM, items.get(i).item());
                size += Cmd.SpawnGroup.Member.size(items.size());
            }
            int at = out.reserve(Cmd.SPAWN_GROUP, size, members.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + Cmd.SpawnGroup.AGENT, agent);
            b.putInt(at + Cmd.SpawnGroup.EPOCH, epoch);
            b.putInt(at + Cmd.SpawnGroup.GROUP_KIND, kind);
            b.putInt(at + Cmd.SpawnGroup.DIM, dim);
            b.putDouble(at + Cmd.SpawnGroup.X, x);
            b.putDouble(at + Cmd.SpawnGroup.Y, y);
            b.putDouble(at + Cmd.SpawnGroup.Z, z);
            int m0 = at + Cmd.SpawnGroup.MEMBERS;
            for (int m = 0; m < members.size(); m++) {
                MemberSpec spec = members.get(m);
                b.putInt(m0 + Cmd.SpawnGroup.Member.INDEX, spec.index());
                b.putInt(m0 + Cmd.SpawnGroup.Member.ITEM_COUNT, spec.items().size());
                b.putFloat(m0 + Cmd.SpawnGroup.Member.HEALTH, spec.health());
                b.putInt(m0 + Cmd.SpawnGroup.Member.BODY_A, spec.bodyA());
                b.putInt(m0 + Cmd.SpawnGroup.Member.BODY_B, spec.bodyB());
                int i0 = m0 + Cmd.SpawnGroup.Member.ITEMS;
                for (int i = 0; i < spec.items().size(); i++) {
                    int it = i0 + i * Obs.Item.SIZE;
                    b.putInt(it + Obs.Item.SLOT, spec.items().get(i).slot());
                    b.putInt(it + Obs.Item.ITEM, itemIds[m][i]);
                    b.putInt(it + Obs.Item.COUNT, spec.items().get(i).count());
                }
                m0 += Cmd.SpawnGroup.Member.size(spec.items().size());
            }
        };
    }

    /** Scripts several commands as one unit: they land in the same step's buffer, or all wait. */
    @SafeVarargs
    public final void scriptTogether(Consumer<CommandOut>... commands) {
        script(out -> {
            for (Consumer<CommandOut> c : commands)
                c.accept(out);
        });
    }

    public void despawnGroup(long agent, int epoch) {
        script(out -> {
            int at = out.reserve(Cmd.DESPAWN_GROUP, Cmd.DespawnGroup.SIZE, 0);
            if (at < 0)
                return;
            out.buffer().putLong(at + Cmd.DespawnGroup.AGENT, agent);
            out.buffer().putInt(at + Cmd.DespawnGroup.EPOCH, epoch);
        });
    }

    public void steerGroup(long agent, int epoch, double x, double y, double z, float speed) {
        script(out -> {
            int at = out.reserve(Cmd.STEER_GROUP, Cmd.SteerGroup.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + Cmd.SteerGroup.AGENT, agent);
            b.putInt(at + Cmd.SteerGroup.EPOCH, epoch);
            b.putFloat(at + Cmd.SteerGroup.SPEED, speed);
            b.putDouble(at + Cmd.SteerGroup.X, x);
            b.putDouble(at + Cmd.SteerGroup.Y, y);
            b.putDouble(at + Cmd.SteerGroup.Z, z);
        });
    }

    /**
     * Scripts a TRANSFER_TO_CONTAINER. The stub does not check its own op ledger first, so a test can
     * make it issue an op id Java has already applied (what an engine rolled back by a crash does).
     */
    public void transferToContainer(long opId, String dimension, int x, int y, int z, String item, int count) {
        script(out -> {
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int it = out.dict(Obs.DICT_ITEM, item);
            int at = out.reserve(Cmd.TRANSFER_TO_CONTAINER, Cmd.TransferToContainer.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + Cmd.TransferToContainer.OP_ID, opId);
            b.putInt(at + Cmd.TransferToContainer.DIM, dim);
            b.putInt(at + Cmd.TransferToContainer.ITEM, it);
            b.putInt(at + Cmd.TransferToContainer.X, x);
            b.putInt(at + Cmd.TransferToContainer.Y, y);
            b.putInt(at + Cmd.TransferToContainer.Z, z);
            b.putInt(at + Cmd.TransferToContainer.COUNT, count);
        });
    }

    /**
     * Test helper: makes the next scripted command carry {@code next} as its sequence, as an engine
     * whose counter went backwards would. Java must skip anything at or below its last applied one.
     */
    public synchronized void rewindCommandSequence(long next) {
        nextCmdSeq = next;
    }

    // ---- inspection -----------------------------------------------------------------------------

    /** The last saved pages: what survives a crash. */
    public Pages pages() {
        return disk;
    }

    public synchronized List<Received> received() {
        return Collections.unmodifiableList(new ArrayList<>(received));
    }

    public synchronized List<Received> received(int kind) {
        List<Received> out = new ArrayList<>();
        for (Received r : received)
            if (r.kind() == kind)
                out.add(r);
        return out;
    }

    public synchronized long ledger(long store) {
        return ledger.getOrDefault(store, 0L);
    }

    public synchronized boolean opApplied(long opId) {
        return appliedOps.contains(opId);
    }

    public synchronized long steps() {
        return steps;
    }

    public synchronized long staleSkipped() {
        return staleSkipped;
    }

    public synchronized long opDuplicates() {
        return opDuplicates;
    }

    public synchronized String javaDictName(int id) {
        return javaDict.get(id);
    }

    public synchronized long maxObsSeq() {
        return maxObsSeq;
    }

    public int scriptedPending() {
        return script.size();
    }

    public String protocolError() {
        return protocolError;
    }

    /** Test helper: blocks until every submitted step has run. Never call it on a tick in real play. */
    public void awaitIdle() {
        try {
            worker.submit(() -> {}).get(10, TimeUnit.SECONDS);
        } catch (Exception e) {
            throw new IllegalStateException("stub engine worker stuck", e);
        }
    }
}
