package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import net.minecraft.gametest.framework.GameTestGenerator;
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
import java.util.function.Supplier;
import java.util.stream.Collectors;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassReplayGameTests.IO_TICKS;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassReplayGameTests.SETTLE;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassReplayGameTests.WINDOW;

/**
 * SEC-1's section fixtures (SECTIONS.md §9.2, slice S2). Each is a {@link CorpusFactory} and a
 * {@link GlassReplayGameTests.Scenario}, run through the replay harness: the corpus cycle (loaded for a window,
 * away for a replay, back for a reconcile that matches) with the fixture's own checks on the report's judging
 * units. Their namespace is {@code mic_rtsim_create_sec1fx}, apart from the module's, so
 * {@code ./gametest.sh create -PgametestNamespaces=mic_rtsim_create_sec1fx} runs only these; the default
 * (the module's namespace and this one) runs them with the rest.
 *
 * <p>{@code MIC_SEC1FX_ONLY=belt_into_press,hopper_vault} runs only the named fixtures.
 */
@GameTestHolder(SectionsFxGameTests.NAMESPACE)
@PrefixGameTestTemplate(false)
public final class SectionsFxGameTests {
    static final String NAMESPACE = "mic_rtsim_create_sec1fx";

    /** The far copies' slots start here, clear of the corpus' (0 to 3 x its size) and the station fixture's. */
    static final int FIRST_SLOT = 100;

    /** One fixture: its build, its checks, and whether it runs as the partial variant. */
    record Fx(String name, boolean partial, Supplier<CorpusFactory> factory, Supplier<GlassReplayGameTests.Scenario> scenario) {}

    static final List<Fx> ALL = List.of(
            new Fx("belt_into_press", false, BeltIntoPressFactory::new, BeltIntoPressFactory.Check::new),
            new Fx("half_loaded", true, HalfLoadedFactory::new, HalfLoadedFactory.Check::new),
            new Fx("hopper_vault", false, HopperAtASharedVaultFactory::new, HopperAtASharedVaultFactory.Check::new),
            new Fx("frozen_neighbour", false, FrozenNeighbourFactory::new, FrozenNeighbourFactory.Check::new),
            new Fx("shared_boiler", false, SharedBoilerTwoLinesFactory::new, SharedBoilerTwoLinesFactory.Check::new),
            new Fx("vault_mixers", false, VaultBetweenMixersFactory::new, VaultBetweenMixersFactory.Check::new),
            new Fx("residue_vault", false, ResidueAtASharedVaultFactory::new, ResidueAtASharedVaultFactory.Check::new),
            new Fx("basin_vault", false, BasinIntoASharedVaultFactory::new, BasinIntoASharedVaultFactory.Check::new));

    /** The fixtures {@code MIC_SEC1FX_ONLY} names (comma separated), or none when it is unset: all. */
    private static Set<String> only() {
        String only = System.getenv("MIC_SEC1FX_ONLY");
        return only == null || only.isBlank() ? Set.of()
                : Arrays.stream(only.split(",")).map(String::trim).filter(s -> !s.isEmpty()).collect(Collectors.toSet());
    }

    /** Whether the named fixture is selected (no filter set, or it is named). */
    static boolean selected(String name) {
        Set<String> filter = only();
        return filter.isEmpty() || filter.contains(name);
    }

    @GameTestGenerator
    public static Collection<TestFunction> sections() {
        Set<String> filter = only();
        List<TestFunction> out = new ArrayList<>();
        for (int i = 0; i < ALL.size(); i++) {
            Fx fx = ALL.get(i);
            if (!filter.isEmpty() && !filter.contains(fx.name()))
                continue;
            int slot = FIRST_SLOT + i;
            CorpusFactory probe = fx.factory().get();
            GlassReplayGameTests.Scenario sc = fx.scenario().get();
            int maxTicks = probe.warmupTicks() + 2 * (WINDOW + GlassExitGameTests.SLACK) + sc.extraTicks() + sc.span() + 3 * IO_TICKS + SETTLE + 2000;
            out.add(new TestFunction("sec1fx_" + fx.name(), "sec1fx." + fx.name().toLowerCase(Locale.ROOT),
                    NAMESPACE + ":" + IndustryKit.YARD, Rotation.NONE, maxTicks, 0L, true, false, 1, 1, true,
                    helper -> GlassReplayGameTests.start(helper, fx.factory().get(), slot, fx.partial(), false, fx.scenario().get())));
        }
        return out;
    }
}
