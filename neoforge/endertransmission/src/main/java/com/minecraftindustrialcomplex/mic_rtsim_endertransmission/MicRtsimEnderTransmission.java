package com.minecraftindustrialcomplex.mic_rtsim_endertransmission;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;

/**
 * mic_rtsim_endertransmission (A19): Create Ender Transmission 2.1.1's channel buffers as shared stock
 * (I13, F8): IND_CHANNEL_SAMPLE, channel ports, and IND_CHANNEL_DELTA applied. No block, item or entity
 * of its own (D3).
 */
@Mod(MicRtsimEnderTransmission.MODID)
public final class MicRtsimEnderTransmission {
    public static final String MODID = "mic_rtsim_endertransmission";

    public MicRtsimEnderTransmission(IEventBus modBus, ModContainer container) {
        IndustryApi.register(new ChannelPlugin());
        IndustryApi.registerCommand(IndRecords.IND_CHANNEL_DELTA, ChannelWatch::channelDelta);
    }
}
