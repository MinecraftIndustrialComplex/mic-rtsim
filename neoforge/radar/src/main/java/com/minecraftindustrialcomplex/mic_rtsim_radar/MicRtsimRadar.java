package com.minecraftindustrialcomplex.mic_rtsim_radar;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;

/**
 * mic_rtsim_radar (A19, choice Q2): Create Radars 0.4.9.4 driving Create Big Cannons 5.11.7 mounts, as the
 * auto-turrets of the defence survey (I21). No block, item or entity of its own (D3).
 */
@Mod(MicRtsimRadar.MODID)
public final class MicRtsimRadar {
    public static final String MODID = "mic_rtsim_radar";

    public MicRtsimRadar(IEventBus modBus, ModContainer container) {
        IndustryApi.register(new RadarPlugin());
    }
}
