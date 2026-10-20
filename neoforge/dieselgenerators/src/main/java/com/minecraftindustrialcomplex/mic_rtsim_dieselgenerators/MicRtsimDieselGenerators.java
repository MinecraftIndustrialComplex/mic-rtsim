package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;

/**
 * mic_rtsim_dieselgenerators (A19): Create Diesel Generators 1.3.15's chunk oil as a finite deposit for
 * the industry engine (I11, F15). No block, item or entity of its own (D3).
 */
@Mod(MicRtsimDieselGenerators.MODID)
public final class MicRtsimDieselGenerators {
    public static final String MODID = "mic_rtsim_dieselgenerators";

    public MicRtsimDieselGenerators(IEventBus modBus, ModContainer container) {
        IndustryApi.register(new DieselOilPlugin());
    }
}
