package com.minecraftindustrialcomplex.mic_rtsim_oreexcavation;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;

/**
 * mic_rtsim_oreexcavation (A19): Create Ore Excavation 1.6.8's veins as finite deposits for the industry
 * engine (I11, F15). No block, item or entity of its own (D3).
 */
@Mod(MicRtsimOreExcavation.MODID)
public final class MicRtsimOreExcavation {
    public static final String MODID = "mic_rtsim_oreexcavation";

    public MicRtsimOreExcavation(IEventBus modBus, ModContainer container) {
        IndustryApi.register(new OreExcavationPlugin());
    }
}
