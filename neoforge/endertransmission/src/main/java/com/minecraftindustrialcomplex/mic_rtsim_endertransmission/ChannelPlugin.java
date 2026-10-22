package com.minecraftindustrialcomplex.mic_rtsim_endertransmission;

import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.entity.BlockEntity;

import java.util.function.Consumer;

/**
 * The industry plugin side of Ender Transmission: transmitters are industrial (members, but wiring: they do
 * not seed a district, DS-10; the machines they feed do), each
 * links to the other endpoints of its channel so they are one district, and the tick keeps the
 * cross-dimension warning ({@link ChannelLinks}).
 */
public final class ChannelPlugin implements IndustryPlugin {

    @Override
    public boolean isIndustrial(BlockEntity be) {
        return ChannelLinks.isTransmitter(be);
    }

    @Override
    public Boolean seed(BlockEntity be) {
        return ChannelLinks.isTransmitter(be) ? Boolean.FALSE : null;
    }

    @Override
    public void links(BlockEntity be, Consumer<BlockPos> out) {
        if (ChannelLinks.isTransmitter(be))
            ChannelLinks.links(be, out);
    }

    @Override
    public void tick(Industry industry, long now) {
        ChannelLinks.tick(industry, now);
    }
}
