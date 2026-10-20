package com.minecraftindustrialcomplex.mic_rtsim.engine;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Buffers;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.market.PriceCurve;
import com.minecraftindustrialcomplex.mic_rtsim.market.PriceOracle;
import com.minecraftindustrialcomplex.mic_rtsim.market.Ratio;
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
 *       is one. The next command header acknowledges what the snapshot includes. A "crash"
 *       ({@link #crashAndReopen}) is dropping the instance without saving and building a new one from
 *       the last pages.</li>
 * </ul>
 * It simulates nothing.
 */
public final class StubEngine implements ScriptedEngine {

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
    private final int capacity;
    private final RecordReader reader = new RecordReader();
    private final RecordWriter writer = new RecordWriter();

    private volatile Pages disk;
    private volatile boolean saveRequested;
    private volatile long saveThrough;
    private volatile boolean saveAtStepEnd;
    private volatile String protocolError;
    /** failForTest: the next step fails, and every one after it. */
    private volatile boolean failed;
    private volatile boolean failNext;

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
    /** Java op ids of events (TAKEN, DIED, TRADE, CARGO_CHANGED), with how often each arrived unskipped. */
    private final Map<Long, Integer> eventOps = new HashMap<>();
    private long eventOpDuplicates;
    private boolean started;
    /** Guarded by {@code worker}. */
    private boolean closed;

    private final ConcurrentLinkedQueue<Consumer<CommandOut>> script = new ConcurrentLinkedQueue<>();

    public StubEngine(Pages pages, Path pagesFile, int capacity) {
        this.disk = pages;
        this.pagesFile = pagesFile;
        this.capacity = capacity;
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
    public void requestSave(long throughObsSeq) {
        saveThrough = throughObsSeq;
        saveRequested = true;
    }

    @Override
    public void failForTest() {
        failNext = true;
    }

    @Override
    public void requestSaveAtNextStep() {
        saveAtStepEnd = true;
    }

    @Override
    public String describe() {
        return "stub engine (pure Java, simulates nothing)";
    }

    @Override
    public void close() {
        if (!stopWorker())
            return;
        synchronized (this) {
            if (pagesFile != null)
                savePages();
        }
    }

    /** Ends the session without the final save: a crash. Steps already submitted still run. */
    public void closeWithoutSaving() {
        stopWorker();
    }

    /** Stops the worker once; false if this engine was already closed (saved or crashed). */
    private boolean stopWorker() {
        synchronized (worker) {
            if (closed)
                return false;
            closed = true;
        }
        worker.shutdown();
        try {
            worker.awaitTermination(5, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        return true;
    }

    @Override
    public StubEngine crashAndReopen() {
        closeWithoutSaving();
        return new StubEngine(pages(), pagesFile, capacity);
    }

    // ---- the step -------------------------------------------------------------------------------

    private synchronized void step(long h) {
        steps++;
        if (failNext || failed) {
            failNext = false;
            failed = true;
            protocolError = "step " + h + ": failed on purpose (failForTest)";
            ByteBuffer fo = obs[(int) (h & 1)];
            if (Seq.getAcquire(fo, Layout.H_PUBLISHED) == h)
                Seq.setRelease(fo, Layout.H_CONSUMED, h);
            ByteBuffer fc = cmd[(int) (h & 1)];
            fc.putInt(Layout.H_USED, 0);
            fc.putInt(Layout.H_COUNT, 0);
            fc.putInt(Layout.H_FLAGS, Layout.CMD_FLAG_FAILED);
            fc.putLong(Layout.H_WORD_A, ackedObsSeq);
            fc.putLong(Layout.H_WORD_B, 0L);
            fc.putLong(Layout.H_WORD_C, 0L);
            fc.putLong(Layout.H_RESERVED, 0L);
            Seq.setRelease(fc, Layout.H_PUBLISHED, h);
            return;
        }
        ByteBuffer o = obs[(int) (h & 1)];
        if (Seq.getAcquire(o, Layout.H_PUBLISHED) != h) {
            protocolError = "step " + h + ": observation buffer not published";
            return;
        }
        long gameTime = o.getLong(Layout.H_WORD_B);
        reader.reset(o, Layout.HEADER_BYTES, o.getInt(Layout.H_USED));
        while (reader.next()) {
            // A save for a world save that knew observations up to saveThrough is taken right before
            // the first journaled record past it, so the pages never get ahead of that world save.
            if (saveRequested && Obs.isJournaled(reader.kind())
                    && reader.buffer().getLong(reader.at() + Obs.OBS_SEQ) > saveThrough) {
                saveRequested = false;
                savePages();
            }
            observe(h, reader);
        }
        if (reader.error() != null)
            protocolError = "step " + h + ": " + reader.error();
        Seq.setRelease(o, Layout.H_CONSUMED, h);

        if ((saveRequested && maxObsSeq >= saveThrough) || saveAtStepEnd) {
            saveRequested = false;
            saveAtStepEnd = false;
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
        if (kind == Obs.SESSION_START) {
            // The world model's list (docs/boundary.md §10.5): the stub keeps no habitat, so its list is END
            // alone, and the bridge's sampler starts at once.
            script(out -> out.reserve(com.minecraftindustrialcomplex.mic_rtsim.habitat.HabitatRecords.HABITAT_KNOWN,
                    com.minecraftindustrialcomplex.mic_rtsim.habitat.HabitatRecords.Known.ENTRIES,
                    com.minecraftindustrialcomplex.mic_rtsim.habitat.HabitatRecords.Known.FLAG_END));
        }
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
                } else {
                    int opAt = switch (kind) {
                        case Obs.TAKEN -> Obs.Taken.OP_ID;
                        case Obs.DIED -> Obs.Died.OP_ID;
                        case Obs.TRADE -> Obs.Trade.OP_ID;
                        case Obs.CARGO_CHANGED -> Obs.CargoChanged.OP_ID;
                        default -> -1;
                    };
                    if (opAt > 0 && r.size() >= opAt + 8) {
                        // An event's op id applies at most once: the engine counts the cargo it moves once.
                        if (eventOps.merge(b.getLong(at + opAt), 1, Integer::sum) > 1)
                            eventOpDuplicates++;
                    }
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

    @Override
    public void spawnGroup(long agent, int epoch, String groupKind, String dimension, double x, double y, double z,
                           List<MemberSpec> members) {
        script(spawnGroupCommand(agent, epoch, groupKind, dimension, x, y, z, members));
    }

    @Override
    public void spawnTogether(SpawnSpec... spawns) {
        List<Consumer<CommandOut>> commands = new ArrayList<>();
        for (SpawnSpec s : spawns)
            commands.add(spawnGroupCommand(s.agent(), s.epoch(), s.groupKind(), s.dimension(), s.x(), s.y(), s.z(), s.members()));
        script(out -> {
            for (Consumer<CommandOut> c : commands)
                c.accept(out);
        });
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

    @Override
    public void despawnGroup(long agent, int epoch) {
        script(out -> {
            int at = out.reserve(Cmd.DESPAWN_GROUP, Cmd.DespawnGroup.SIZE, 0);
            if (at < 0)
                return;
            out.buffer().putLong(at + Cmd.DespawnGroup.AGENT, agent);
            out.buffer().putInt(at + Cmd.DespawnGroup.EPOCH, epoch);
        });
    }

    @Override
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

    @Override
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

    @Override
    public synchronized void rewindCommandSequence(long next) {
        nextCmdSeq = next;
    }

    // ---- phase 1 scripting ----------------------------------------------------------------------

    /** A test scripts this village's phase 1 commands; the stub decides nothing, so there is nothing to switch off. */
    @Override
    public void scriptVillage(long village) {}

    /**
     * A scripted MARKET. The stub simulates nothing, so it prices the test's curves with the bridge's
     * test oracle ({@link PriceOracle}, rtsim-trade's math copied), into the bundles the native engine
     * computes from the same curves with rtsim-trade itself (choice PR2).
     */
    @Override
    public void market(long village, long asOfObsSeq, String currency, long treasury, long treasuryTarget, int worstNum, int worstDen,
                       List<GoodSpec> goods) {
        script(out -> {
            int cur = out.dict(Obs.DICT_ITEM, currency);
            int[] ids = new int[goods.size()];
            for (int i = 0; i < ids.length; i++)
                ids[i] = out.dict(Obs.DICT_ITEM, goods.get(i).item());
            int[] thresholds = PriceOracle.thresholds();
            int levels = thresholds.length, depth = PriceOracle.LADDER_DEPTH;
            int at = out.reserve(Cmd.MARKET, Cmd.Market.size(goods.size(), levels, depth), goods.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + Cmd.Market.VILLAGE, village);
            b.putLong(at + Cmd.Market.AS_OF_OBS_SEQ, asOfObsSeq);
            b.putInt(at + Cmd.Market.CURRENCY, cur);
            b.putLong(at + Cmd.Market.TREASURY, treasury);
            b.putLong(at + Cmd.Market.TREASURY_TARGET, treasuryTarget);
            b.put(at + Cmd.Market.LEVELS, (byte) levels);
            b.put(at + Cmd.Market.DEPTH, (byte) depth);
            for (int k = 0; k < levels; k++)
                b.putInt(at + Cmd.Market.THRESHOLDS + 4 * k, thresholds[k]);
            Ratio worst = new Ratio(Math.max(worstNum, 0), Math.max(worstDen, 1));
            for (int i = 0; i < ids.length; i++) {
                GoodSpec g = goods.get(i);
                int o = at + Cmd.Market.GOODS + i * Cmd.Market.good(levels, depth);
                b.putInt(o + Cmd.Market.G_ITEM, ids[i]);
                b.putLong(o + Cmd.Market.G_STOCK, g.stock());
                PriceCurve curve = new PriceCurve(g.anchor(), g.rNum(), g.rDen(), g.target());
                List<PriceOracle.Bundle>[][] ladders = PriceOracle.ladders(curve, g.stock(), treasury, treasuryTarget, worst);
                for (int side = 0; side < 2; side++)
                    for (int k = 0; k < levels; k++)
                        for (int j = 0; j < ladders[side][k].size(); j++) {
                            int st = o + Cmd.Market.step(levels, depth, side, k, j);
                            b.putInt(st, (int) ladders[side][k].get(j).items());
                            b.putInt(st + 4, (int) ladders[side][k].get(j).coins());
                        }
            }
        });
    }

    @Override
    public void merchantOffers(long agent, int epoch, int member, List<OfferSpec> offers) {
        script(out -> {
            int[][] ids = new int[offers.size()][3];
            for (int i = 0; i < offers.size(); i++) {
                OfferSpec o = offers.get(i);
                ids[i][0] = out.dict(Obs.DICT_ITEM, o.costA());
                ids[i][1] = o.costB() == null ? 0 : out.dict(Obs.DICT_ITEM, o.costB());
                ids[i][2] = out.dict(Obs.DICT_ITEM, o.result());
            }
            int at = out.reserve(Cmd.MERCHANT_OFFERS, Cmd.MerchantOffers.size(offers.size()), offers.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + Cmd.MerchantOffers.AGENT, agent);
            b.putInt(at + Cmd.MerchantOffers.EPOCH, epoch);
            b.putInt(at + Cmd.MerchantOffers.MEMBER, member);
            for (int i = 0; i < offers.size(); i++) {
                OfferSpec o = offers.get(i);
                int p = at + Cmd.MerchantOffers.OFFERS + i * Cmd.MerchantOffers.OFFER;
                b.putInt(p + Cmd.MerchantOffers.O_COST_A_ITEM, ids[i][0]);
                b.putInt(p + Cmd.MerchantOffers.O_COST_A_COUNT, o.costACount());
                b.putInt(p + Cmd.MerchantOffers.O_COST_B_ITEM, ids[i][1]);
                b.putInt(p + Cmd.MerchantOffers.O_COST_B_COUNT, o.costBCount());
                b.putInt(p + Cmd.MerchantOffers.O_RESULT_ITEM, ids[i][2]);
                b.putInt(p + Cmd.MerchantOffers.O_RESULT_COUNT, o.resultCount());
                b.putInt(p + Cmd.MerchantOffers.O_MAX_USES, o.maxUses());
                b.putFloat(p + Cmd.MerchantOffers.O_PRICE_MULT, 0.05f);
                b.putInt(p + Cmd.MerchantOffers.O_FLAGS, o.flags());
            }
        });
    }

    @Override
    public void writeGossip(long opId, long village, java.util.UUID player, int gossipType, int amount, int report, int flags) {
        script(out -> {
            int at = out.reserve(Cmd.WRITE_GOSSIP, Cmd.WriteGossip.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + Cmd.WriteGossip.OP_ID, opId);
            b.putLong(at + Cmd.WriteGossip.VILLAGE, village);
            b.putLong(at + Cmd.WriteGossip.PLAYER_MOST, player.getMostSignificantBits());
            b.putLong(at + Cmd.WriteGossip.PLAYER_LEAST, player.getLeastSignificantBits());
            b.putInt(at + Cmd.WriteGossip.GOSSIP, gossipType);
            b.putInt(at + Cmd.WriteGossip.AMOUNT, amount);
            b.putInt(at + Cmd.WriteGossip.REPORT, report);
            b.putInt(at + Cmd.WriteGossip.FLAGS, flags);
        });
    }

    @Override
    public void sentiment(long holder, int dislike, int friend, boolean replace, Map<java.util.UUID, Integer> entries) {
        List<Map.Entry<java.util.UUID, Integer>> list = new ArrayList<>(entries.entrySet());
        script(out -> {
            int at = out.reserve(Cmd.SENTIMENT, Cmd.Sentiment.size(list.size()), list.size());
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + Cmd.Sentiment.HOLDER, holder);
            b.putInt(at + Cmd.Sentiment.DISLIKE, dislike);
            b.putInt(at + Cmd.Sentiment.FRIEND, friend);
            b.putInt(at + Cmd.Sentiment.FLAGS, replace ? Cmd.Sentiment.FLAG_REPLACE : 0);
            for (int i = 0; i < list.size(); i++) {
                int e = at + Cmd.Sentiment.ENTRIES + i * Cmd.Sentiment.ENTRY;
                b.putLong(e, list.get(i).getKey().getMostSignificantBits());
                b.putLong(e + 8, list.get(i).getKey().getLeastSignificantBits());
                b.putInt(e + 16, list.get(i).getValue());
            }
        });
    }

    @Override
    public void placeHouse(long opId, long village, String dimension, String template, String processors, int x, int y, int z,
                           int rotation, int mirror, int flags, int radius, long seed, int yOffset) {
        script(out -> {
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int t = out.dict(Obs.DICT_TEMPLATE, template);
            int p = processors == null ? 0 : out.dict(Obs.DICT_PROCESSORS, processors);
            int at = out.reserve(Cmd.PLACE_HOUSE, Cmd.PlaceHouse.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + Cmd.PlaceHouse.OP_ID, opId);
            b.putLong(at + Cmd.PlaceHouse.VILLAGE, village);
            b.putInt(at + Cmd.PlaceHouse.DIM, dim);
            b.putInt(at + Cmd.PlaceHouse.TEMPLATE, t);
            b.putInt(at + Cmd.PlaceHouse.PROCESSORS, p);
            b.putInt(at + Cmd.PlaceHouse.X, x);
            b.putInt(at + Cmd.PlaceHouse.Y, y);
            b.putInt(at + Cmd.PlaceHouse.Z, z);
            b.put(at + Cmd.PlaceHouse.ROTATION, (byte) rotation);
            b.put(at + Cmd.PlaceHouse.MIRROR, (byte) mirror);
            b.putShort(at + Cmd.PlaceHouse.FLAGS, (short) flags);
            b.putInt(at + Cmd.PlaceHouse.RADIUS, radius);
            b.putLong(at + Cmd.PlaceHouse.SEED, seed);
            b.putInt(at + Cmd.PlaceHouse.Y_OFFSET, yOffset);
        });
    }

    @Override
    public void spawnResident(long opId, long village, String dimension, String body, String profession, int flags,
                              double x, double y, double z, long house) {
        script(out -> {
            int dim = out.dict(Obs.DICT_DIMENSION, dimension);
            int bd = out.dict(Obs.DICT_BODY, body);
            int pr = profession == null ? 0 : out.dict(Obs.DICT_PROFESSION, profession);
            int at = out.reserve(Cmd.SPAWN_RESIDENT, Cmd.SpawnResident.SIZE, 0);
            if (at < 0)
                return;
            ByteBuffer b = out.buffer();
            b.putLong(at + Cmd.SpawnResident.OP_ID, opId);
            b.putLong(at + Cmd.SpawnResident.VILLAGE, village);
            b.putInt(at + Cmd.SpawnResident.DIM, dim);
            b.putInt(at + Cmd.SpawnResident.BODY, bd);
            b.putInt(at + Cmd.SpawnResident.PROFESSION, pr);
            b.putInt(at + Cmd.SpawnResident.FLAGS, flags);
            b.putDouble(at + Cmd.SpawnResident.X, x);
            b.putDouble(at + Cmd.SpawnResident.Y, y);
            b.putDouble(at + Cmd.SpawnResident.Z, z);
            b.putLong(at + Cmd.SpawnResident.HOUSE, house);
        });
    }

    // ---- inspection -----------------------------------------------------------------------------

    /** The last saved pages: what survives a crash. */
    public Pages pages() {
        return disk;
    }

    @Override
    public synchronized List<Received> received() {
        return Collections.unmodifiableList(new ArrayList<>(received));
    }

    @Override
    public synchronized List<Received> received(int kind) {
        List<Received> out = new ArrayList<>();
        for (Received r : received)
            if (r.kind() == kind)
                out.add(r);
        return out;
    }

    @Override
    public synchronized long ledger(long store) {
        return ledger.getOrDefault(store, 0L);
    }

    @Override
    public synchronized boolean opApplied(long opId) {
        return appliedOps.contains(opId);
    }

    @Override
    public synchronized long steps() {
        return steps;
    }

    @Override
    public synchronized long staleSkipped() {
        return staleSkipped;
    }

    @Override
    public synchronized long opDuplicates() {
        return opDuplicates;
    }

    /** How many times an event op id (TAKEN, DIED, TRADE, CARGO_CHANGED) arrived unskipped; the engine applies it once. */
    @Override
    public synchronized int eventOpCount(long opId) {
        return eventOps.getOrDefault(opId, 0);
    }

    public synchronized long eventOpDuplicates() {
        return eventOpDuplicates;
    }

    @Override
    public synchronized String javaDictName(int id) {
        return javaDict.get(id);
    }

    @Override
    public synchronized long maxObsSeq() {
        return maxObsSeq;
    }

    @Override
    public long pagesObsSeq() {
        return disk.obsSeq();
    }

    @Override
    public int scriptedPending() {
        return script.size();
    }

    @Override
    public String protocolError() {
        return protocolError;
    }

    @Override
    public void awaitIdle() {
        try {
            worker.submit(() -> {}).get(10, TimeUnit.SECONDS);
        } catch (Exception e) {
            throw new IllegalStateException("stub engine worker stuck", e);
        }
    }
}
