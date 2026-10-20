package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Engine;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Handover;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Seq;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.SessionStart;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.engine.StubEngine;
import com.minecraftindustrialcomplex.mic_rtsim.journal.Journal;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.neoforged.fml.loading.FMLPaths;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDateTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The hand-over cost per tick (PLAN.md §4.5, a phase 0 measurement, §14): from Java's {@code submit}
 * to the engine's {@code published}, on the real native engine (rtsim-jni on mic_native's background
 * pool) and, for comparison, the pure-Java stub. Off unless {@code -Dmic_rtsim.bench=true}
 * ({@code ./gametest.sh -PrtsimBench}); it then writes {@code mic_rtsim-bench/handover-<time>.json}
 * into the game directory.
 *
 * <p>Each engine gets its own boundary session (no Minecraft in the loop), and the server thread
 * drives it:
 * <ul>
 *   <li><b>burst</b>: 2,000 hand-overs back to back inside one tick, each waited for by spinning on
 *       the command buffer's {@code published} word: the pool thread is warm;</li>
 *   <li><b>cadence</b>: one hand-over per server tick for 100 ticks, each after a 50 ms sleep (the
 *       GameTest server ticks as fast as it can; a real one every 50 ms), so the pool thread is idle
 *       between steps and each one pays its wake-up.</li>
 * </ul>
 * Payloads (PLACEHOLDERS, not a measured tick): the preamble only (a TICK record), 30 PUPPET_STATE
 * records (1.7 KB), and 1,000 (56 KB, a fifth of the 256 KiB default buffer).
 *
 * <p>Per hand-over it records: {@code handOver} (Java's whole call: preamble, journal drain, header,
 * the release store and the JNI submit), {@code submitCall} (the submit call alone),
 * {@code submitToPublish} (from the submit call's start to Java seeing {@code published}, acquire),
 * and for the native engine {@code step} (the step's own time on the pool thread, from its stats).
 * Spinning is for the bench only: the bridge never waits.
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class HandoverBenchGameTests {
    private HandoverBenchGameTests() {}

    public static final boolean ENABLED = Boolean.getBoolean("mic_rtsim.bench");

    private static final int BURST = 2000;
    private static final int CADENCE = 100;
    /** The gap before each cadence hand-over: one server tick. */
    private static final long GAP_MS = 50;
    private static final int[] PAYLOADS = {0, 30, 1000};
    private static final String[] ENGINES = {"native", "native-p1", "stub"};

    /** Times the submit call and remembers when it started. */
    private static final class TimedEngine implements Engine {
        final Engine delegate;
        long lastStart;
        long lastCall;

        TimedEngine(Engine delegate) {
            this.delegate = delegate;
        }

        @Override
        public ByteBuffer observationBuffer(int index) {
            return delegate.observationBuffer(index);
        }

        @Override
        public ByteBuffer commandBuffer(int index) {
            return delegate.commandBuffer(index);
        }

        @Override
        public SessionStart start(long lastAppliedCmdSeq) {
            return delegate.start(lastAppliedCmdSeq);
        }

        @Override
        public void submit(long handover) {
            long t = System.nanoTime();
            delegate.submit(handover);
            lastCall = System.nanoTime() - t;
            lastStart = t;
        }

        @Override
        public void requestSave(long throughObsSeq) {
            delegate.requestSave(throughObsSeq);
        }

        @Override
        public String describe() {
            return delegate.describe();
        }

        @Override
        public void close() {
            delegate.close();
        }
    }

    private static final Handover.CommandSink SINK = new Handover.CommandSink() {
        @Override
        public void onHeader(long handover, long ackedObsSeq, long gameTime, int flags) {}

        @Override
        public boolean accept(ByteBuffer buffer, int at, int kind, int size, int aux) {
            return true;
        }

        @Override
        public void onMalformed(long handover, String error) {
            throw new IllegalStateException(error);
        }
    };

    /** One configuration's samples. */
    private static final class Series {
        final String engine;
        final String mode;
        final int payload;
        final long[] handOver;
        final long[] submitCall;
        final long[] submitToPublish;
        final long[] step;
        int n;

        Series(String engine, String mode, int payload, int size) {
            this.engine = engine;
            this.mode = mode;
            this.payload = payload;
            this.handOver = new long[size];
            this.submitCall = new long[size];
            this.submitToPublish = new long[size];
            this.step = new long[size];
        }

        String key() {
            return engine + "/" + mode + "/" + payload;
        }
    }

    /** One engine under test with its own boundary session. */
    private static final class Rig {
        final String name;
        final TimedEngine engine;
        final BoundarySession session;
        long time = 1;
        /** Phase 1 runs (the native-p1 rig): players reported with every hand-over. */
        int[] playerDims = new int[0];

        Rig(String name, Engine engine) {
            this(name, engine, 0);
        }

        Rig(String name, Engine engine, int moduleMask) {
            this.name = name;
            this.engine = new TimedEngine(engine);
            this.session = new BoundarySession(this.engine, new Journal(), moduleMask, time);
        }

        void payload(int records) {
            for (int p = 0; p < playerDims.length; p++) {
                int at = session.direct(Obs.PLAYER, Obs.Player.SIZE, playerDims[p], time);
                if (at < 0)
                    break;
                ByteBuffer b = session.openBuffer();
                b.putLong(at + Obs.Player.UUID_MOST, 0x5EED_0000L + p);
                b.putLong(at + Obs.Player.UUID_LEAST, 0x5EED_1111L + p);
                // Each player stands 30 blocks off a village of the grid (P1_VILLAGE_GRID).
                b.putDouble(at + Obs.Player.X, (p % 2) * 4 * P1_VILLAGE_SPACING + 30);
                b.putDouble(at + Obs.Player.Y, 64);
                b.putDouble(at + Obs.Player.Z, (p / 2) * 4 * P1_VILLAGE_SPACING + 30);
            }
            for (int i = 0; i < records; i++) {
                int at = session.direct(Obs.PUPPET_STATE, Obs.PuppetState.SIZE, i % 3, time);
                if (at < 0)
                    break;
                ByteBuffer b = session.openBuffer();
                b.putLong(at + Obs.PuppetState.AGENT, 1_000 + i / 3);
                b.putInt(at + Obs.PuppetState.EPOCH, 1);
                b.putDouble(at + Obs.PuppetState.X, i);
            }
        }

        /** One hand-over, waited for. Returns false when the protocol held it back (it never should). */
        boolean once(Series s, int payload) {
            session.applyCommands(SINK);
            payload(payload);
            long t0 = System.nanoTime();
            boolean submitted = session.handOver(time++);
            long t1 = System.nanoTime();
            if (!submitted)
                return false;
            long h = session.handover().submitted();
            ByteBuffer c = engine.commandBuffer((int) (h & 1));
            long deadline = t1 + 5_000_000_000L;
            while (Seq.getAcquire(c, Layout.H_PUBLISHED) != h) {
                Thread.onSpinWait();
                if (System.nanoTime() - deadline > 0)
                    throw new IllegalStateException(name + ": step " + h + " never published");
            }
            long t2 = System.nanoTime();
            if (engine.delegate instanceof NativeEngine ne)
                while (ne.steps() < h) // the step's last lines, after its publish
                    Thread.onSpinWait();
            s.handOver[s.n] = t1 - t0;
            s.submitCall[s.n] = engine.lastCall;
            s.submitToPublish[s.n] = t2 - engine.lastStart;
            s.step[s.n] = engine.delegate instanceof NativeEngine ne ? ne.lastStepNanos() : -1;
            s.n++;
            // The commands of step h are applied at the next call, as the bridge does.
            return true;
        }

        void close() {
            session.applyCommands(SINK);
            engine.close();
        }
    }

    private static Rig nativeRig() {
        Path dir = FMLPaths.GAMEDIR.get().resolve("mic_rtsim-test").resolve("bench-" + UUID.randomUUID());
        return new Rig("native", NativeEngine.open(dir, 0, NativeEngine.FLAG_NO_FSYNC));
    }

    /** The phase 1 rig's world: villages on a grid this many blocks apart, this many a side. */
    private static final int P1_VILLAGE_SPACING = 500;
    private static final int P1_VILLAGE_GRID = 8;
    private static final String[] P1_STRUCTURES = {"minecraft:village_plains", "minecraft:village_taiga",
            "minecraft:village_desert", "minecraft:village_savanna", "minecraft:village_snowy"};

    /**
     * The native engine with phase 1 running: 64 villages adopted (an 8 x 8 grid, five structures),
     * the world run 10 days ahead so its flows, news and caravans are going, and four players each
     * reported with every hand-over (the bridge reports a player every 20 ticks, so this is more),
     * near enough to villages for MARKET snapshots and roads for caravans to materialize.
     */
    private static Rig nativePhase1Rig() {
        Path dir = FMLPaths.GAMEDIR.get().resolve("mic_rtsim-test").resolve("bench-p1-" + UUID.randomUUID());
        NativeEngine ne = NativeEngine.open(dir, 0, NativeEngine.FLAG_NO_FSYNC | NativeEngine.FLAG_TEST);
        Rig r = new Rig("native-p1", ne, 0b1111); // knowledge, settlements, trade, travel
        int dim = r.session.dictId(Obs.DICT_DIMENSION, "minecraft:overworld");
        int human = r.session.dictId(Obs.DICT_CULTURE, "human");
        for (int i = 0; i < P1_VILLAGE_GRID * P1_VILLAGE_GRID; i++) {
            int x = (i % P1_VILLAGE_GRID) * P1_VILLAGE_SPACING, z = (i / P1_VILLAGE_GRID) * P1_VILLAGE_SPACING;
            int structure = r.session.dictId(Obs.DICT_STRUCTURE, P1_STRUCTURES[(i * 7 + i / P1_VILLAGE_GRID) % P1_STRUCTURES.length]);
            ByteBuffer b = r.session.beginJournaled(Obs.VILLAGE_ADOPTED, Obs.VillageAdopted.SIZE, 0, r.time);
            b.putLong(Obs.VillageAdopted.VILLAGE, Obs.villageId(dim, x >> 4, z >> 4));
            b.putInt(Obs.VillageAdopted.DIM, dim);
            b.putInt(Obs.VillageAdopted.STRUCTURE, structure);
            b.putInt(Obs.VillageAdopted.CULTURE, human);
            b.putInt(Obs.VillageAdopted.PIECES, 45);
            b.putInt(Obs.VillageAdopted.MIN_X, x - 30);
            b.putInt(Obs.VillageAdopted.MIN_Y, 60);
            b.putInt(Obs.VillageAdopted.MIN_Z, z - 30);
            b.putInt(Obs.VillageAdopted.MAX_X, x + 30);
            b.putInt(Obs.VillageAdopted.MAX_Y, 80);
            b.putInt(Obs.VillageAdopted.MAX_Z, z + 30);
            b.putInt(Obs.VillageAdopted.MEET_X, x);
            b.putInt(Obs.VillageAdopted.MEET_Y, 64);
            b.putInt(Obs.VillageAdopted.MEET_Z, z);
            r.session.commit(b);
        }
        ne.phase1Advance(10 * 24_000L);
        r.playerDims = new int[] {dim, dim, dim, dim};
        return r;
    }

    private static Rig stubRig() {
        return new Rig("stub", new StubEngine(StubEngine.Pages.EMPTY, null, StubEngine.DEFAULT_CAPACITY));
    }

    /** The bench's state machine, one call per server tick. */
    private static final class Bench {
        final List<Series> done = new ArrayList<>();
        final String loadBefore = loadAverage();
        final long startedAt = System.currentTimeMillis();
        Rig rig;
        Series cadence;
        int phase;
        boolean finished;

        void tick() {
            if (finished)
                return;
            // Phases: for each engine (native, native with phase 1 running, stub) and payload, a burst
            // then a cadence run.
            int perEngine = PAYLOADS.length * 2;
            if (phase >= ENGINES.length * perEngine) {
                finish();
                return;
            }
            String engineName = ENGINES[phase / perEngine];
            int within = phase % perEngine;
            int payload = PAYLOADS[within / 2];
            boolean burst = within % 2 == 0;
            if (rig == null)
                rig = switch (engineName) {
                    case "native" -> nativeRig();
                    case "native-p1" -> nativePhase1Rig();
                    default -> stubRig();
                };
            if (burst) {
                Series s = new Series(engineName, "burst", payload, BURST);
                // Warm-up: the JIT and the pool, not counted.
                Series warm = new Series(engineName, "warmup", payload, 200);
                for (int i = 0; i < 200; i++)
                    check("warm-up hand-over", rig.once(warm, payload));
                for (int i = 0; i < BURST; i++)
                    check("burst hand-over " + i, rig.once(s, payload));
                done.add(s);
                phase++;
                return;
            }
            if (cadence == null)
                cadence = new Series(engineName, "cadence", payload, CADENCE);
            // The GameTest server ticks as fast as it can; a real server ticks every 50 ms, and the
            // pool thread sleeps in between. Give it that gap, so each step pays its wake-up.
            try {
                Thread.sleep(GAP_MS);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException(e);
            }
            check("cadence hand-over", rig.once(cadence, payload));
            if (cadence.n == CADENCE) {
                done.add(cadence);
                cadence = null;
                phase++;
                if (phase % perEngine == 0) {
                    rig.close();
                    rig = null;
                }
            }
        }

        void finish() {
            finished = true;
            StringBuilder json = new StringBuilder();
            json.append("{\n  \"bench\": \"handover\",\n");
            json.append("  \"started\": \"").append(LocalDateTime.now().format(DateTimeFormatter.ISO_LOCAL_DATE_TIME)).append("\",\n");
            json.append("  \"wallSeconds\": ").append((System.currentTimeMillis() - startedAt) / 1000.0).append(",\n");
            json.append("  \"cpus\": ").append(Runtime.getRuntime().availableProcessors()).append(",\n");
            json.append("  \"java\": \"").append(System.getProperty("java.vm.name")).append(' ').append(System.getProperty("java.version")).append("\",\n");
            json.append("  \"os\": \"").append(System.getProperty("os.name")).append(' ').append(System.getProperty("os.version")).append("\",\n");
            json.append("  \"loadBefore\": \"").append(loadBefore).append("\",\n");
            json.append("  \"loadAfter\": \"").append(loadAverage()).append("\",\n");
            json.append("  \"cadenceGapMs\": ").append(GAP_MS).append(",\n");
        json.append("  \"unit\": \"ns\",\n  \"series\": [\n");
            for (int i = 0; i < done.size(); i++) {
                Series s = done.get(i);
                json.append("    {\"engine\": \"").append(s.engine).append("\", \"mode\": \"").append(s.mode)
                        .append("\", \"payloadRecords\": ").append(s.payload).append(", \"n\": ").append(s.n)
                        .append(",\n     \"handOver\": ").append(stats(s.handOver, s.n))
                        .append(",\n     \"submitCall\": ").append(stats(s.submitCall, s.n))
                        .append(",\n     \"submitToPublish\": ").append(stats(s.submitToPublish, s.n));
                if (s.engine.equals("native"))
                    json.append(",\n     \"step\": ").append(stats(s.step, s.n));
                json.append("}").append(i + 1 < done.size() ? ",\n" : "\n");
                record("handover-bench", s.key(), String.format(Locale.ROOT,
                        "submitToPublish p50 %.1f us p99 %.1f us; submit call p50 %.2f us; handOver p50 %.2f us%s",
                        pct(s.submitToPublish, s.n, 50) / 1000.0, pct(s.submitToPublish, s.n, 99) / 1000.0,
                        pct(s.submitCall, s.n, 50) / 1000.0, pct(s.handOver, s.n, 50) / 1000.0,
                        s.engine.equals("native") ? String.format(Locale.ROOT, "; step p50 %.1f us", pct(s.step, s.n, 50) / 1000.0) : ""));
            }
            json.append("  ]\n}\n");
            Path out = FMLPaths.GAMEDIR.get().resolve("mic_rtsim-bench")
                    .resolve("handover-" + LocalDateTime.now().format(DateTimeFormatter.ofPattern("yyyy-MM-dd-HHmmss")) + ".json");
            try {
                Files.createDirectories(out.getParent());
                Files.writeString(out, json.toString());
                record("handover-bench", "results", out.toAbsolutePath());
            } catch (IOException e) {
                throw new IllegalStateException("cannot write " + out, e);
            }
        }
    }

    private static long pct(long[] v, int n, int p) {
        long[] s = Arrays.copyOf(v, n);
        Arrays.sort(s);
        return s[Math.min(n - 1, (int) Math.ceil(p / 100.0 * n) - 1)];
    }

    private static String stats(long[] v, int n) {
        long[] s = Arrays.copyOf(v, n);
        Arrays.sort(s);
        double mean = Arrays.stream(s).average().orElse(0);
        return String.format(Locale.ROOT, "{\"min\": %d, \"p50\": %d, \"p90\": %d, \"p99\": %d, \"max\": %d, \"mean\": %.0f}",
                s[0], pct(s, n, 50), pct(s, n, 90), pct(s, n, 99), s[n - 1], mean);
    }

    private static String loadAverage() {
        try {
            return Files.readString(Path.of("/proc/loadavg")).trim();
        } catch (IOException | RuntimeException e) {
            return "unknown";
        }
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 4800, batch = "mic_rtsim_bench")
    public static void handOverCostPerTick(GameTestHelper helper) {
        if (!ENABLED) {
            record("handover-bench", "skipped", "run with -PrtsimBench (-Dmic_rtsim.bench=true)");
            helper.succeed();
            return;
        }
        Bench bench = new Bench();
        helper.onEachTick(bench::tick);
        helper.succeedWhen(() -> check("the bench finished", bench.finished));
    }
}
