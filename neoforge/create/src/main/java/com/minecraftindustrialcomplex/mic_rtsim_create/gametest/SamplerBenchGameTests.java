package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_create.bench.SamplerBench;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.world.level.block.Rotation;
import net.neoforged.neoforge.gametest.GameTestHolder;

import java.util.Collection;
import java.util.List;

/**
 * Keeps a GameTest server running for {@link SamplerBench}'s windows, then ends it (a GameTest server stops
 * when its tests are done). The test succeeds when the bench reports its last window ({@link SamplerBench#done()}),
 * not after a fixed number of ticks from its own start: the windows begin when the conflict bridge is up and the
 * circles are placed, which may be later than the test's start, and a fixed end would drop the last window. With
 * {@code mic_rtsim.samplerbench.windows} 0 it never succeeds and has no timeout: the server runs until it is
 * stopped. Runs only in the {@code samplerBench} run ({@code ./gradlew :create:runSamplerBench
 * -PskipJni}), which enables the {@value #NS} namespace alone and sets {@code mic_rtsim.samplerbench}: the
 * ordinary GameTest server enables {@code mic_rtsim_create}, whose filter drops this function (its structure is
 * {@value #NS}{@code :stage}), and the property keeps a dev client's {@code /test runall} from starting it.
 */
@GameTestHolder(SamplerBenchGameTests.NS)
public final class SamplerBenchGameTests {
    private SamplerBenchGameTests() {}

    public static final String NS = "mic_rtsim_samplerbench";

    /** Ticks the bridge may take to come up before the bench times out: its windows start at the placement. */
    static final int PLACE_WITHIN = 12_000;

    @GameTestGenerator
    public static Collection<TestFunction> samplerBench() {
        if (!SamplerBench.ENABLED)
            return List.of();
        boolean forever = SamplerBench.WINDOWS == 0;
        int timeout = forever ? Integer.MAX_VALUE
                : (int) Math.min(Integer.MAX_VALUE, (long) SamplerBench.WINDOWS * SamplerBench.WINDOW + PLACE_WITHIN);
        return List.of(new TestFunction("samplerbench", "samplerbench.run", NS + ":stage", Rotation.NONE, timeout, 0L, true,
                false, 1, 1, false, h -> {
                    if (!forever)
                        h.succeedWhen(() -> {
                            if (!SamplerBench.done())
                                throw new GameTestAssertException("samplerbench: " + SamplerBench.progress());
                        });
                }));
    }
}
