package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import net.minecraft.gametest.framework.GameTestHelper;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 6 over-fed (GB72): {@link PrecisionMechanismFactory}'s loop with the feeder belt at 64 RPM instead
 * of 8, so sheets come onto the loop faster than the deployers finish them. The loop jams for good and
 * nothing comes out: every good of the factory nets 0 in the window.
 *
 * <ul>
 *   <li>Feed f: the andesite funnel moves one sheet, then waits 8 ticks ({@code defaultExtractionTimer}),
 *       then until the segment is free; at 64 RPM the feeder moves 64 / 480 blocks per tick, so the sheet
 *       is past 1.0625 blocks after 8 ticks (8 x 64/480 = 1.067) and the funnel's wait is the limit: about a
 *       sheet per 9 ticks, 133 a minute. The feeder's end inserts each into the loop's belt C from the side,
 *       and {@code BeltInventory.canInsertAtFromSide} only waits for earlier insertions from that side,
 *       never for the items going round: the loop takes every sheet the feeder brings while C has a gap.</li>
 *   <li>The machines' rate mu: each deployer applies 5 parts per product at about 6 ticks each, 30 ticks
 *       per product, 40 a minute (corpus 6's working, not measured).</li>
 *   <li>f &gt; mu, so the loop's population grows by at least f - mu per tick until its 28 belt slots are full.
 *       Then nothing moves: a stuck item is never processed ({@code
 *       BeltInventory.handleBeltProcessingAndCheckIfRemoved} returns early when the item has no room to move),
 *       the exit funnel only takes an item as it crosses it, and the feeder backs up to the funnel. The
 *       jam is absorbing, so the end state does not depend on tick order.</li>
 * </ul>
 *
 * <p>Measured once with corpus 6's build and the feeder at 64 RPM (before this factory existed): by the end
 * of the 1,000-tick warm-up all 28 loop segments and the feeder held an item, and nothing left the loop in a
 * 7,200-tick window. The glass box predicts the stall (L = 0 for every good of the loop, GLASSBOX.md §3.4);
 * the expectations here are Create's side of it: every good 0.
 */
public final class PrecisionOverfedFactory implements CorpusFactory {
    static final int FEED_RPM = 64;
    static final int WINDOW = 2400;

    @Override
    public String name() {
        return "precision_overfed";
    }

    @Override
    public int warmupTicks() {
        return 1000;
    }

    @Override
    public Built build(GameTestHelper h) {
        PrecisionMechanismFactory.Layout l = PrecisionMechanismFactory.layout(h, FEED_RPM);
        String jam = "the loop jams (feed 1 sheet per 9 ticks > 1 product per 30 ticks): nothing moves";
        List<Expect> expect = new ArrayList<>();
        for (String good : List.of("create:golden_sheet", "create:cogwheel", "create:large_cogwheel",
                "minecraft:iron_nugget", "create:precision_mechanism", "create:andesite_alloy"))
            expect.add(Expect.exact(good, 0, jam));
        return new Built(l.anchor(), l.inputs(), l.outputs(), List.copyOf(expect), List.of(), WINDOW,
                "corpus 6 with the feeder at " + FEED_RPM + " RPM: over-fed, expected to jam within the "
                        + warmupTicks() + "-tick warm-up");
    }

    @Override
    public void perturb(GameTestHelper h, Built built, int windowTick) {
        if (windowTick == 0)
            PrecisionMechanismFactory.checkBelts(h);
    }
}
