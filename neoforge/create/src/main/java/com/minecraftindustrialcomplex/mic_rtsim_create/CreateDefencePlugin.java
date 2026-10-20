package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.simibubi.create.content.kinetics.crusher.CrushingWheelControllerBlockEntity;
import com.simibubi.create.content.kinetics.drill.DrillBlockEntity;
import com.simibubi.create.content.kinetics.saw.SawBlockEntity;
import net.minecraft.world.level.block.entity.BlockEntity;

/**
 * Create for the threat side (overnight agent 14): the kill machines of the defence survey (I21,
 * INDUSTRY.md §13.5: {@code SawBlock}, {@code DrillBlock} and {@code CrushingWheelControllerBlockEntity}
 * hurt entities, verified in INDUSTRY.md Appendix A) and the gauge network warning (I16, {@link GaugeNetworks}).
 * Armed deployers are not counted: whether a deployer hurts depends on its held item and mode, which is
 * UNVERIFIED (choice Q15).
 */
public final class CreateDefencePlugin implements IndustryPlugin {

    @Override
    public boolean killMachine(BlockEntity be) {
        return be instanceof SawBlockEntity || be instanceof DrillBlockEntity || be instanceof CrushingWheelControllerBlockEntity;
    }

    @Override
    public void tick(Industry industry, long now) {
        GaugeNetworks.tick(industry, now);
    }
}
