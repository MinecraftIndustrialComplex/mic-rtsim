package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.base.HorizontalKineticBlock;
import com.simibubi.create.content.kinetics.crafter.ConnectedInputHandler;
import com.simibubi.create.content.kinetics.crafter.MechanicalCrafterBlock;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import net.createmod.catnip.math.Pointing;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.state.BlockState;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 7: a 3 x 3 mechanical crafter grid making iron blocks from iron ingots (vanilla
 * {@code iron_block}: nine ingots, shaped; Create runs regular crafting recipes in crafters while
 * {@code CRecipes.allowRegularCraftingInCrafter} is true, its default). Nine crafters face north in one
 * vertical plane, their inputs all connected (what a wrench does: {@code ConnectedInputHandler.toggleConnection}
 * along a spanning tree), so one smart chute fills all nine slots. The arrows run along each row to the
 * east column, then down it; the bottom-east crafter is the final one and points down into the output
 * chest. A cogwheel beside the east column on a creative motor turns the grid at 64 RPM (crafters mesh
 * like small cogs; they take no shaft).
 *
 * <pre>
 *   front view (looking south at the crafters' faces), x 10..13, y 1..6, crafters at z 10
 *
 *   y6  chest (ingots)
 *   y5  smart chute (filter: iron ingot, exactly 9)
 *   y4  [>] [>] [v]
 *   y3  [>] [>] [v]  cog (Z, motor behind it at z 11, 64 RPM)
 *   y2  [>] [>] [v]
 *   y1              chest (blocks)
 * </pre>
 *
 * <p><b>Rates</b> ({@code MechanicalCrafterBlockEntity.tick}, {@code getCountDownSpeed}, {@code begin},
 * {@code continueIfAllPrecedingFinished}, {@code tryInsert}; {@code ChuteBlockEntity.tick},
 * {@code handleDownwardOutput}; {@code SmartChuteBlockEntity.getExtractionAmount}):
 * <ul>
 *   <li>c = clamp((int) |64|, 4, 250) = 64. A crafter's slot holds one item ({@code Inventory}: one slot
 *       of stack size 1) and takes items only while IDLE. When the ninth slot fills, every crafter of the
 *       arrow chain begins: the three with no crafter pointing at them (the west column) go ASSEMBLING
 *       with countDown 1, the rest ACCEPTING.</li>
 *   <li>ASSEMBLING with a target: one tick (1 - 64 &lt; 0) sets EXPORTING with countDown 1000, which runs
 *       out after floor(1000 / 64) + 1 = 16 ticks (1000 - 16 x 64 &lt; 0); the export merges the grid into
 *       the target, which goes ASSEMBLING once every crafter pointing at it is WAITING. Each target is
 *       placed after the crafters that feed it, so it ticks later in the same tick and starts that tick:
 *       a hand-off is 16 ticks. The longest chain (west column top to the final crafter) has d = 4
 *       hand-offs: 64 ticks.</li>
 *   <li>The final crafter's ASSEMBLING tick applies the recipe and sets CRAFTING with countDown 2000:
 *       floor(2000 / 64) + 1 = 32 ticks, then it inserts the block into the chest below and every crafter
 *       goes IDLE ({@code ejectWholeGrid}, with an empty grid).</li>
 *   <li>Feed: the smart chute pulls exactly 9 ingots (filter count 9, not "up to") as soon as it is empty,
 *       well inside the craft, and waits at item position 0.5 while the crafters refuse. It is placed
 *       first, so it ticks before the crafters: the tick after they go IDLE it moves to 0.3 (the
 *       simulated insert succeeds), then 0.1, then below 0, when it inserts all nine (one per slot,
 *       {@code ItemHandlerHelper.insertItemStacked}); the ninth starts the chain in that tick. Fill time
 *       3 ticks.</li>
 *   <li>Cycle = 3 + 4 x 16 + 32 = 99 ticks per block: iron blocks +1200/99 = 12.121 per minute, iron
 *       ingots -9 x 1200/99 = -109.091 per minute. Both exact (deterministic; every rule exact).</li>
 * </ul>
 *
 * <p>The cycle depends on block entity tick order (the order of placement here): a target ticking before
 * its source adds a tick per hand-off, and the chute ticking after the final crafter saves one.
 */
