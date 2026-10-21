package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllItems;
import com.simibubi.create.content.kinetics.base.DirectionalAxisKineticBlock;
import com.simibubi.create.content.kinetics.saw.SawBlock;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 5: saw shafts. A chest of andesite alloy feeds an upward mechanical saw through a smart chute
 * above it (filter: alloy, count 1, so it pulls one at a time); the saw cuts each alloy into 6 shafts and pushes them sideways into a chute, which drops them into
 * a chest. The saw's filter is set to shafts. A creative motor turns the saw at 96 RPM.
 *
 * <pre>
 *   y5          chest (alloy)
 *   y4          smart chute (alloy x1)
 *   y3  chute   saw (up, axis X)   chute     motor (east) at x-1
 *   y2  chest                      chest
 * </pre>
 * The saw pushes its outputs along z, one way or the other by the sign of its speed
 * ({@code SawBlockEntity.getItemMovementVec}), so both sides have a chute and a chest; both are outputs.
 *
 * <p><b>Rates</b> ({@code SawBlockEntity.tick}, {@code start}, {@code applyRecipe}; {@code ProcessingInventory}):
 * <ul>
 *   <li>The filter matters: andesite alloy also matches stonecutting recipes (bars, ladders, scaffolding,
 *       table cloth: {@code allowStonecuttingOnSaw} defaults true) and the saw rotates its
 *       {@code recipeIndex} at every start, so without a filter only one alloy in five or so would make
 *       shafts. With the filter on {@code create:shaft} only {@code cutting/andesite_alloy} (200 ticks,
 *       6 shafts) is left.</li>
 *   <li>One item at a time ({@code bulkCutting} false: slot limit 1). Inserting it calls {@code start}:
 *       remaining = 200 x max(1, 1/5) = 200. Each saw tick takes p = clamp(96/24, 1, 128) = 4. The recipe
 *       applies on the first tick that leaves less than 5: 200 - 4k &lt; 5 at k = 49. Remaining is set
 *       to 20 and counts down: 0 after 5 more ticks, when the 6 shafts go to the chute beside it
 *       ({@code DirectBeltInputBehaviour}). The inventory is then empty, so the waiting input chute's
 *       next step inserts the next alloy.</li>
 *   <li>The input chute pulls its next alloy the tick after it hands one down and parks it in 3 ticks,
 *       long before the saw is free. It parks the alloy at 0.5 while the saw is busy ({@code ChuteBlockEntity.tick});
 *       at 0.2 blocks per tick ({@code getItemMotion}: gravity only, -4 / 20) it inserts on the third
 *       step after the saw empties (0.3, 0.1, -0.1), the first of which falls in the saw's export tick
 *       whichever ticks first. Cycle 49 + 5 + 2 = 56 ticks per alloy: 1200 / 56 = 21.429 alloys and
 *       128.571 shafts per minute, exact.</li>
 * </ul>
 */
public final class SawShaftsFactory implements CorpusFactory {
    static final int RPM = 96;
    static final int CYCLE = 56;

    @Override
    public String name() {
        return "saw_shafts";
    }

    @Override
    public int warmupTicks() {
        return 200;
    }

    @Override
    public Built build(GameTestHelper h) {
        BlockPos saw = at(h, 8, 3, 8), in = at(h, 8, 5, 8);
        BlockPos outA = at(h, 8, 2, 7), outB = at(h, 8, 2, 9);
        set(h, saw, AllBlocks.MECHANICAL_SAW.getDefaultState().setValue(SawBlock.FACING, Direction.UP)
                .setValue(DirectionalAxisKineticBlock.AXIS_ALONG_FIRST_COORDINATE, true));
        BlockEntityBehaviour.get(h.getLevel(), saw, FilteringBehaviour.TYPE).setFilter(new ItemStack(AllBlocks.SHAFT.get()));
        // A smart chute pulling one alloy at a time keeps the input chest's count a fine staircase (a plain
        // chute pulls 16, which a 2,400-tick window resolves to about 5%).
        BlockPos feed = at(h, 8, 4, 8);
        set(h, feed, AllBlocks.SMART_CHUTE.getDefaultState());
        FilteringBehaviour feedFilter = BlockEntityBehaviour.get(h.getLevel(), feed, FilteringBehaviour.TYPE);
        feedFilter.setFilter(new ItemStack(AllItems.ANDESITE_ALLOY.get()));
        feedFilter.count = 1;
        chest(h, in);
        fill(h, in, AllItems.ANDESITE_ALLOY.get(), 640);
        for (BlockPos out : List.of(outA, outB)) {
            chest(h, out);
            set(h, out.above(), AllBlocks.CHUTE.getDefaultState());
        }
        motor(h, at(h, 7, 3, 8), Direction.EAST, RPM);

        double alloy = TICKS_PER_MINUTE / (double) CYCLE;
        return new Built(saw, List.of(in), List.of(outA, outB), List.of(
                Expect.exact("create:andesite_alloy", -alloy,
                        "SawBlockEntity: p = 96/24 = 4; 200 - 4k < 5 at k 49; 20/4 = 5; chute hand-down 2: 56 ticks"),
                Expect.exact("create:shaft", 6 * alloy, "cutting/andesite_alloy: 6 shafts per alloy, filter on shaft")),
                "saw at 96 RPM, filter shaft, smart chute fed one at a time; 640 alloy lasts 640 x 56 = 35,840 ticks");
    }
}
