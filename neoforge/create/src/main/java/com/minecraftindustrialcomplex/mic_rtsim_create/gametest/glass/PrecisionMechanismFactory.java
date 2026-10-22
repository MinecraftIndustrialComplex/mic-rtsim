package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllDataComponents;
import com.simibubi.create.AllItems;
import com.simibubi.create.content.kinetics.base.DirectionalAxisKineticBlock;
import com.simibubi.create.content.kinetics.base.DirectionalKineticBlock;
import com.simibubi.create.content.logistics.funnel.BeltFunnelBlock;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.component.ItemContainerContents;
import net.minecraft.world.level.block.state.BlockState;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 6: precision mechanisms on a deployer loop (Create's {@code sequenced_assembly/precision_mechanism}:
 * golden sheet in, three deploying steps (cogwheel, large cogwheel, iron nugget), {@code loops} 5, a
 * weighted pool of 150 with the mechanism at 120). Four belts at 256 RPM make a closed loop; three
 * deployers at 256 RPM hang over the first belt, each fed by a chute from a chest above it; an andesite
 * belt funnel feeds golden sheets onto a slow feeder belt (8 RPM) whose end hands them into the loop; a
 * brass belt funnel with a deny-list filter (the transitional item) takes every finished item off the
 * loop into a chest. Each item laps the loop five times, passing the same three deployers each lap.
 *
 * <pre>
 *   top view, y 1 (belts), x 2..14, z 5..13; arrows are the item flow
 *
 *   z5        M                                        (M: belt motors, -256 or 8 RPM)
 *   z6        A&gt; A  A  D1 A  D2 A  D3 A  A  B   M     D1..D3 at y 3 over A, chute y 4, chest y 5
 *   z7        D^                               Bv
 *   z8        D^                               Bv [exit funnel y 2, chest east of it]
 *   z9        D^                               Bv
 *   z10    M  D^ C&lt; C  C  C  C  C  C  C  C  C
 *   z11                      F^             M
 *   z12                      F^
 *   z13              [sheets] F^ M          (andesite belt funnel over F at y 2, chest west of it)
 *         x2 x3 x4 ...       x8             x13 x14
 * </pre>
 *
 * <p><b>The loop is fed, not saturated.</b> Belt side insertion checks only earlier insertions from the
 * same side ({@code BeltInventory.canInsertAtFromSide}, {@code isBlocking}), never items already passing,
 * so a feed onto the loop adds sheets whatever the loop holds. Fed faster than the deployers finish them,
 * the loop's population grows until every belt slot is full and it jams for good. Measured once with this
 * build and the feeder at 64 RPM (a sheet per 9 ticks, the funnel's cooldown): by the end of the 1,000-tick
 * warm-up all 28 loop segments and the feeder held an item, and nothing left the loop in the 7,200-tick
 * window ({@link PrecisionOverfedFactory} builds that case). So the feed here is slower than the deployers,
 * and it sets every rate:
 * <ul>
 *   <li>Feed ({@code FunnelBlockEntity.activateExtractingBeltFunnel}, {@code getAmountToExtract};
 *       {@code BeltBlockEntity.tryInsertingFromSide}, {@code isOccupied}): an andesite funnel moves one
 *       item per transfer, then waits 8 ticks ({@code defaultExtractionTimer}), then until the belt
 *       segment is free. The sheet goes in at 0.4375 along the segment and blocks the next insertion
 *       from the same side while at or behind 1.5: it must move more than 1.0625 blocks. The feeder belt
 *       turns at 8 RPM: 8 / 480 blocks per tick, so 64 ticks (63 x 8/480 = 1.05, 64 x 8/480 = 1.067), in
 *       either tick order of funnel and belt. One sheet per 64 ticks: 18.75 sheets per minute, exact.
 *       The feeder's spacing (1.067 blocks) is above the belt's spacing of 1, so sheets never queue on it,
 *       and its end inserts each into the loop at once (the loop's side check only sees the previous
 *       sheet, long gone at 0.533 blocks per tick).</li>
 *   <li>Steps ({@code SequencedAssemblyRecipe.advance}, {@code getNextRecipe}): step s (0 to 14) is
 *       sequence[s % 3]; after step 14, (14 + 1) / 3 = 5 &gt;= loops, so the item rolls the pool. Each item
 *       takes 5 cogwheels, 5 large cogwheels and 5 iron nuggets (one per application: {@code
 *       BeltDeployerCallbacks.activate} shrinks the held stack by 1).</li>
 *   <li>Deployer capacity ({@code DeployerBlockEntity.tick}, {@code getTimerSpeed}; {@code
 *       BeltDeployerCallbacks.onItemReceived}, {@code whenItemHeld}): u = clamp(256 x 2, 8, 512) = 512;
 *       the belt starts the deployer when an item reaches the segment centre, expanding takes ceil(1000/512)
 *       + 1 = 3 ticks, the belt applies the recipe when it sees RETRACTING with timer 1000, retracting takes
 *       3 more, and the belt restarts it as soon as it is WAITING (the 500 wait only applies off a belt):
 *       about 6 ticks per application (not measured: this factory never queues long enough to show it).
 *       Each deployer applies 5 per sheet, 5 x 6 = 30 ticks per 64: 47% busy. A lap is 28 blocks at 0.533
 *       per tick plus the three holds, about 65 ticks, so the loop holds about 5 items of its 28 slots
 *       (measured: 4 or 5 on the loop and 2 on the feeder, in a pattern that repeats within 4,800 ticks).</li>
 *   <li>Pool ({@code SequencedAssemblyRecipe.rollResult}: one weighted draw per finished item, weights
 *       sum 150): precision mechanism 120, golden sheet 8, andesite alloy 8, cogwheel 5, gold nugget 3,
 *       shaft 2, crushed raw gold 2, iron ingot 1, clock 1. Per sheet, each output is Bernoulli(w / 150):
 *       mean r w / 150 and variance r (w / 150)(1 - w / 150) per minute at r = 18.75 sheets per
 *       minute. The scrap golden sheets and cogwheels land in the output chest, so the net of each is
 *       its use plus its scrap.</li>
 * </ul>
 *
 * <p>Rare scraps: gold nugget, shaft, crushed raw gold, iron ingot and clock expect 0.75 to 2.25 items in a
 * 6-minute window, so a window often holds none, and the smoke runner fails a produced good measured at 0.
 * They are listed BOUNDED in [0, mean + 4 sigma] with a most likely rate of 0 for that reason only; their
 * expectation is in the derivation, and with a runner that allows a zero they belong as
 * {@code Expect.expected}.
 */
public final class PrecisionMechanismFactory implements CorpusFactory {
    static final int LOOP_RPM = 256;
    static final int DEPLOYER_RPM = 256;
    static final int FEED_RPM = 8;
    /** Ticks per sheet from the feeder: the first k with k x 8/480 above 1.0625. */
    static final int FEED_TICKS = 64;
    static final double SHEETS_PER_MIN = TICKS_PER_MINUTE / (double) FEED_TICKS;
    static final int LOOPS = 5;
    static final double POOL = 150;
    static final int WINDOW = 7200;

    static final int SHEETS = 256, PARTS = 1024;

    @Override
    public String name() {
        return "precision_mechanisms";
    }

    @Override
    public int warmupTicks() {
        return 1000;
    }

    /** Corpus 6's blocks: the anchor, the input chests (sheets first, then the three parts) and the output chest. */
    record Layout(BlockPos anchor, List<BlockPos> inputs, List<BlockPos> outputs) {}

    @Override
    public Built build(GameTestHelper h) {
        Layout l = layout(h, FEED_RPM);
        double r = SHEETS_PER_MIN;
        double minutes = WINDOW / (double) TICKS_PER_MINUTE;
        String per = "per sheet (1 per " + FEED_TICKS + " ticks, andesite funnel onto an 8 RPM belt)";
        List<Expect> expect = List.of(
                chance("create:golden_sheet", -r, 8, r, "-1 sheet " + per + " plus scrap 8/150 into the output chest"),
                chance("create:cogwheel", -LOOPS * r, 5, r, "-5 cogwheels " + per + " (5 loops x 1) plus scrap 5/150"),
                Expect.exact("create:large_cogwheel", -LOOPS * r, "-5 large cogwheels " + per + " (5 loops x 1)"),
                Expect.exact("minecraft:iron_nugget", -LOOPS * r, "-5 iron nuggets " + per + " (5 loops x 1)"),
                chance("create:precision_mechanism", 0, 120, r, "pool 120/150 " + per),
                chance("create:andesite_alloy", 0, 8, r, "pool 8/150 " + per),
                rare("minecraft:gold_nugget", 3, r, minutes),
                rare("create:shaft", 2, r, minutes),
                rare("create:crushed_raw_gold", 2, r, minutes),
                rare("minecraft:iron_ingot", 1, r, minutes),
                rare("minecraft:clock", 1, r, minutes));
        return new Built(l.anchor(), l.inputs(), l.outputs(), expect, List.of(), WINDOW,
                "4-belt loop at 256 RPM, deployers at 256 RPM, feed 1 sheet per " + FEED_TICKS + " ticks; "
                        + SHEETS + " sheets last " + SHEETS * FEED_TICKS + " ticks, " + PARTS + " of each part last "
                        + PARTS / LOOPS * FEED_TICKS + " ticks");
    }

    /** Builds corpus 6 with the feeder belt at {@code feedRpm} (8 here, 64 in {@link PrecisionOverfedFactory}). */
    static Layout layout(GameTestHelper h, int feedRpm) {
        // The loop: A east along z 6, B south along x 13, C west along z 10, D north along x 3.
        belt(h, at(h, 3, 1, 6), at(h, 12, 1, 6));
        belt(h, at(h, 13, 1, 6), at(h, 13, 1, 9));
        belt(h, at(h, 13, 1, 10), at(h, 4, 1, 10));
        belt(h, at(h, 3, 1, 10), at(h, 3, 1, 7));
        // The feeder, north along x 8 into C's segment at x 8.
        belt(h, at(h, 8, 1, 13), at(h, 8, 1, 11));

        // One motor per belt, on its start pulley. Speed signs: an X-axis belt moves east when its speed is
        // negative, a Z-axis belt south when positive (BeltBlockEntity.getMovementFacing); a motor gives
        // +rpm facing a positive direction, -rpm facing a negative one (convertToDirection).
        motor(h, at(h, 3, 1, 5), Direction.SOUTH, -LOOP_RPM);   // A: -256, east
        motor(h, at(h, 14, 1, 6), Direction.WEST, -LOOP_RPM);   // B: +256, south
        motor(h, at(h, 13, 1, 11), Direction.NORTH, -LOOP_RPM); // C: +256, west
        motor(h, at(h, 2, 1, 10), Direction.EAST, -LOOP_RPM);   // D: -256, north
        motor(h, at(h, 9, 1, 13), Direction.WEST, feedRpm);     // F: -8 (or -64), north

        // Deployers over A at x 6, 8, 10, on one X-axis shaft line at y 3.
        BlockState deployer = AllBlocks.DEPLOYER.getDefaultState()
                .setValue(DirectionalKineticBlock.FACING, Direction.DOWN)
                .setValue(DirectionalAxisKineticBlock.AXIS_ALONG_FIRST_COORDINATE, true);
        motor(h, at(h, 4, 3, 6), Direction.EAST, DEPLOYER_RPM);
        shaft(h, at(h, 5, 3, 6), Direction.Axis.X);
        set(h, at(h, 6, 3, 6), deployer);
        shaft(h, at(h, 7, 3, 6), Direction.Axis.X);
        set(h, at(h, 8, 3, 6), deployer);
        shaft(h, at(h, 9, 3, 6), Direction.Axis.X);
        set(h, at(h, 10, 3, 6), deployer);

        String[] parts = {"create:cogwheel", "create:large_cogwheel", "minecraft:iron_nugget"};
        List<BlockPos> inputs = new ArrayList<>();
        for (int i = 0; i < 3; i++) {
            int x = 6 + 2 * i;
            set(h, at(h, x, 4, 6), AllBlocks.CHUTE.getDefaultState());
            BlockPos chest = at(h, x, 5, 6);
            chest(h, chest);
            fill(h, chest, item(parts[i]), PARTS);
            inputs.add(chest);
        }

        // Sheets: an andesite belt funnel over the feeder's first segment, pushing, its chest behind it (west).
        BlockPos sheets = at(h, 7, 2, 13);
        chest(h, sheets);
        fill(h, sheets, AllItems.GOLDEN_SHEET.get(), SHEETS);
        set(h, at(h, 8, 2, 13), AllBlocks.ANDESITE_BELT_FUNNEL.getDefaultState()
                .setValue(BeltFunnelBlock.HORIZONTAL_FACING, Direction.EAST)
                .setValue(BeltFunnelBlock.SHAPE, BeltFunnelBlock.Shape.PUSHING));
        inputs.add(0, sheets);

        // Exit: a brass belt funnel over B at z 8, pulling, deny-list filter of the transitional item, chest east.
        BlockPos out = at(h, 14, 2, 8);
        chest(h, out);
        BlockPos exit = at(h, 13, 2, 8);
        set(h, exit, AllBlocks.BRASS_BELT_FUNNEL.getDefaultState()
                .setValue(BeltFunnelBlock.HORIZONTAL_FACING, Direction.WEST)
                .setValue(BeltFunnelBlock.SHAPE, BeltFunnelBlock.Shape.PULLING));
        FilteringBehaviour filter = BlockEntityBehaviour.get(h.getLevel(), exit, FilteringBehaviour.TYPE);
        if (filter == null || !filter.setFilter(denyList(AllItems.INCOMPLETE_PRECISION_MECHANISM.asStack())))
            throw new GameTestAssertException("could not set the exit funnel's filter");
        return new Layout(at(h, 6, 3, 6), inputs, List.of(out));
    }

    @Override
    public void perturb(GameTestHelper h, Built built, int windowTick) {
        if (windowTick == 0)
            checkBelts(h);
    }

    /** Not a regime change: a check that the loop and the feeder turn the way the layout needs. */
    static void checkBelts(GameTestHelper h) {
        Object[][] belts = {{at(h, 5, 1, 6), Direction.EAST}, {at(h, 13, 1, 8), Direction.SOUTH},
                {at(h, 8, 1, 10), Direction.WEST}, {at(h, 3, 1, 8), Direction.NORTH}, {at(h, 8, 1, 12), Direction.NORTH}};
        for (Object[] b : belts) {
            Direction moving = beltMovement(h, (BlockPos) b[0]);
            if (moving != b[1])
                throw new GameTestAssertException("the belt at " + ((BlockPos) b[0]).toShortString() + " moves " + moving + ", not " + b[1]);
        }
    }

    /**
     * A chance output of the pool at weight {@code w} out of 150, plus {@code base} per minute that is not
     * chance (the good's own use): mean base + r w/150, variance r (w/150)(1 - w/150) per minute.
     */
    static Expect chance(String good, double base, int w, double r, String derivation) {
        double p = w / POOL;
        return Expect.expected(good, base + r * p, r * p * (1 - p), derivation);
    }

    /** See the class comment: a pool output too rare to show up in every window. */
    static Expect rare(String good, int w, double r, double windowMinutes) {
        double p = w / POOL;
        double mean = r * p, var = r * p * (1 - p);
        double hi = mean + 4 * Math.sqrt(1.2 * var / windowMinutes);
        return new Expect(good, 0, Kind.BOUNDED, 0, hi, var, String.format(java.util.Locale.ROOT,
                "pool %d/150 per sheet: expected %.3f/min (variance %.3f/min), bounded [0, mean + 4 sigma] because a window often holds none",
                w, mean, var));
    }

    /** A list filter that lets through everything except {@code denied} (Create's deny-list mode). */
    static ItemStack denyList(ItemStack denied) {
        ItemStack f = AllItems.FILTER.asStack();
        f.set(AllDataComponents.FILTER_ITEMS, ItemContainerContents.fromItems(List.of(denied)));
        f.set(AllDataComponents.FILTER_ITEMS_BLACKLIST, true);
        return f;
    }
}