public final class CrafterGridFactory implements CorpusFactory {
    static final int RPM = 64;
    /** c = clamp(RPM, 4, 250). */
    static final int C = 64;
    /** floor(1000 / c) + 1. */
    static final int HAND_OFF = 1000 / C + 1;
    /** floor(2000 / c) + 1. */
    static final int CRAFT = 2000 / C + 1;
    /** The longest chain of hand-offs to the final crafter. */
    static final int DEPTH = 4;
    /** The smart chute's hand-down after the grid goes idle: 0.5 to 0.3, 0.1, below 0. */
    static final int FILL = 3;
    static final int CYCLE = FILL + DEPTH * HAND_OFF + CRAFT;
    static final double BLOCKS_PER_MIN = TICKS_PER_MINUTE / (double) CYCLE;
    static final int INGOTS = 1024;

    @Override
    public String name() {
        return "crafter_grid";
    }

    @Override
    public int warmupTicks() {
        return 300;
    }

    @Override
    public Built build(GameTestHelper h) {
        int z = 10;
        BlockPos chutePos = at(h, 10, 5, z), in = at(h, 10, 6, z), out = at(h, 12, 1, z);

        // Placement order is tick order: the chute first, then each crafter after the ones that feed it.
        set(h, chutePos, AllBlocks.SMART_CHUTE.getDefaultState());
        int[][] order = {{10, 2}, {10, 3}, {10, 4}, {11, 2}, {11, 3}, {11, 4}, {12, 4}, {12, 3}, {12, 2}};
        for (int[] xy : order) {
            Direction target = xy[0] < 12 ? Direction.EAST : Direction.DOWN;
            set(h, at(h, xy[0], xy[1], z), crafter(Direction.NORTH, target));
        }
        // Connect the nine inputs along a spanning tree (a toggle on two already-joined crafters splits them).
        for (int y = 2; y <= 4; y++) {
            ConnectedInputHandler.toggleConnection(h.getLevel(), at(h, 10, y, z), at(h, 11, y, z));
            ConnectedInputHandler.toggleConnection(h.getLevel(), at(h, 11, y, z), at(h, 12, y, z));
        }
        ConnectedInputHandler.toggleConnection(h.getLevel(), at(h, 10, 2, z), at(h, 10, 3, z));
        ConnectedInputHandler.toggleConnection(h.getLevel(), at(h, 10, 3, z), at(h, 10, 4, z));
        int slots = items(h, at(h, 10, 4, z)).getSlots();
        if (slots != 9)
            throw new GameTestAssertException("the crafters' connected input has " + slots + " slots, not 9");

        cog(h, at(h, 13, 3, z), Direction.Axis.Z);
        motor(h, at(h, 13, 3, z + 1), Direction.NORTH, RPM);

        chest(h, in);
        fill(h, in, Items.IRON_INGOT, INGOTS);
        chest(h, out);

        FilteringBehaviour filter = BlockEntityBehaviour.get(h.getLevel(), chutePos, FilteringBehaviour.TYPE);
        if (filter == null)
            throw new GameTestAssertException("no filter on the smart chute");
        filter.setFilter(new ItemStack(Items.IRON_INGOT));
        filter.count = 9;
        filter.upTo = false;

        double b = BLOCKS_PER_MIN;
        return new Built(at(h, 12, 2, z), List.of(in), List.of(out), List.of(
                Expect.exact("minecraft:iron_ingot", -9 * b,
                        "crafter grid: 9 ingots per craft, cycle 3 fill + 4 x 16 hand-off + 32 craft = 99 ticks at c 64"),
                Expect.exact("minecraft:iron_block", b,
                        "crafter grid: one iron block per 99-tick cycle (MechanicalCrafterBlockEntity.tick)")),
                List.of(), 3600,
                "3 x 3 crafters at 64 RPM, connected inputs, smart chute of 9; " + INGOTS + " ingots last "
                        + (INGOTS / 9) * CYCLE + " ticks");
    }

    /** A crafter facing {@code facing} whose arrow points at {@code target} (a world direction). */
    static BlockState crafter(Direction facing, Direction target) {
        BlockState base = AllBlocks.MECHANICAL_CRAFTER.getDefaultState().setValue(HorizontalKineticBlock.HORIZONTAL_FACING, facing);
        for (Pointing p : Pointing.values()) {
            BlockState s = base.setValue(MechanicalCrafterBlock.POINTING, p);
            if (MechanicalCrafterBlock.getTargetDirection(s) == target)
                return s;
        }
        throw new GameTestAssertException("no pointing of a crafter facing " + facing + " targets " + target);
    }
}
