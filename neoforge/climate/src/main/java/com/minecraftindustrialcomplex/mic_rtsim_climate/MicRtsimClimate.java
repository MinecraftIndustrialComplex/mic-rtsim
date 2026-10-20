package com.minecraftindustrialcomplex.mic_rtsim_climate;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;

/**
 * mic_rtsim_climate (A19, choice Q2: named after what it supplies, built around mic_climate): the season
 * calendar and temperatures for the industry engine (I12). No block, item or entity of its own (D3).
 */
@Mod(MicRtsimClimate.MODID)
public final class MicRtsimClimate {
    public static final String MODID = "mic_rtsim_climate";

    public MicRtsimClimate(IEventBus modBus, ModContainer container) {
        IndustryApi.register(new ClimatePlugin());
    }
}
