package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.Items;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.SteamHelpers.*;

/**
 * Corpus 8: arm-fed burners and depots. One mechanical arm fuels two blaze burners with coal and moves
 * the iron the burners smelt off two depots into a chest. Each burner is the blasting catalyst of an
 * encased fan blowing up through it onto a depot; a chute from a chest of raw iron above keeps each
 * depot loaded.
 *
 * <pre>
 *   y7  chest A (raw iron)               chest B (raw iron)
 *   y6  chute                            chute
 *   y5  depot A                          depot B
 *   y4  burner A                         burner B
 *   y3  fan A (blows up)                 fan B (blows up)
 *   y2  cog A        arm (x 9)           cog B           out funnel (9,2,8) on a double chest (8..9,2,7)
 *   y1  motor (up, 256 RPM)                              coal smart chute (9,1,12) under the coal chest (9,2,12)
 *        x 8                              x 10
 * </pre>
 *
 * <p>The cogs, the arm (a small cog) and the fans turn at 256 RPM. The arm's targets, in its order
 * (PREFER_FIRST for both lists): take from the coal smart chute, depot A, depot B; deposit into burner A,
 * burner B, a brass funnel filtered to iron ingots on the output chest. The arm carries an item only when an
 * output accepts it ({@code ArmBlockEntity.getDistributableAmount} simulates the outputs), so coal goes only
 * to a burner that takes it, ingots only to the funnel, and raw iron stays on the depots.
 *
 * <p><b>Coal</b> ({@code BlazeBurnerBlockEntity.tick}, {@code tryUpdateFuel}): a burner loses one tick of
 * fuel every tick whether or not it heats anything; automation (the arm is not a player) adds fuel of the
 * same kind only when 500 ({@code INSERTION_THRESHOLD}) or fewer ticks remain, and a coal adds its furnace
 * burn time, 1,600 ({@code getBurnTime}). The arm checks its inputs every idle tick, so each burner takes one
 * coal per 1,600 ticks and never goes out: 2 x 1200 / 1600 = 1.5 coal per minute, exact (heater duty 1).
 * The smart chute (filter coal, count 1, {@code SmartChuteBlockEntity.getExtractionAmount}) takes one coal
 * from the chest each time the arm empties it, so the chest's count steps once per refuel. The burners start
 * with 1,600 and 800 ticks of fuel (set directly) so their refuels are half a period apart; with a window of
 * 4 burn periods (6,400 ticks) the least-squares slope of the two staircases is within 1/(4 n^2) = 1.6% of
 * the rate (n = 4), well inside the 0.1 per minute floor.
 *
 * <p><b>Iron</b> per station ({@code FanProcessing.applyProcessing}, {@code AirCurrent.findAffectedHandlers},
 * {@code AllFanProcessingTypes.BlastingType}): the fan's current passes the burner (tag
 * {@code create:fan_transparent}) and stops at the depot (13 px tall: {@code getFlowLimit} 1 + 1/32), whose
 * item it processes as BLASTING (a burner at FADING or hotter). A stack of 16 gets
 * {@code fanProcessingTime} (150) x ((16 - 1) / 16 + 1) + 1 = 151 on the first fan tick, counts down one per
 * tick, and smelts ({@code minecraft:raw_iron} to {@code iron_ingot}, 1 to 1) on the 152nd tick after; the
 * result keeps {@code processingTime} -1, so it is never processed again. The idle arm sees the ingots that
 * tick and needs ceil(1024 / 256) = 4 ticks to reach the depot ({@code tickMovementProgress}); the chute's
 * next stack waits at offset 0.5 and needs 3 more ticks at 0.2 per tick to drop in
 * ({@code ChuteBlockEntity.tick}). Cycle 1 + 151 + 4 + 3 = 159 ticks per 16 raw iron per station:
 * 2 x 16 x 1200 / 159 = 241.5 per minute each of raw iron used and ingots made. The arm spends 16 ticks per
 * trip (four moves) and has two depot trips per 159 ticks plus two coal trips per 1,600, so it is free when a
 * depot finishes except when a coal trip overlaps (at most 2 x 16 ticks of delay per 1,600, under 2%).
 * EXACT: no bounded rule lies on the path (the arm, a mover, has spare capacity).
 *
 * <p>Stocks: 1,600 raw iron per station lasts 1,600 / 16 x 159 = 15,900 ticks; the double chest holds 3,456
 * ingots (6,800 ticks make 2,066); 64 coal last 51,200 ticks.
 */
