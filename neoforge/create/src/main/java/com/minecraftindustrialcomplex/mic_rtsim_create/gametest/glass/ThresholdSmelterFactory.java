package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.redstone.DirectedDirectionalBlock;
import com.simibubi.create.content.redstone.thresholdSwitch.ThresholdSwitchBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.HopperBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.AttachFace;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.SteamHelpers.*;

/**
 * Corpus 12: a threshold-controlled smelter. Two lava-catalysed fans blast sand to glass on two depots; a
 * mechanical arm moves the glass through a brass funnel into the output chest. A threshold switch reads
 * that chest and, through a clutch, stops the whole kinetic line (fans and arm) while the stock is high. A
 * hopper under the chest drains it into a sink chest at a steady rate, so the line cycles on and off.
 *
 * <pre>
 *   y8  sand chest A                       sand chest B
 *   y7  chute                              chute
 *   y6  depot A                            depot B
 *   y5  lava A (walled)                    lava B (walled)
 *   y4  fan A (blows up)                   fan B (blows up)
 *   y3  cog A        arm (x 9)             cog B             funnel (8,3,8) facing up, glass only
 *   y2  clutch                                               output chest (8,2,8) &lt;- switch (8,2,9) beside the clutch
 *   y1  motor (up, 256 RPM)                                  hopper (8,1,8) west into the sink chest (7,1,8)
 *        x 8                                x 10
 * </pre>
 *
 * <p><b>Production while on</b> (the station of corpus 8 with lava as the catalyst:
 * {@code AllFanProcessingTypes.BlastingType.isValidAt} takes a lava source; smelting sand gives glass 1 to 1;
 * the result keeps {@code processingTime} -1 so it is never blasted again): 16 sand per
 * 1 + 151 + 4 + 3 = 159 ticks per station, P = 2 x 16 x 1200 / 159 = 241.5 glass per minute into the chest.
 * The arm (deposit: the funnel only, so it never lifts sand: {@code getDistributableAmount}) is free when a
 * depot finishes: the two stations settle 16 ticks apart (one arm trip) after the first, simultaneous batch.
 *
 * <p><b>Drain</b> ({@code HopperBlockEntity.tryMoveItems}, {@code MOVE_ITEM_SPEED} 8): one item per 8 ticks,
 * D = 150 per minute, as long as the output chest is not empty (it never is: see the low point below).
 *
 * <p><b>Switch</b> ({@code ThresholdSwitchBlockEntity.updateCurrentLevel} on its lazy tick, every 10 ticks;
 * the filter is empty, so every item in the chest counts): on when the count is at least 128
 * ({@code onWhenAbove}), off when at most 64 ({@code offWhenBelow}), the defaults. The output follows 2 ticks
 * later ({@code scheduleBlockTick}); the switch powers every neighbour but its target
 * ({@code ThresholdSwitchBlock.getSignal}), here the clutch. A powered clutch gives speed modifier 0 on the
 * side away from its source ({@code ClutchBlockEntity.getRotationSpeedModifier}); it detaches at once and
 * re-attaches a tick later ({@code GearshiftBlock.neighborChanged}, {@code tick}). Work in progress (a
 * depot's countdown, the arm's load) freezes and resumes.
 *
 * <p><b>Regimes</b> (output chest stock s):
 * <ul>
 *   <li>ON, switch off, line at 256 RPM: ds/dt = P - D = +91.5 per minute; from 64 to 128 in
 *       64 x 1200 / 91.5 = 839 ticks;</li>
 *   <li>OFF, switch on, line at 0 RPM: ds/dt = -D = -150 per minute; from 128 to 64 in 512 ticks.</li>
 * </ul>
 * Period 839 + 512 = 1,351 ticks nominal, duty D / P = 0.621, so about 5 cycles in the window. Overshoot: at
 * the check that turns the switch on, s is at most 127 + 16 (one arm delivery since the last check), and one
 * more delivery can land in the 2-tick delay: s up to {@value #STOCK_HI}. At the check that turns it off s is
 * at least 63 (two hopper moves in 10 ticks); a restart can take a fresh batch (152 ticks) and an arm trip
 * (16) before glass lands, 21 more hopper moves: s down to {@value #STOCK_LO}, above 0.
 *
 * <p><b>Rates</b> (BOUNDED: a controller is on the path, GLASSBOX.md §4.3). Over time the controlled stock's
 * inflow equals its outflow, so glass made (output chest plus sink, both stores) averages D. The stores'
 * glass count is sink(0) + D t + s(t); s stays inside a band of B = {@value #STOCK_HI} - {@value #STOCK_LO}
 * items, and a least-squares slope over W ticks of anything inside a band B differs from the trend by at
 * most 1.5 B / W per tick (the worst case is a step at mid-window). Sand taken from the chests is glass made
 * plus what is in flight: per station 0 to 16 in the chute and 0 to 16 on the depot, and up to 16 in the arm,
 * a band of {@value #FLIGHT} more. The regime bounds themselves are [0, P] (all off, all on).
 *
 * <p>Stocks: 1,024 sand per station (about 500 used in warm-up plus window); the sink takes 975 of its 1,728;
 * the output chest starts at 96 glass so the first cycle begins in warm-up.
 */
public final class ThresholdSmelterFactory implements CorpusFactory {
    static final int RPM = 256;
    static final int CYCLE = 159;
    static final int BATCH = 16;
    static final int WINDOW = 7200;
    static final int ON_ABOVE = 128;
    static final int OFF_BELOW = 64;
    static final int STOCK_HI = 128 - 1 + 2 * 16;
    static final int STOCK_LO = 64 - 1 - 21;
    static final int FLIGHT = 80;
    static final double DRAIN_PER_MIN = TICKS_PER_MINUTE / 8.0;
    static final double ON_PER_MIN = 2.0 * BATCH * TICKS_PER_MINUTE / CYCLE;

