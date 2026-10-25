package com.minecraftindustrialcomplex.mic_rtsim_create.bench;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Territories;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainSampler;
import com.simibubi.create.Create;
import com.simibubi.create.content.trains.entity.Train;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

import java.util.Arrays;

/**
 * The train sampler's bench (MZB-331's sampler half; docs/choices-mowzie-build.md M7 design): how long
 * {@link TrainSampler} takes with {@link FarTrainBench}'s synthetic trains running. Only with
 * {@code -Dmic_rtsim.samplerbench=true} and {@code -Dmic_rtsim.farbench=true} (its trains:
 * {@code mic_rtsim.farbench.trains}, 200 for the budget's figure); {@code ./gradlew :create:runSamplerBench
 * -PskipJni} sets both and stops by itself ({@code SamplerBenchGameTests}).
 *
 * <p>It puts {@link #TERRITORIES} circles of radius {@link #RADIUS} on the bench's two rings (half on each,
 * evenly spaced, centred on the track's circle) into the conflict bridge's territories, each minding the rail,
 * under made-up faction keys the engine does not know (it drops their RAIL_PASS). The trains cross them at
 * 0.6 blocks a tick, so every window sends passes as well as testing segments: the cost includes the journal
 * writes. Every {@value #WINDOW} ticks it logs {@code [samplerbench]}: the samples taken, the passes sent, the
 * ns per sample (median and 99th percentile) and the ns per tick of the sampler's handler (mean and 99th
 * percentile). The budget is 2 microseconds a sample (PLACEHOLDER), 20 per tick at 200 trains. The first window
 * includes the JIT warming up.
 *
 * <p>The windows are counted from the circles' placement, which waits for the conflict bridge to come up, so a
 * late bridge delays them and drops none: {@code SamplerBenchGameTests} ends the run when {@link #done()}.
 */
public final class SamplerBench {
    public static final boolean ENABLED = Boolean.getBoolean("mic_rtsim.samplerbench");
    public static final int TERRITORIES = Math.max(0, Integer.getInteger("mic_rtsim.samplerbench.territories", 64));
    /** Windows to run before the bench's GameTest ends the server (0: no end, until the server is stopped). */
    public static final int WINDOWS = Math.max(0, Integer.getInteger("mic_rtsim.samplerbench.windows", 6));
    public static final int WINDOW = 1200;
    static final int RADIUS = Integer.getInteger("mic_rtsim.samplerbench.radius", 40);
    /** Made-up faction keys: namespace byte 0x7E, which no engine hands out. */
    static final long FACTION_BASE = 0x7E00_0000_0000_0000L;

    private static long[] sampleNs = new long[1 << 14], tickNs = new long[WINDOW];
    private static int samples, ticks;
    private static long lastSamples, lastPasses, windows;
    private static boolean placed;

    private SamplerBench() {}

    /** True once {@link #WINDOWS} windows have been reported (never with 0 windows, which runs until stopped). */
    public static boolean done() {
        return WINDOWS > 0 && windows >= WINDOWS;
    }

    /** Where the run stands, for the GameTest's failure when it times out first. */
    public static String progress() {
        return windows + " of " + WINDOWS + " windows" + (placed ? "" : ", the circles never placed (the conflict bridge never came up)");
    }

    public static void register() {
        if (!FarTrainBench.ENABLED || FarTrainBench.TRAINS == 0)
            MicRtsimCreate.LOGGER.warn("[samplerbench] no trains: set -Dmic_rtsim.farbench=true and mic_rtsim.farbench.trains");
        TrainSampler.bench(SamplerBench::sampled, SamplerBench::ticked);
        // After the sampler's own handler (registered first), at the same priority
        NeoForge.EVENT_BUS.addListener(SamplerBench::tick);
    }

    private static void sampled(long ns) {
        if (samples == sampleNs.length)
            sampleNs = Arrays.copyOf(sampleNs, samples * 2);
        sampleNs[samples++] = ns;
    }

    private static void ticked(long ns) {
        if (ticks < tickNs.length)
            tickNs[ticks] = ns;
    }

    private static void tick(ServerTickEvent.Post event) {
        if (!placed) {
            if (RtsimApi.conflictUp())
                place();
            return;
        }
        if (++ticks < WINDOW)
            return;
        report();
        ticks = 0;
        samples = 0;
    }

    /** Circles on the rings: the first half on the inner ring, the rest on the outer. */
    private static void place() {
        Territories t = RtsimApi.territories();
        int inner = TERRITORIES / 2, outer = TERRITORIES - inner;
        for (int i = 0; i < TERRITORIES; i++) {
            boolean in = i < inner;
            int k = in ? i : i - inner, of = in ? inner : outer;
            double r = FarTrainBench.RADII[in ? 0 : 1], a = 2.0 * Math.PI * k / Math.max(1, of);
            int x = (int) Math.round(FarTrainBench.CENTRE_X + r * Math.cos(a)), z = (int) Math.round(FarTrainBench.CENTRE_Z + r * Math.sin(a));
            t.put(new Territories.Territory(FACTION_BASE + i, Level.OVERWORLD, 1 << ConflictRecords.SIGNAL_RAIL, x, z, RADIUS));
        }
        placed = true;
        lastSamples = TrainSampler.samples;
        lastPasses = TrainSampler.passes;
        MicRtsimCreate.LOGGER.info("[samplerbench] {} territories of radius {} on the rings, {} trains", TERRITORIES, RADIUS,
                FarTrainBench.TRAINS);
    }

    private static void report() {
        long[] s = Arrays.copyOf(sampleNs, samples);
        long[] k = Arrays.copyOf(tickNs, Math.min(ticks, tickNs.length));
        Arrays.sort(s);
        Arrays.sort(k);
        long sum = 0;
        for (long v : k)
            sum += v;
        long taken = TrainSampler.samples - lastSamples, sent = TrainSampler.passes - lastPasses;
        lastSamples = TrainSampler.samples;
        lastPasses = TrainSampler.passes;
        windows++;
        // A derailed train is skipped by the sampler: a bench of them measures nothing
        int derailed = 0;
        for (Train t : Create.RAILWAYS.trains.values())
            if (t.derailed)
                derailed++;
        MicRtsimCreate.LOGGER.info("[samplerbench] window {}: {} samples, {} passes sent, {} tracked, {} derailed; ns per sample: median {}, "
                        + "p99 {}, max {}; ns per tick: mean {}, p99 {}, max {}",
                windows, taken, sent, TrainSampler.tracked(), derailed, at(s, 0.5), at(s, 0.99), at(s, 1.0),
                k.length == 0 ? 0 : sum / k.length, at(k, 0.99), at(k, 1.0));
        if (derailed > 0)
            MicRtsimCreate.LOGGER.warn("[samplerbench] {} trains are derailed: this window's numbers are not the sampler's work", derailed);
    }

    private static long at(long[] sorted, double q) {
        return sorted.length == 0 ? 0 : sorted[Math.min(sorted.length - 1, (int) Math.floor(q * (sorted.length - 1) + 0.5))];
    }
}
