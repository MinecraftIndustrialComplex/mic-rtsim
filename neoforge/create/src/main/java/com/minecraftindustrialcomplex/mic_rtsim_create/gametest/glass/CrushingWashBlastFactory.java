package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.crusher.CrushingWheelControllerBlock;
import com.simibubi.create.content.kinetics.fan.EncasedFanBlock;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 1: crushing wheels with washing and blasting. Two lines, each a pair of crushing wheels over a
 * slow belt that runs through a fan's air current; line A washes (water), line B blasts (lava). One line
 * cannot show both: blasting destroys every washing output of a crushed ore (nuggets have no smelting
 * recipe, {@code AllFanProcessingTypes.BlastingType.canProcess}), and washing does nothing to ingots.
 * <ul>
 *   <li>Line A (z 8): raw iron, crushed to crushed raw iron (and an experience nugget at 0.75), washed to
 *       9 iron nuggets (and redstone at 0.75); the experience nugget has no washing recipe and passes.</li>
 *   <li>Line B (z 14): raw gold, crushed to crushed raw gold (and 2 experience nuggets at 0.75), blasted
 *       to a gold ingot; blasting destroys the experience nuggets.</li>
 * </ul>
 *
 * <pre>
 *   y6  chest
 *   y5  smart chute (good x2)
 *   y4  controller (wheels at z-1, z+1, axis X; motors at x3 facing east, +50 and -50)
 *   y3  (air: drop)  stopper  .  .  .  .  .  .  iron bars   water|lava   fan (west)   motor (west)
 *   y2  belt ------------------------------------------ belt   chute
 *   y1                                                          chest
 *       x4           x5       x6 ...               x12         x13          x14          x15
 *   belt motor at (x4, y2, z-1), facing south; glass beside the water or lava at z-1 and z+1.
 * </pre>
 *
 * <p><b>Crushing</b> ({@code CrushingWheelControllerBlockEntity.tick}, {@code itemInserted};
 * {@code CrushingWheelControllerBlock.updateSpeed}): wheels at 50 RPM give crushingspeed 50 / 50 = 1, so
 * speed 4. The smart chute hands in exactly 2 (filter count 2) when the controller is empty
 * ({@code ProcessingInventory} takes items only then): remainingTime = 400 (both recipes), and each tick
 * takes clamp(4 / log2(2), 0.25, 20) = 4. The recipe applies at the first tick below 20 (400 - 4 x 96 =
 * 16); {@code applyRecipe} clears the inventory, which also zeroes remainingTime
 * ({@code ProcessingInventory.clear}), so the 97th tick drops the outputs as item entities (no belt input
 * directly under the controller) and empties it. The parked chute inserts the next 2 two ticks later (as
 * corpus 5's saw): 2 raw ores per 99 ticks, 24.242 a minute, exact (measured: 2 per 99, traced tick by
 * tick). A count of 1 would not do: log2(1) = 0 makes the first stage 20 a tick whatever the speed.
 *
 * <p><b>Belt and fan</b> ({@code AirCurrent.rebuild}, {@code findAffectedHandlers}, {@code getFlowLimit};
 * {@code FanProcessing.applyProcessing}): the fan at 256 RPM blows 20 blocks west through the catalyst
 * and the iron bars (fan-transparent) and stops at the stopper, a full block. The belt one below the
 * current, segments x6 to x12, is in the zone; the belt at 16 RPM moves 16 / 480 = 1/30 block a tick, so
 * an item spends 7 x 30 = 210 ticks there. A stack of up to 16 is processed after 150 + 1 ticks in the
 * zone and its outputs never again ({@code processingTime} -1 is copied, {@code getSimilar}). The
 * outputs reach a chute at the belt's end and drop into a chest. The dropped stacks (two per cycle) land
 * on the belt's first segment, which takes a new stack once the last has moved 1.06 blocks: 32 ticks.
 * Fluids stay put: the chute under them blocks motion, and flowing water cannot waterlog iron bars.
 *
 * <p><b>Expected</b> (a = 2400 / 99 = 24.242 raw ores a minute per line):
 * <ul>
 *   <li>Raw iron -a, iron nuggets +9a, exact; redstone and experience nuggets 0.75a each, binomial
 *       (variance 0.1875 per raw iron).</li>
 *   <li>Raw gold -a, gold ingots +a, exact; no experience nuggets (burnt).</li>
 * </ul>
 */
public final class CrushingWashBlastFactory implements CorpusFactory {
    static final int WHEEL_RPM = 50, BELT_RPM = 16, FAN_RPM = 256;
    /** Ticks per two raw ores. */
    static final int CYCLE = 99;
    static final int LINE_A_Z = 8, LINE_B_Z = 14;

