package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassConfig;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.GlassExitKit;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.world.level.block.Rotation;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.stream.Collectors;

/**
 * G1's exit test (GLASSBOX.md §11.3): each corpus factory is built and loaded until the engine has judged a
 * checksum window of its compile, and the window passes when every exact good is within the checksum
 * tolerance and every bounded good inside [L, U] (the engine's verdicts, logged with their numbers). The
 * solve's rounds are logged on a {@code [gametest] glass exit} line per factory, which the results table
 * is built from.
 *
 * <p>Both engines: the native engine judges its own run; the stub's run is judged offline by the same Rust
 * code on the records the stub received (choice GB80). Without the natives ({@code -PskipJni}) the test
 * logs that it cannot judge and passes once the compile reached the stub.
 *
 * <p>A factory with regime phases (the boiler that loses a pump's water) is held at the end of its first
 * phase until the engine has judged a window there, then let into the next phase, and judged again on a
 * window that starts after the change settled. {@code MIC_GLASS_ONLY} limits the run to the named factories, as the smoke tests do.
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class GlassExitGameTests {
    private GlassExitGameTests() {}

    /** The checksum window the exit test asks for (END's window, GB7): 2,400 ticks, 120 sweeps at 20. */
    public static final int WINDOW = 2400;
    /** Ticks between report polls (and recorder polls, far inside the received log's 10,000 records). */
    static final int POLL = 100;
    /** Slack on top of warm-up and windows: compile, the engine's warm-up and resends. */
    static final int SLACK = 4000;

    @GameTestGenerator
    public static Collection<TestFunction> glassExit() {
        // MIC_GLASS_SUITE=smoke or exit runs only that suite of the corpus (iterating); unset, both run.
        String suite = System.getenv("MIC_GLASS_SUITE");
        if (suite != null && !suite.isBlank() && !suite.trim().equals("exit"))
            return List.of();
        String only = System.getenv("MIC_GLASS_ONLY");
        Set<String> filter = only == null || only.isBlank() ? Set.of()
                : Arrays.stream(only.split(",")).map(String::trim).filter(s -> !s.isEmpty()).collect(Collectors.toSet());
        List<TestFunction> out = new ArrayList<>();
        for (CorpusFactory f : GlassCorpus.ALL) {
            if (!filter.isEmpty() && !filter.contains(f.name()))
                continue;
            int maxTicks = f.warmupTicks() + 2 * (WINDOW + SLACK) + 7200;
            out.add(new TestFunction("glassexit_" + f.name(), "glassexit." + f.name().toLowerCase(Locale.ROOT),
                    MicRtsimCreate.MODID + ":" + IndustryKit.YARD, Rotation.NONE, maxTicks, 0L, true, false, 1, 1, true,
                    helper -> exit(helper, f)));
        }
        return out;
    }

    /** One factory's exit test state, ticked by the helper. */
    static final class Run {
        final GameTestHelper helper;
        final CorpusFactory f;
        final GlassExitKit.Recorder rec;
        CorpusFactory.Built built;
        District district;
        int ticks;
        /** The window tick the factory's perturb sees; held at a phase's last tick until it is judged. */
        int windowTick = -1;
        int phase;
        /** Game time the current phase began; only a window starting after it is judged for it. */
        long phaseStart;
        /** (compile, window start) of the windows judged, in order. */
        final List<long[]> judged = new ArrayList<>();
        final List<String> failures = new ArrayList<>();
        boolean done;
        /** The last report seen, for the timeout's message. */
        String last = "no report yet";

        Run(GameTestHelper helper, CorpusFactory f) {
            this.helper = helper;
            this.f = f;
            this.rec = GlassExitKit.start();
        }
    }

    static void exit(GameTestHelper helper, CorpusFactory f) {
        GlassConfig.Test.set(GlassConfig.CHECKSUM_WINDOW_TICKS, WINDOW);
        Run run = new Run(helper, f);
        run.built = GlassCorpus.prepare(helper, f);
        GlassCorpusKit.log(f.name(), "exit built", run.built.notes());
        helper.onEachTick(() -> tick(run));
        helper.succeedWhen(() -> {
            if (!run.failures.isEmpty())
                throw new GameTestAssertException(String.join("; ", run.failures));
            if (!run.done)
                throw new GameTestAssertException("waiting for the glass box's windows of " + f.name() + " (" + run.judged.size() + " judged; "
                        + run.last + ")");
        });
    }

    static void tick(Run run) {
        if (run.done)
            return;
        run.ticks++;
        CorpusFactory f = run.f;
        // The factory's own clock: its warm-up, then the window ticks its perturbations are written for.
        if (run.ticks > f.warmupTicks()) {
            List<CorpusFactory.Phase> phases = run.built.effectivePhases();
            // Held at the current phase's last tick until its window is judged; the next phase's change
            // (at that phase's end) comes only then.
            int holdAt = run.phase + 1 < phases.size() ? phases.get(run.phase).to() - 1 : Integer.MAX_VALUE;
            if (run.windowTick < holdAt)
                run.windowTick++;
            f.perturb(run.helper, run.built, run.windowTick);
        }
        if (run.ticks % POLL != 0)
            return;
        run.rec.poll();
        if (run.district == null) {
            run.district = IndustryKit.districtAt(run.helper, run.built.anchor());
            if (run.district == null)
                return;
        }
        GlassReport r;
        try {
            r = GlassExitKit.report(run.district.id, run.rec);
        } catch (RuntimeException e) {
            run.failures.add("the report failed: " + e.getMessage());
            return;
        }
        if (!GlassExitKit.canJudge()) {
            // -PskipJni: nothing can judge; the test only shows the compile reached the engine.
            if (IndustryKit.received(com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords.IND_GRAPH_END,
                    com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords.DISTRICT, run.district.id).isEmpty())
                return;
            IndustryKit.record("glass exit " + f.name(), "not judged", "no natives in this run (-PskipJni)");
            run.done = true;
            return;
        }
        run.last = r == null ? "no complete compile, " + run.rec.size() + " records kept" : r.summary();
        if (run.ticks % 1200 == 0)
            IndustryKit.record("glass exit " + f.name(), "status", run.last);
        if (r == null || r.windowsCompleted() == 0 || r.verdict() == GlassReport.NONE || r.windowStart() < run.phaseStart)
            return;
        if (!run.judged.isEmpty() && run.judged.get(run.judged.size() - 1)[1] == r.windowStart())
            return; // already judged
        run.judged.add(new long[] {r.compile(), r.windowStart()});
        String test = "glass exit " + f.name() + " phase " + run.phase;
        List<String> failed = GlassExitKit.judge(test, r, id -> IndustryKit.ind().goodName(id));
        MicRtsim.LOGGER.info(String.format(Locale.ROOT,
                "[gametest] glass exit %s: phase %d, compile %d, %d nodes, %d edges, %d rounds (%s), solve %.0f us, %d goods, %s",
                f.name(), run.phase, r.compile(), r.nodes(), r.edges(), r.rounds(), r.converged() ? "converged" : "NOT converged",
                r.solveMicros(), r.goods().size(), failed.isEmpty() ? "pass" : "FAIL " + failed));
        if (!failed.isEmpty()) {
            run.failures.add(f.name() + " phase " + run.phase + ": " + failed);
            GlassExitKit.logGraph(test, run.district.id, run.rec);
        }
        List<CorpusFactory.Phase> phases = run.built.effectivePhases();
        if (run.phase + 1 < phases.size()) {
            // The next phase starts once the change at this phase's end has settled (the factory's own gap).
            run.phaseStart = run.helper.getLevel().getGameTime() + 1 + phases.get(run.phase + 1).from() - phases.get(run.phase).to();
            run.phase++;
            IndustryKit.record(test, "next phase", phases.get(run.phase).name());
        } else {
            run.done = true;
        }
    }
}
