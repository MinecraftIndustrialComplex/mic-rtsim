package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim_create.bench.SamplerBench;
import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.world.level.block.Rotation;
import net.neoforged.neoforge.gametest.GameTestHolder;

import java.util.Collection;
import java.util.List;

/**
 * Keeps a GameTest server running for {@link SamplerBench}'s windows, then ends it (a GameTest server stops
 * when its tests are done). Runs only in the {@code samplerBench} run ({@code ./gradlew :create:runSamplerBench
 * -PskipJni}), which enables the {@value #NS} namespace alone and sets {@code mic_rtsim.samplerbench}: the
 * ordinary GameTest server enables {@code mic_rtsim_create}, whose filter drops this function (its structure is
 * {@value #NS}{@code :stage}), and the property keeps a dev client's {@code /test runall} from starting it.
 */
@GameTestHolder(SamplerBenchGameTests.NS)
public final class SamplerBenchGameTests {
    private SamplerBenchGameTests() {}

    public static final String NS = "mic_rtsim_samplerbench";

    @GameTestGenerator
    public static Collection<TestFunction> samplerBench() {
        if (!SamplerBench.ENABLED)
            return List.of();
        int run = Math.max(1, SamplerBench.WINDOWS) * SamplerBench.WINDOW + 40;
        return List.of(new TestFunction("samplerbench", "samplerbench.run", NS + ":stage", Rotation.NONE, run + 400, 0L, true,
                false, 1, 1, false, h -> h.runAfterDelay(run, h::succeed)));
    }
}
