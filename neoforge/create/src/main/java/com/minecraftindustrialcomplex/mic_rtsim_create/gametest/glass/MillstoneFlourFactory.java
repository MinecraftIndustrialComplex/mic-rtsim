package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.Items;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 4: millstone flour. A chest of wheat drains through a chute into a millstone; a chute under the
 * millstone empties it into a chest. A creative motor turns a cogwheel beside the millstone at 64 RPM
 * (the millstone is a small cog, so it turns at 64 the other way).
 *
 * <pre>
 *   y5  chest (wheat)
 *   y4  chute
 *   y3  millstone   cogwheel (Y)
 *   y2  chute       motor (up, 64 RPM)
 *   y1  chest (out)
 * </pre>
 *
 * <p><b>Rates</b> ({@code MillstoneBlockEntity.tick}, {@code getProcessingSpeed}, {@code process}):
 * <ul>
 *   <li>speed s = clamp((int) |64 / 16|, 1, 512) = 4 per tick; the wheat recipe's processing_time is
 *       150 ({@code milling/wheat.json}). One tick sets the timer to 150 (the input is never empty: the
 *       chute keeps 64 in the input slot), then it counts down by 4 and processes on the tick it reaches
 *       0 or below: ceil(150 / 4) = 38 ticks. Cycle 39 ticks, one wheat each: 1200 / 39 = 30.769 per
 *       minute, exact.</li>
 *   <li>Outputs ({@code ProcessingOutput.rollOutput}, binomial per item): 1 flour, then 2 flour at chance
 *       0.25 each, then 1 seed at 0.25. Flour 1.5 per wheat (variance 2 × 0.25 × 0.75 = 0.375), seeds 0.25
 *       (variance 0.1875).</li>
 *   <li>Movers are far above that: a plain chute takes up to 16 per pull and moves 0.2 blocks per tick
 *       ({@code ChuteBlockEntity.getExtractionAmount}, {@code getItemMotion}), about 16 per 6 ticks.
 *       Nothing on the way is bounded.</li>
 * </ul>
 */
public final class MillstoneFlourFactory implements CorpusFactory {
    static final int RPM = 64;
    static final int CYCLE = 39;
    static final double WHEAT_PER_MIN = TICKS_PER_MINUTE / (double) CYCLE;

    @Override
    public String name() {
        return "millstone_flour";
    }

    @Override
    public int warmupTicks() {
        return 200;
    }

    @Override
    public Built build(GameTestHelper h) {
        BlockPos out = at(h, 8, 1, 8), lowChute = at(h, 8, 2, 8), mill = at(h, 8, 3, 8), highChute = at(h, 8, 4, 8), in = at(h, 8, 5, 8);
        chest(h, out);
        set(h, lowChute, AllBlocks.CHUTE.getDefaultState());
        set(h, mill, AllBlocks.MILLSTONE.getDefaultState());
        set(h, highChute, AllBlocks.CHUTE.getDefaultState());
        chest(h, in);
        fill(h, in, Items.WHEAT, 640);
        cog(h, at(h, 9, 3, 8), Direction.Axis.Y);
        motor(h, at(h, 9, 2, 8), Direction.UP, RPM);

        double w = WHEAT_PER_MIN;
        return new Built(mill, List.of(in), List.of(out), List.of(
                Expect.exact("minecraft:wheat", -w,
                        "MillstoneBlockEntity: s = 64/16 = 4, timer 150, ceil(150/4) + 1 = 39 ticks per wheat"),
                Expect.expected("create:wheat_flour", 1.5 * w, 0.375 * w,
                        "milling/wheat: 1 + Binomial(2, 0.25) flour per wheat"),
                Expect.expected("minecraft:wheat_seeds", 0.25 * w, 0.1875 * w,
                        "milling/wheat: Bernoulli(0.25) seeds per wheat")),
                "millstone at 64 RPM, chute fed; 640 wheat lasts 640 x 39 = 24,960 ticks");
    }
}