public final class ArmBurnersDepotsFactory implements CorpusFactory {
    static final int RPM = 256;
    static final int CYCLE = 159;
    static final int BATCH = 16;
    static final int WINDOW = 6400;
    static final double IRON_PER_MIN = 2.0 * BATCH * TICKS_PER_MINUTE / CYCLE;
    static final double COAL_PER_MIN = 2.0 * TICKS_PER_MINUTE / 1600;

    BlockPos burnerA, burnerB, depotA, depotB, arm;

    @Override
    public String name() {
        return "arm_burners_depots";
    }

    @Override
    public int warmupTicks() {
        return 400;
    }

    @Override
    public Built build(GameTestHelper h) {
        BlockPos chestA = at(h, 8, 7, 10), chestB = at(h, 10, 7, 10);
        BlockPos coalChest = at(h, 9, 2, 12), coalChute = at(h, 9, 1, 12);
        BlockPos outLeft = at(h, 9, 2, 7), outRight = at(h, 8, 2, 7), funnel = at(h, 9, 2, 8);
        depotA = at(h, 8, 5, 10);
        depotB = at(h, 10, 5, 10);
        burnerA = at(h, 8, 4, 10);
        burnerB = at(h, 10, 4, 10);
        arm = at(h, 9, 2, 10);

        for (BlockPos[] s : new BlockPos[][]{{chestA, at(h, 8, 6, 10), depotA, burnerA, at(h, 8, 3, 10)},
                {chestB, at(h, 10, 6, 10), depotB, burnerB, at(h, 10, 3, 10)}}) {
            chest(h, s[0]);
            fill(h, s[0], Items.RAW_IRON, 1600);
            chute(h, s[1]);
            depot(h, s[2]);
            burner(h, s[3]);
            fan(h, s[4], Direction.UP);
        }
        burnerFuel(h, burnerA, 1600);
        burnerFuel(h, burnerB, 800);

        chest(h, coalChest);
        fill(h, coalChest, Items.COAL, 64);
        smartChute(h, coalChute, Items.COAL, 1);
        doubleChest(h, outLeft, outRight, Direction.SOUTH);
        brassFunnel(h, funnel, Direction.SOUTH, Items.IRON_INGOT);

        cog(h, at(h, 8, 2, 10), Direction.Axis.Y);
        cog(h, at(h, 10, 2, 10), Direction.Axis.Y);
        motor(h, at(h, 8, 1, 10), Direction.UP, RPM);
        SteamHelpers.arm(h, arm, List.of(coalChute, depotA, depotB), List.of(burnerA, burnerB, funnel),
                ArmBlockEntity.SelectionMode.PREFER_FIRST);

        return new Built(arm, List.of(chestA, chestB, coalChest), List.of(outLeft), List.of(
                Expect.exact("minecraft:coal", -COAL_PER_MIN,
                        "BlazeBurnerBlockEntity: 1 tick burnt per tick, arm refuels at <= 500 with coal's 1,600: 2 burners x 1200/1600"),
                Expect.exact("minecraft:raw_iron", -IRON_PER_MIN,
                        "FanProcessing: 150 x ((16-1)/16+1) + 1 = 151, smelts on the 152nd tick; arm 4 ticks, chute 3: 16 per 159 ticks x 2 depots"),
                Expect.exact("minecraft:iron_ingot", IRON_PER_MIN,
                        "smelting raw_iron -> iron_ingot 1:1 (BlastingType tries smelting first), arm to the filtered funnel")),
                List.of(), WINDOW,
                "arm at 256 RPM, 2 burner-catalysed fans over depots, chute fed 16 at a time; coal staircases staggered by 800 ticks");
    }

    @Override
    public void perturb(GameTestHelper h, Built b, int t) {
        if (t % 400 == 0)
            log(name(), "t" + t, "burners " + burnTime(h, burnerA) + "/" + burnTime(h, burnerB) + " " + heat(h, burnerA) + "/" + heat(h, burnerB)
                    + ", arm speed " + speed(h, arm));
    }
}
