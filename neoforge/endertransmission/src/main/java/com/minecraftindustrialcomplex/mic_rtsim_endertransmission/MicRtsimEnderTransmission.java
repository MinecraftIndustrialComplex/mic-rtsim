package com.minecraftindustrialcomplex.mic_rtsim_endertransmission;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;

/**
 * mic_rtsim_endertransmission (A19): Create Ender Transmission 2.1.1's channels join their endpoints into
 * one district (I13 as corrected: a stop-gap for tightly linked machinery, not long-distance
 * transmission). IND_CHANNEL_DELTA is not applied (no channel is shared stock), so the core answers it
 * FAILED. No block, item or entity of its own (D3).
 */
@Mod(MicRtsimEnderTransmission.MODID)
public final class MicRtsimEnderTransmission {
    public static final String MODID = "mic_rtsim_endertransmission";

    public MicRtsimEnderTransmission(IEventBus modBus, ModContainer container) {
        IndustryApi.register(new ChannelPlugin());
    }
}