    @Override
    public String name() {
        return "crushing_wash_blast";
    }

    @Override
    public int warmupTicks() {
        return 600;
    }

    private record Line(BlockPos in, BlockPos out, BlockPos controller) {}

    @Override
    public Built build(GameTestHelper h) {
        Line a = line(h, LINE_A_Z, Items.RAW_IRON, Blocks.WATER.defaultBlockState());
        Line b = line(h, LINE_B_Z, Items.RAW_GOLD, Blocks.LAVA.defaultBlockState());

        double ore = 2 * TICKS_PER_MINUTE / (double) CYCLE;
        return new Built(a.controller, List.of(a.in, b.in), List.of(a.out, b.out), List.of(
                Expect.exact("minecraft:raw_iron", -ore,
                        "CrushingWheelControllerBlockEntity: 2 per insert, 4 a tick from 400, applied at 96, out at 97; chute +2: 2 per 99 ticks"),
                Expect.exact("minecraft:iron_nugget", 9 * ore, "splashing/crushed_raw_iron: 9 nuggets per crushed ore, 210 ticks in the zone"),
                Expect.expected("minecraft:redstone", 0.75 * ore, 0.1875 * ore, "splashing/crushed_raw_iron: Bernoulli(0.75) per crushed ore"),
                Expect.expected("create:experience_nugget", 0.75 * ore, 0.1875 * ore,
                        "crushing/raw_iron: Bernoulli(0.75); no splashing recipe, passes the water"),
                Expect.exact("minecraft:raw_gold", -ore, "same crusher, same cycle"),
                Expect.exact("minecraft:gold_ingot", ore, "blasting/gold_ingot_from_crushed; the experience nuggets burn")),
                "two crushers at 50 RPM fed 2 at a time; belts at 16 RPM through a washing (water) and a blasting (lava) fan zone");
    }

    private static Line line(GameTestHelper h, int z, Item good, BlockState catalyst) {
        BlockPos in = at(h, 4, 6, z), out = at(h, 13, 1, z), controller = at(h, 4, 4, z);
        chest(h, in);
        fill(h, in, good, 512);
        BlockPos feed = at(h, 4, 5, z);
        set(h, feed, AllBlocks.SMART_CHUTE.getDefaultState());
        FilteringBehaviour filter = BlockEntityBehaviour.get(h.getLevel(), feed, FilteringBehaviour.TYPE);
        filter.setFilter(new ItemStack(good));
        filter.count = 2;
        // The controller first, so the chute never finds air under it (it would drop the ores past the
        // wheels); the wheels make it valid and face it down once they turn (CrushingWheelBlock.updateControllers:
        // the wheel at z-1 turning positive, the one at z+1 negative).
        set(h, controller, AllBlocks.CRUSHING_WHEEL_CONTROLLER.getDefaultState()
                .setValue(CrushingWheelControllerBlock.FACING, Direction.DOWN).setValue(CrushingWheelControllerBlock.VALID, false));
        for (int side : new int[] {-1, 1}) {
            set(h, at(h, 4, 4, z + side), AllBlocks.CRUSHING_WHEEL.getDefaultState().setValue(BlockStateProperties.AXIS, Direction.Axis.X));
            motor(h, at(h, 3, 4, z + side), Direction.EAST, -side * WHEEL_RPM);
        }
        // Belt x4 to x12 moving +X (negative speed on an X belt; a south-facing motor keeps its sign).
        belt(h, at(h, 4, 2, z), at(h, 12, 2, z));
        motor(h, at(h, 4, 2, z - 1), Direction.SOUTH, -BELT_RPM);
        // The zone: stopper, iron bars, the catalyst held in by glass and the chute, the fan.
        set(h, at(h, 5, 3, z), Blocks.SMOOTH_STONE.defaultBlockState());
        set(h, at(h, 12, 3, z), Blocks.IRON_BARS.defaultBlockState());
        chest(h, out);
        set(h, at(h, 13, 2, z), AllBlocks.CHUTE.getDefaultState());
        set(h, at(h, 13, 3, z - 1), Blocks.GLASS.defaultBlockState());
        set(h, at(h, 13, 3, z + 1), Blocks.GLASS.defaultBlockState());
        set(h, at(h, 13, 3, z), catalyst);
        // The fan faces west and pushes west at negative speed (EncasedFanBlockEntity.getAirFlowDirection);
        // a west-facing motor generates minus its value.
        set(h, at(h, 14, 3, z), AllBlocks.ENCASED_FAN.getDefaultState().setValue(EncasedFanBlock.FACING, Direction.WEST));
        motor(h, at(h, 15, 3, z), Direction.WEST, FAN_RPM);
        return new Line(in, out, controller);
    }
}
