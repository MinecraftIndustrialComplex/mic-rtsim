package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllItems;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlock;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 3: pressing sheets. A chest of iron ingots drops whole stacks (a smart chute, no filter, pulls
 * 64) onto a two-block belt that ends in a depot; a mechanical press two blocks above the depot presses
 * the held stack one ingot at a time; a smart chute under the depot, filtered to iron sheets, takes the
 * sheets down into a chest. Press at 128 RPM, belt at 64 RPM, each on its own creative motor.
 *
 * <pre>
 *   y5  chest (ingots)           press (axis Z)  motor (north) at z+1
 *   y4  smart chute
 *   y3  belt  belt  -> depot
 *   y2                 smart chute (iron sheet)
 *   y1                 chest (sheets)
 *      x5    x6       x7
 * </pre>
 *
 * <p><b>Rates</b> ({@code PressingBehaviour.tick}, {@code getRunningTickSpeed}; {@code BeltPressingCallbacks};
 * {@code DepotBehaviour.tick}, {@code applyToAllItems}):
 * <ul>
 *   <li>Press step s = (int) lerp(clamp(128/512, 0, 1), 1, 60) = (int) 15.75 = 15. From {@code start}
 *       (runningTicks 0) it takes ceil(120/15) = 8 press ticks to reach 120 (the step that crosses 120
 *       snaps to it), floor(120/15) + 1 = 9 more to exceed 240, and one tick to see it and finish: 18
 *       press ticks from the start to {@code running = false}.</li>
 *   <li>The depot presses while {@code runningTicks == 120} ({@code whenItemHeld}); the stack is not
 *       bulk ({@code bulkPressing} false), so one ingot becomes a sheet in the output buffer and the
 *       rest stays held and locked. The tick after the press finishes, {@code whenItemHeld} answers
 *       PASS and the depot unlocks it; the tick after that {@code handleReceivedItem} starts the press
 *       again. Whichever of the two block entities ticks first, consecutive starts are 19 ticks apart:
 *       1200 / 19 = 63.158 ingots and sheets per minute, exact.</li>
 *   <li>Per 64 ingots one hand-off: the last ingot becomes the held sheet, the smart chute takes it, the
 *       belt's next stack slides in and centres in 9 ticks ({@code DepotBehaviour.tick(heldItem)}:
 *       beltPosition += (0.5 - pos)/4 until the gap is under 1/16). A replacement that arrives before the
 *       press finishes costs nothing; one that arrives later costs at most the chute's few ticks: under
 *       0.6% of 64 x 19 = 1,216 ticks, inside the tolerance and left out of the rate.</li>
 *   <li>The belt and both chutes move stacks far faster than one per 1,216 ticks.</li>
 * </ul>
 * The window is 7,200 ticks because the input chest drains in steps of 64 (see {@link #WINDOW}).
 */
public final class PressSheetsFactory implements CorpusFactory {
    static final int PRESS_RPM = 128;
    static final int BELT_RPM = 64;
    static final int CYCLE = 19;
    /**
     * The input chest loses 64 ingots per 1,216 ticks: over 2,400 ticks a least-squares slope of that
     * staircase is off by up to 24% with the phase, over 7,200 by up to 2.3% (simulated).
     */
    static final int WINDOW = 7200;

    @Override
    public String name() {
        return "press_sheets";
    }

    @Override
    public int warmupTicks() {
        return 300;
    }

    @Override
    public Built build(GameTestHelper h) {
        BlockPos in = at(h, 5, 5, 8), depot = at(h, 7, 3, 8), press = at(h, 7, 5, 8), out = at(h, 7, 1, 8);
        chest(h, in);
        fill(h, in, Items.IRON_INGOT, 1024);
        set(h, at(h, 5, 4, 8), AllBlocks.SMART_CHUTE.getDefaultState());
        belt(h, at(h, 5, 3, 8), at(h, 6, 3, 8));
        // An X belt moves +X when its speed is negative (BeltBlockEntity.getMovementFacing); a motor facing
        // south (+Z) generates its value as is (KineticBlockEntity.convertToDirection).
        motor(h, at(h, 5, 3, 7), Direction.SOUTH, -BELT_RPM);
        set(h, depot, AllBlocks.DEPOT.getDefaultState());
        set(h, press, AllBlocks.MECHANICAL_PRESS.getDefaultState().setValue(MechanicalPressBlock.HORIZONTAL_FACING, Direction.NORTH));
        motor(h, at(h, 7, 5, 9), Direction.NORTH, PRESS_RPM);
        BlockPos sheetChute = at(h, 7, 2, 8);
        set(h, sheetChute, AllBlocks.SMART_CHUTE.getDefaultState());
        BlockEntityBehaviour.get(h.getLevel(), sheetChute, FilteringBehaviour.TYPE).setFilter(new ItemStack(AllItems.IRON_SHEET.get()));
        chest(h, out);

        double r = TICKS_PER_MINUTE / (double) CYCLE;
        return new Built(press, List.of(in), List.of(out), List.of(
                Expect.exact("minecraft:iron_ingot", -r, "PressingBehaviour: s 15, 18 press ticks, depot unlock and restart +1: 19 ticks per ingot"),
                Expect.exact("create:iron_sheet", r, "pressing/iron_ingot: one sheet per ingot")),
                List.of(), WINDOW, "press at 128 RPM over a depot fed 64-stacks by a belt; 1,024 ingots last 19,456 ticks");
    }
}