    BlockPos sw, clutch, arm, fanA, fanB, outChest;
    boolean lastPowered;
    int transitions;

    @Override
    public String name() {
        return "threshold_smelter";
    }

    @Override
    public int warmupTicks() {
        return 600;
    }

    @Override
    public Built build(GameTestHelper h) {
        BlockState stone = Blocks.STONE.defaultBlockState();
        BlockPos rawA = at(h, 8, 8, 10), rawB = at(h, 10, 8, 10);
        BlockPos depotA = at(h, 8, 6, 10), depotB = at(h, 10, 6, 10);
        fanA = at(h, 8, 4, 10);
        fanB = at(h, 10, 4, 10);
        outChest = at(h, 8, 2, 8);
        BlockPos funnel = at(h, 8, 3, 8), hopper = at(h, 8, 1, 8), sink = at(h, 7, 1, 8);
        sw = at(h, 8, 2, 9);
        clutch = at(h, 8, 2, 10);
        arm = at(h, 9, 3, 10);

        for (int x : new int[]{8, 10}) {
            BlockPos lava = at(h, x, 5, 10);
            for (Direction d : Direction.Plane.HORIZONTAL)
                set(h, lava.relative(d), stone);
            fan(h, at(h, x, 4, 10), Direction.UP);
            set(h, lava, Blocks.LAVA.defaultBlockState());
            depot(h, at(h, x, 6, 10));
            chute(h, at(h, x, 7, 10));
            BlockPos raw = at(h, x, 8, 10);
            chest(h, raw);
            fill(h, raw, Items.SAND, 1024);
        }

        chest(h, outChest);
        fill(h, outChest, Items.GLASS, 96);
        brassFunnel(h, funnel, Direction.UP, Items.GLASS);
        chest(h, sink);
        set(h, hopper, Blocks.HOPPER.defaultBlockState().setValue(HopperBlock.FACING, Direction.WEST));

        motor(h, at(h, 8, 1, 10), Direction.UP, RPM);
        set(h, clutch, AllBlocks.CLUTCH.getDefaultState().setValue(BlockStateProperties.AXIS, Direction.Axis.Y));
        cog(h, at(h, 8, 3, 10), Direction.Axis.Y);
        cog(h, at(h, 10, 3, 10), Direction.Axis.Y);
        SteamHelpers.arm(h, arm, List.of(depotA, depotB), List.of(funnel), ArmBlockEntity.SelectionMode.PREFER_FIRST);

        set(h, sw, AllBlocks.THRESHOLD_SWITCH.getDefaultState()
                .setValue(DirectedDirectionalBlock.TARGET, AttachFace.WALL).setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.NORTH));
        ThresholdSwitchBlockEntity t = be(h, sw, ThresholdSwitchBlockEntity.class);
        t.onWhenAbove = ON_ABOVE;
        t.offWhenBelow = OFF_BELOW;
        lastPowered = false;
        transitions = 0;

        double d = DRAIN_PER_MIN;
        double glassBand = 1.5 * (STOCK_HI - STOCK_LO) / WINDOW * TICKS_PER_MINUTE;
        double sandBand = 1.5 * (STOCK_HI - STOCK_LO + FLIGHT) / WINDOW * TICKS_PER_MINUTE;
        return new Built(arm, List.of(rawA, rawB), List.of(outChest, sink), List.of(
                Expect.bounded("minecraft:glass", d - glassBand, d, d + glassBand,
                        "threshold switch (on >= 128, off <= 64) on the output chest gates the line; inflow averages the hopper's 1 per 8 ticks;"
                                + " +-1.5 x stock band (" + STOCK_LO + ".." + STOCK_HI + ") / window; regime bounds [0, " + fmt(ON_PER_MIN) + "]"),
                Expect.bounded("minecraft:sand", -d - sandBand, -d, -d + sandBand,
                        "sand taken = glass made + in flight (chutes, depots, arm: 0.." + FLIGHT + "); +-1.5 x (stock band + flight band) / window")),
                List.of(), WINDOW,
                "on: 2 lava fans x 16 per 159 ticks = " + fmt(ON_PER_MIN) + "/min in, hopper 150/min out; switch 128/64 through a clutch;"
                        + " nominal on 839, off 512 ticks, period 1351, duty 0.621");
    }

    private static String fmt(double v) {
        return String.format(java.util.Locale.ROOT, "%.1f", v);
    }

    @Override
    public void perturb(GameTestHelper h, Built b, int t) {
        boolean powered = h.getLevel().getBlockEntity(sw) instanceof ThresholdSwitchBlockEntity s && s.isPowered();
        int stock = 0;
        var inv = items(h, outChest);
        for (int i = 0; i < inv.getSlots(); i++)
            stock += inv.getStackInSlot(i).getCount();
        if (t == 0) {
            lastPowered = powered;
            log(name(), "t0", "switch " + (powered ? "on" : "off") + " at stock " + stock);
        } else if (powered != lastPowered) {
            transitions++;
            log(name(), "t" + t, "switch " + (powered ? "on (line stopped)" : "off (line running)") + " at stock " + stock
                    + ", transition " + transitions + ", fans " + speed(h, fanA) + "/" + speed(h, fanB) + " arm " + speed(h, arm));
            lastPowered = powered;
        }
        if (t % 600 == 0)
            log(name(), "t" + t, "stock " + stock + ", switch " + (powered ? "on" : "off") + ", clutch "
                    + h.getLevel().getBlockState(clutch).getValue(BlockStateProperties.POWERED) + ", fans " + speed(h, fanA) + "/" + speed(h, fanB)
                    + " arm " + speed(h, arm) + ", transitions " + transitions);
    }
}
