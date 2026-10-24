package com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass;

import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllItems;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlock;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.*;

/**
 * SEC-1 fixture (SECTIONS.md §9.2): a station's port into a vault that feeds a press line. The line is
 * {@link PressSheetsFactory}'s, with a vault where its input chest stood, and the port of
 * {@link StationCargoGameTests} on top of the vault; {@link StationPressLineGameTests} runs the station's
 * calls against it.
 *
 * <pre>
 *   y10  PORTABLE STORAGE INTERFACE (x 12, facing south)
 *   y9   smart chute (iron ingots, 64) pulling out of the interface into the vault
 *   y7-8 VAULT x 12..13, z 7..8 (3,000 iron ingots)
 *   y6   smart chute (no filter) pulling out of the vault
 *   y5   belt x 12..14 -> depot x 15 (press over it at y 7, axis Z), belt motor at z 7
 *   y4   smart chute (iron sheets)
 *   y3   chest (sheets)
 * </pre>
 * The press line is the vault's only section user, so the port's vault lies in the press line's unit and the
 * station's ports (one inventory) regroup nothing (SC75 regroups from two). The bystander
 * ({@link SectionsFx#bystander}) makes the report carry its units.
 */
public final class StationPressLineFactory implements CorpusFactory {
    static final int PRESS_RPM = 128, BELT_RPM = 64;
    static final int CYCLE = 19;
    static final int VAULT_INGOTS = 3000;

    /** The interface; the vault's corner (as {@link StationCargoGameTests#vault} lays it out) is three below. */
    static final BlockPos PSI = new BlockPos(12, 10, 8);
    static final BlockPos VAULT = PSI.below(3), PORT_CHUTE = PSI.below();
    static final BlockPos DRAW = new BlockPos(12, 6, 8), BELT_START = new BlockPos(12, 5, 8), BELT_END = new BlockPos(14, 5, 8);
    static final BlockPos DEPOT = new BlockPos(15, 5, 8), PRESS = new BlockPos(15, 7, 8);
    static final BlockPos SHEET_CHUTE = new BlockPos(15, 4, 8), SHEETS = new BlockPos(15, 3, 8);
    /** Where the station block is: air in the district. */
    static final BlockPos STATION = PSI.above(2);

    @Override
    public String name() {
        return "station_press_line";
    }

    @Override
    public int warmupTicks() {
        return 300;
    }

    @Override
    public Built build(GameTestHelper h) {
        // The vault forms on its first tick: the test fills it then ({@link StationPressLineGameTests}).
        StationCargoGameTests.vault(h.getLevel(), abs(h, VAULT));
        SteamHelpers.smartChute(h, abs(h, PORT_CHUTE), Items.IRON_INGOT, 64);
        set(h, abs(h, PSI), AllBlocks.PORTABLE_STORAGE_INTERFACE.getDefaultState()
                .setValue(com.simibubi.create.content.contraptions.actors.psi.PortableStorageInterfaceBlock.FACING, Direction.SOUTH));
        set(h, abs(h, DRAW), AllBlocks.SMART_CHUTE.getDefaultState());
        belt(h, abs(h, BELT_START), abs(h, BELT_END));
        motor(h, at(h, 12, 5, 7), Direction.SOUTH, -BELT_RPM);
        set(h, abs(h, DEPOT), AllBlocks.DEPOT.getDefaultState());
        set(h, abs(h, PRESS), AllBlocks.MECHANICAL_PRESS.getDefaultState().setValue(MechanicalPressBlock.HORIZONTAL_FACING, Direction.NORTH));
        motor(h, at(h, 15, 7, 9), Direction.NORTH, PRESS_RPM);
        set(h, abs(h, SHEET_CHUTE), AllBlocks.SMART_CHUTE.getDefaultState());
        SteamHelpers.filtering(h, abs(h, SHEET_CHUTE)).setFilter(new ItemStack(AllItems.IRON_SHEET.get()));
        chest(h, abs(h, SHEETS));

        double r = TICKS_PER_MINUTE / (double) CYCLE;
        Built line = new Built(abs(h, PRESS), List.of(abs(h, VAULT)), List.of(abs(h, SHEETS)), List.of(
                Expect.exact("minecraft:iron_ingot", -r, "PressingBehaviour on a depot: 19 ticks per ingot (nominal)"),
                Expect.exact("create:iron_sheet", r, "pressing/iron_ingot: one sheet per ingot")),
                "press line under a vault with a station port on top; the test fills the vault with " + VAULT_INGOTS + " ingots (" + (VAULT_INGOTS * CYCLE) + " ticks of pressing)");
        return SectionsFx.merge(7200, line.notes() + "; bystander saw shafts at z +" + SectionsFx.BYSTANDER_DZ, line, SectionsFx.bystander(h));
    }

    static BlockPos abs(GameTestHelper h, BlockPos rel) {
        return at(h, rel.getX(), rel.getY(), rel.getZ());
    }
}
