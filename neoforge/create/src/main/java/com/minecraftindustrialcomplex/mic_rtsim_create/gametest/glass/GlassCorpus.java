package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;

import java.util.ArrayList;
import java.util.List;

/**
 * The G1 corpus: GLASSBOX.md §11.3's twelve factories plus C32's sheep farm, in the exit test's order.
 * {@link #prepare} builds one and makes it a district; {@link GlassCorpusGameTests} runs each for a
 * window and checks Create against the hand-worked rates; the G1 exit test (integration) adds the glass
 * box's prediction on top of the same builds.
 */
public final class GlassCorpus {
    private GlassCorpus() {}

    public static final List<CorpusFactory> ALL = List.of(
            new MillstoneFlourFactory()
    );

    public static CorpusFactory byName(String name) {
        for (CorpusFactory f : ALL)
            if (f.name().equals(name))
                return f;
        throw new IllegalArgumentException("no corpus factory " + name);
    }

    /**
     * Builds the factory in a clean yard and marks every store as placed by a player (the I2 seed), so the
     * factory becomes one district on the next detection. Call at the test's first tick.
     */
    public static CorpusFactory.Built prepare(GameTestHelper helper, CorpusFactory f) {
        IndustryKit.fastConfig();
        IndustryKit.forget(helper);
        CorpusFactory.Built b = f.build(helper);
        for (BlockPos p : b.stores())
            IndustryKit.playerPlaced(helper, p);
        IndustryKit.playerPlaced(helper, b.anchor());
        return b;
    }

    /**
     * Fails unless the anchor and every store are in one district (each factory is exactly one), and
     * returns it.
     */
    public static District district(GameTestHelper helper, CorpusFactory.Built b) {
        District d = IndustryKit.districtAt(helper, b.anchor());
        if (d == null)
            throw new GameTestAssertException("no district at the anchor " + b.anchor().toShortString());
        List<String> stray = new ArrayList<>();
        for (BlockPos p : b.stores()) {
            District o = IndustryKit.districtAt(helper, p);
            if (o != d)
                stray.add(p.toShortString() + " in " + o);
        }
        if (!stray.isEmpty())
            throw new GameTestAssertException("stores outside the anchor's district " + d + ": " + stray);
        return d;
    }
}
