package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassReport;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllItems;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlock;
import com.simibubi.create.content.logistics.vault.ItemVaultBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.item.Items;
import net.neoforged.neoforge.items.IItemHandler;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * SEC-1 fixture (SECTIONS.md §9.2): a vault between two lines, one filling it and one drawing from it, and a
 * vanilla hopper also feeding the vault. The hopper is modelled (a mover from its chest to the vault) and
 * not counted, so the lines' sections stay one judging group with the vault interior, and the group's row
 * balances and passes.
 *
 * <p>The counted movers at the vault would cut the lines apart (S3): {@link FrozenNeighbourFactory}, the same
 * lines without the hopper, checks that they are two units there. Here the uncounted hopper at the vault keeps
 * them one group, so the two fixtures together show that the hopper is what joins them (review M5).
 *
 * <p>The hopper's chest ({@value #HOPPER_BRASS} ingots, near a chest's 27 stacks of 64) lasts 13,600 ticks at one per 8; the loaded
 * stage and the away span take about 11,000. Should a slow learning run it dry while away, the replay has a
 * store-limited mover to model, which it does by the chest's stock.
 *
 * <pre>
 *   y10  zinc chest (x 4, z 6)  copper chest (x 2)  mixer (4)  cog (5)
 *   y8   hopper chest (6, z 9)  basin (4, z 8)
 *   y7   hopper (6, z 9)        burner (4)         chute (5)      line A: BrassMixingFactory, 4 west, 5 up
 *   y5-6 VAULT x 5..6, z 8..9  (A's chute drops into it; the hopper feeds its far corner)
 *   y4   smart chute (5)                                          line B: PressSheetsFactory's press line
 *   y3   belt (5..6)  depot (7)  press (7, y 5)  drawing brass ingots out of the vault
 * </pre>
 * Line A mixes copper and zinc to brass ingots into the vault ({@code BrassMixingFactory}: 2 brass per 32
 * ticks); line B presses them to brass sheets from a smart chute under the vault ({@code PressSheetsFactory}:
 * one ingot per 19 ticks); the hopper moves a brass ingot from its chest into the vault every 8 ticks
 * ({@code HopperBlockEntity.MOVE_ITEM_SPEED}). The vault gains brass: A 75 and the hopper 150 a minute, B takes
 * 63.
 *
 * <p>Differs from the table: the second line presses the brass instead of mixing (no mixing recipe uses
 * brass), and the vault is drawn from by a smart chute, not a funnel (SC80).
 */
public final class HopperAtASharedVaultFactory implements CorpusFactory {
    static final int PRESS_RPM = 128, BELT_RPM = 64;
    static final int HOPPER_BRASS = 1700;

    /** Line A, BrassMixing's (8,y,8) moved by this. */
    static final BlockPos A_BY = new BlockPos(-4, 5, 0);
    static final BlockPos MIXER = new BlockPos(4, 10, 8), VAULT = new BlockPos(5, 6, 8), VAULT_BOTTOM = new BlockPos(5, 5, 8);
    static final BlockPos PRESS = new BlockPos(7, 5, 8), HOPPER = new BlockPos(6, 7, 9), HOPPER_CHEST = new BlockPos(6, 8, 9);
    static final BlockPos FEED = new BlockPos(5, 4, 8), BELT_END = new BlockPos(6, 3, 8);

    /** Whether the hopper is built ({@link FrozenNeighbourFactory} builds the same lines without it). */
    private final boolean withHopper;
    private final String name;

    public HopperAtASharedVaultFactory() {
        this(true, "hopper_vault");
    }

    HopperAtASharedVaultFactory(boolean withHopper, String name) {
        this.withHopper = withHopper;
        this.name = name;
    }

    @Override
    public String name() {
        return name;
    }

    @Override
    public int warmupTicks() {
        return 300;
    }

    @Override
    public Built build(GameTestHelper h) {
        // Line A, with its output chest where the vault will be: the vault replaces it.
        Built a = new BrassMixingFactory().build(new SectionsFx.Shifted(h, A_BY));
        // The vault, x 5..6, y 5..6, z 8..9 (corner at the +z side, as StationCargoGameTests lays one out).
        BlockState v = AllBlocks.ITEM_VAULT.getDefaultState().setValue(ItemVaultBlock.HORIZONTAL_AXIS, Direction.Axis.X);
        BlockPos corner = at(h, 5, 5, 9);
        for (int dx = 0; dx < 2; dx++)
            for (int dy = 0; dy < 2; dy++)
                for (int dz = 0; dz < 2; dz++)
                    h.getLevel().setBlock(corner.offset(dx, dy, -dz), v, 3);

        // Line B: the press line of PressSheetsFactory under the vault, for brass.
        set(h, abs(h, FEED), AllBlocks.SMART_CHUTE.getDefaultState());
        belt(h, at(h, 5, 3, 8), abs(h, BELT_END));
        motor(h, at(h, 5, 3, 7), Direction.SOUTH, -BELT_RPM);
        set(h, at(h, 7, 3, 8), AllBlocks.DEPOT.getDefaultState());
        set(h, abs(h, PRESS), AllBlocks.MECHANICAL_PRESS.getDefaultState().setValue(MechanicalPressBlock.HORIZONTAL_FACING, Direction.NORTH));
        motor(h, at(h, 7, 5, 9), Direction.NORTH, PRESS_RPM);
        BlockPos sheetChute = at(h, 7, 2, 8), sheets = at(h, 7, 1, 8);
        set(h, sheetChute, AllBlocks.SMART_CHUTE.getDefaultState());
        SteamHelpers.filtering(h, sheetChute).setFilter(new ItemStack(AllItems.BRASS_SHEET.get()));
        chest(h, sheets);

        // The hopper: a chest above it, the vault below.
        BlockPos hopperChest = abs(h, HOPPER_CHEST);
        if (withHopper) {
            chest(h, hopperChest);
            set(h, abs(h, HOPPER), Blocks.HOPPER.defaultBlockState());
            fill(h, hopperChest, AllItems.BRASS_INGOT.get(), HOPPER_BRASS);
        }

        double pressed = TICKS_PER_MINUTE / 19.0;
        Built b = new Built(abs(h, PRESS), withHopper ? List.of(hopperChest) : List.of(), List.of(sheets), List.of(
                Expect.exact("create:brass_sheet", pressed, "pressing/brass_ingot: one sheet per ingot (nominal, 19 ticks)")),
                withHopper ? "press line under the vault; a hopper feeds the vault a brass ingot per 8 ticks from " + HOPPER_BRASS
                        : "press line under the vault, which line A fills at 75 a minute and the press empties at 63");
        return SectionsFx.merge(7200, "line A (brass mixing) into a vault, line B (press) out of it" + (withHopper ? ", a hopper into it; " : "; ") + a.notes(),
                a, b, SectionsFx.bystander(h));
    }

    private static BlockPos abs(GameTestHelper h, BlockPos rel) {
        return at(h, rel.getX(), rel.getY(), rel.getZ());
    }

    /** The checks of §9.2's row: one judging group over the vault, the bystander's apart, learned, replayed. */
    static final class Check implements GlassReplayGameTests.Scenario {
        private boolean structure;

        @Override
        public void built(GlassReplayGameTests.Run run) {
            structure = false;
        }

        @Override
        public String ready(GlassReplayGameTests.Run run, GlassReport far) {
            if (far.units().size() < 2)
                return far.units().size() + " units of at least 2";
            for (GlassReport.Unit u : far.units())
                if (!SectionsFx.learned(u))
                    return "a unit has not learned: " + SectionsFx.describe(far);
            String gate = SectionsFx.replayGate(far, SectionsFx.unitAt(far, run.far.absolutePos(MIXER)));
            if (gate != null)
                return gate;
            if (!structure) {
                structure = true;
                structure(run, far);
            }
            return null;
        }

        private void structure(GlassReplayGameTests.Run run, GlassReport r) {
            BlockPos mixer = run.far.absolutePos(MIXER), press = run.far.absolutePos(PRESS), vault = run.far.absolutePos(VAULT);
            BlockPos hopper = run.far.absolutePos(HOPPER);
            GlassReport.Unit group = SectionsFx.unitAt(r, mixer);
            GlassReport.Unit pressUnit = SectionsFx.unitAt(r, press);
            GlassReport.Unit bystander = SectionsFx.unitAt(r, run.far.absolutePos(SectionsFx.BYSTANDER_FEED));
            String d = SectionsFx.describe(r);
            SectionsFx.info(run, "far units", d);
            SectionsFx.check(run, "the mixing line and the press line are in one judging group", group != null && group == pressUnit, d);
            // Review M5: without the hopper the vault is counted and the lines are two units (FrozenNeighbour checks it); the
            // uncounted hopper here is what keeps them one group, which the check above makes.
            SectionsFx.check(run, "the group has the lines' sections (at least two)", group != null && group.sections() >= 2,
                    group == null ? "no unit" : group.sections() + " sections");
            SectionsFx.check(run, "the group's box holds the vault", group != null && group.holds(vault.getX(), vault.getY(), vault.getZ()),
                    group == null ? "no unit" : group.summary());
            SectionsFx.check(run, "the group's box holds the hopper (modelled, in the group)",
                    group != null && group.holds(hopper.getX(), hopper.getY(), hopper.getZ()), group == null ? "no unit" : group.summary());
            SectionsFx.check(run, "the bystander is a unit of its own", bystander != null && bystander != group && bystander.sections() == 1, d);
            SectionsFx.check(run, "the group's row passes", SectionsFx.learned(group), group == null ? "no unit" : group.summary());
            // The hopper does its work: brass leaves its chest.
            long left = HOPPER_BRASS - count(run.level().getCapability(net.neoforged.neoforge.capabilities.Capabilities.ItemHandler.BLOCK,
                    run.far.absolutePos(HOPPER_CHEST), null));
            SectionsFx.check(run, "the hopper has moved brass into the vault", left > 0, left + " ingots left its chest");
        }

        private static long count(IItemHandler inv) {
            long n = 0;
            for (int i = 0; inv != null && i < inv.getSlots(); i++)
                n += inv.getStackInSlot(i).getCount();
            return n;
        }

        @Override
        public void away(GlassReplayGameTests.Run run, GlassReport far, boolean last) {
            if (!last)
                return;
            GlassReport.Unit group = SectionsFx.unitAt(far, run.far.absolutePos(MIXER));
            SectionsFx.info(run, "units while away", SectionsFx.describe(far));
            SectionsFx.check(run, "the group replays while away", group != null && group.mayReplay() && !group.frozen() && !group.stale(),
                    group == null ? "no unit" : group.summary());
        }

        @Override
        public void judged(GlassReplayGameTests.Run run, GlassReport far, GlassReport control) {
            GlassReport.Unit group = SectionsFx.unitAt(far, run.far.absolutePos(MIXER));
            SectionsFx.info(run, "units after the reconcile", SectionsFx.describe(far));
            SectionsFx.check(run, "the group is neither frozen nor stale after the reconcile", group != null && !group.frozen() && !group.stale(),
                    group == null ? "no unit" : group.summary());
        }
    }
}
