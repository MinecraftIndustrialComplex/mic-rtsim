package com.minecraftindustrialcomplex.mic_rtsim_endertransmission;

import com.forsteri.createendertransmission.blocks.AbstractMatterTransmitterBlockEntity;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import net.minecraft.world.level.block.entity.BlockEntity;

/**
 * The industry plugin side of Ender Transmission: transmitters are industrial (they seed districts), and
 * the tick flushes channel ports and sends channel samples ({@link ChannelWatch}).
 */
public final class ChannelPlugin implements IndustryPlugin {

    @Override
    public boolean isIndustrial(BlockEntity be) {
        return be instanceof AbstractMatterTransmitterBlockEntity<?>;
    }

    @Override
    public void tick(Industry industry, long now) {
        ChannelWatch.tick(industry, now);
    }
}
