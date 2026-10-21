package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.logistics.tunnel.BeltTunnelBlock;
import com.simibubi.create.content.logistics.tunnel.BrassTunnelBlockEntity;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.scrollValue.ScrollValueBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * Corpus 10: brass tunnels, one line per rule, each with a different good so the district's net per good
 * tells the lines apart. Each line: a chest drops one item every 7 ticks through a smart chute (filter the
 * good, count 1) onto a four-block belt at 128 RPM; a brass tunnel on the last belt segment splits between
 * a millstone beside the belt (south side, the tunnel's T) and a chute straight ahead that drops into a
 * chest. The millstone empties through a chute into a chest under it. The rule decides the split:
 * <ul>
 *   <li>Line A, wheat, PREFER_NEAREST, millstone at 256 RPM: the millstone (capacity 109.09 a minute)
 *       is slower than the feed (171.43) but faster than half of it. The tunnel tries the side first
 *       every time, so the millstone's 64-slot input fills and then it takes exactly what it mills; the
 *       rest goes ahead. Round robin here would give it half the feed, 85.71.</li>
 *   <li>Line B, cornflower, ROUND_ROBIN, millstone at 128 RPM: the millstone (capacity 150) is faster
 *       than half the feed, so it never fills and the tunnel alternates strictly: half the feed each way.
 *       Prefer nearest here would give it 150.</li>
 * </ul>
 *
 * <pre>
 *   y5  chest
 *   y4  smart chute                   tunnel
 *   y3  belt     belt     belt        belt         chute (ahead)
 *   y2                                             chest (ahead)
 *      x4       x5       x6          x7           x8
 *   z+1 (south of the last segment): y3 millstone at x7, cogwheel (Y) at x8 over a motor (up) at y2;
 *   y2 chute and y1 chest under the millstone. Belt motor at (x4, y3, z-1), facing south.
 *   Line A at z 8, line B at z 13.
 * </pre>
 *
 * <p><b>Feed</b> ({@code ChuteBlockEntity.tick}, {@code handleInput}, {@code handleDownwardOutput};
 * {@code BeltInventory.canInsertAtFromSide}): the empty smart chute pulls one item and sets it at 1.0;
 * gravity moves it -0.2 a tick in float: 0.8, 0.6, 0.40000004, 0.20000003, 2.98e-8, then -0.19999997 &lt; 0,
 * when it goes onto the belt and the chute is empty; the next tick it pulls again. One item per 7 ticks:
 * R = 1200 / 7 = 171.43 a minute. The belt at 128 RPM moves 128 / 480 = 0.267 blocks a tick, so the item
 * inserted at 0.4375 clears the insertion block (beyond 1.5) in 4 ticks and items sit 1.87 blocks apart
 * (the belt keeps them 1.0 apart at most). The tunnel takes each on entering the last segment and hands
 * it out 3 ticks later ({@code BrassTunnelBlockEntity.tick}: -1, then 2, 1, 0), before the next arrives.
 *
 * <p><b>Split</b> ({@code BrassTunnelBlockEntity.distribute}, {@code addValidOutputsOf}): the targets are
 * the sides first, then straight ahead. PREFER_NEAREST starts at index 0 (the millstone) and moves on to
 * the chute when the millstone's insert refuses; ROUND_ROBIN tries only the target at
 * {@code previousOutputIndex} and advances it at every distribute. The ahead chute holds one item and
 * drops it into its chest in 6 ticks, so it is free for every item.
 *
 * <p><b>Mills</b> ({@code MillstoneBlockEntity}, as corpus 4): cycle ceil(t / s) + 1 with s = RPM / 16.
 * Wheat (t 150) at 256 RPM: ceil(150 / 16) + 1 = 11 ticks, 109.09 a minute. Cornflower (t 50) at 128 RPM:
 * ceil(50 / 8) + 1 = 8 ticks, 150 a minute.
 *
 * <p><b>Expected</b> (net over the district's stores; forwarded items go back into a store):
 * <ul>
 *   <li>Wheat: -109.09 (what the millstone mills once its input is full; the chest loses 171.43 and the
 *       ahead chest gains 62.34). Exact. The millstone's input fills at 62.34 a minute, 64 in about 1,232
 *       ticks: inside the 1,800-tick warm-up.</li>
 *   <li>Flour and seeds: as corpus 4, 1.5 and 0.25 per wheat, binomial.</li>
 *   <li>Cornflower: -85.71 (half the feed milled, half returned to a chest). Exact.</li>
 *   <li>Blue dye: 171.43 ({@code milling/cornflower}: 2 per cornflower). Exact.</li>
 * </ul>
 */
public final class BrassTunnelsFactory implements CorpusFactory {
    static final int FEED_PERIOD = 7;
    static final int BELT_RPM = 128;
    static final int WHEAT_MILL_RPM = 256, WHEAT_MILL_CYCLE = 11;
    static final int FLOWER_MILL_RPM = 128, FLOWER_MILL_CYCLE = 8;
    static final int LINE_A_Z = 8, LINE_B_Z = 13;

    @Override
    public String name() {
        return "brass_tunnels";
    }

    @Override
    public int warmupTicks() {
        return 1800;
    }

    /** The stores of one line: input chest, ahead chest, mill chest. */
    private record Line(BlockPos in, BlockPos ahead, BlockPos milled, BlockPos tunnel) {}

    @Override
    public Built build(GameTestHelper h) {
        Line a = line(h, LINE_A_Z, Items.WHEAT, WHEAT_MILL_RPM, BrassTunnelBlockEntity.SelectionMode.PREFER_NEAREST);
        Line b = line(h, LINE_B_Z, Items.CORNFLOWER, FLOWER_MILL_RPM, BrassTunnelBlockEntity.SelectionMode.ROUND_ROBIN);

        double feed = TICKS_PER_MINUTE / (double) FEED_PERIOD;
        double wheat = TICKS_PER_MINUTE / (double) WHEAT_MILL_CYCLE;
        List<BlockPos> outputs = new ArrayList<>(List.of(a.ahead, a.milled, b.ahead, b.milled));
        return new Built(a.tunnel, List.of(a.in, b.in), outputs, List.of(
                Expect.exact("minecraft:wheat", -wheat,
                        "PREFER_NEAREST: the side millstone (256 RPM, ceil(150/16) + 1 = 11 ticks) stays full; it mills 1200/11"),
                Expect.expected("create:wheat_flour", 1.5 * wheat, 0.375 * wheat, "milling/wheat: 1 + Binomial(2, 0.25) per wheat"),
                Expect.expected("minecraft:wheat_seeds", 0.25 * wheat, 0.1875 * wheat, "milling/wheat: Bernoulli(0.25) per wheat"),
                Expect.exact("minecraft:cornflower", -feed / 2,
                        "ROUND_ROBIN: strict alternation; feed 1200/7 (smart chute, 7 ticks per item), half milled"),
                Expect.exact("minecraft:blue_dye", feed, "milling/cornflower: 2 dye per cornflower, half the feed")),
                List.of(), 2400,
                "two lines fed 1200/7 a minute: wheat PREFER_NEAREST to a millstone (109.09) or ahead; cornflower ROUND_ROBIN to a millstone (150) or ahead");
    }

    private static Line line(GameTestHelper h, int z, Item good, int millRpm, BrassTunnelBlockEntity.SelectionMode mode) {
        BlockPos in = at(h, 4, 5, z), ahead = at(h, 8, 2, z), milled = at(h, 7, 1, z + 1);
        BlockPos mill = at(h, 7, 3, z + 1), tunnel = at(h, 7, 4, z);
        chest(h, in);
        fill(h, in, good, 1600);
        BlockPos feed = at(h, 4, 4, z);
        set(h, feed, AllBlocks.SMART_CHUTE.getDefaultState());
        FilteringBehaviour filter = BlockEntityBehaviour.get(h.getLevel(), feed, FilteringBehaviour.TYPE);
        filter.setFilter(new ItemStack(good));
        filter.count = 1;
        // Belt along +X: an X belt moves +X at negative speed; a motor facing south generates its value.
        belt(h, at(h, 4, 3, z), at(h, 7, 3, z));
        motor(h, at(h, 4, 3, z - 1), Direction.SOUTH, -BELT_RPM);
        // Ahead: a chute in front of the last segment over a chest.
        chest(h, ahead);
        set(h, at(h, 8, 3, z), AllBlocks.CHUTE.getDefaultState());
        // Side: the millstone south of the last segment, driven by a cogwheel east of it.
        chest(h, milled);
        set(h, at(h, 7, 2, z + 1), AllBlocks.CHUTE.getDefaultState());
        set(h, mill, AllBlocks.MILLSTONE.getDefaultState());
        cog(h, at(h, 8, 3, z + 1), Direction.Axis.Y);
        motor(h, at(h, 8, 2, z + 1), Direction.UP, millRpm);
        // The tunnel: what placing it computes (BeltTunnelBlock.getTunnelState: axis of the belt, and a
        // valid output on the left, EAST.getClockWise() = SOUTH, makes it T_LEFT). Setting the default
        // state and letting neighbour updates fix it does not work: updateShape keeps a state whose
        // axis and window match.
        set(h, tunnel, AllBlocks.BRASS_TUNNEL.getDefaultState()
                .setValue(BeltTunnelBlock.HORIZONTAL_AXIS, Direction.Axis.X)
                .setValue(BeltTunnelBlock.SHAPE, BeltTunnelBlock.Shape.T_LEFT));
        BlockEntityBehaviour.get(h.getLevel(), tunnel, ScrollValueBehaviour.TYPE).setValue(mode.ordinal());
        return new Line(in, ahead, milled, tunnel);
    }
}
