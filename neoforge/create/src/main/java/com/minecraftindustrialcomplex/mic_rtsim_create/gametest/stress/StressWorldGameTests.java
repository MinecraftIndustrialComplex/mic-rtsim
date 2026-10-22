package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.stress;

import net.minecraft.gametest.framework.GameTestGenerator;
import net.minecraft.gametest.framework.TestFunction;
import net.minecraft.world.level.block.Rotation;
import net.neoforged.neoforge.gametest.GameTestHolder;

import java.util.Collection;
import java.util.List;

/**
 * The synthetic stress world's generator (DECISIONS.md ST1 and ST3; docs/stress-worlds-results.md
 * section 10, its SG choices in 10.7). One GameTest that builds the world with Create's own APIs and then
 * lets the GameTest server save it: {@link StressWorld} does the work.
 *
 * <p>It runs only in the {@code stressGen} run ({@code ./gradlew :create:runStressGen -PskipJni}, or
 * {@code tools/stress-worlds/synth.sh}), which enables the {@value #NS} namespace alone and sets
 * {@code mic_rtsim.stressgen}. The ordinary GameTest server enables {@code mic_rtsim_create}, whose
 * filter drops this generator's function (its structure is {@value #NS}{@code :stage}); the system
 * property keeps a dev client's {@code /test runall} from starting it too. GameTests never register in
 * production, so nothing here is reachable by a player (D3).
 */
@GameTestHolder(StressWorldGameTests.NS)
public final class StressWorldGameTests {
    private StressWorldGameTests() {}

    public static final String NS = "mic_rtsim_stressgen";

    @GameTestGenerator
    public static Collection<TestFunction> stressWorld() {
        if (!Boolean.getBoolean("mic_rtsim.stressgen"))
            return List.of();
        StressWorld.Params p = StressWorld.Params.fromEnv();
        return List.of(new TestFunction("stressgen", "stressgen.build", NS + ":stage", Rotation.NONE,
                StressWorld.MAX_TICKS, 0L, true, false, 1, 1, false, h -> new StressWorld(h, p).start()));
    }
}
