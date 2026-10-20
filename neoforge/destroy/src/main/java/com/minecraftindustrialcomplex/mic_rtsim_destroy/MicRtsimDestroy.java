package com.minecraftindustrialcomplex.mic_rtsim_destroy;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;

/**
 * mic_rtsim_destroy (A19): Destroy 0.4.1's pollution attributed to districts (I14) and its chunk crude oil
 * as a finite deposit (I11). No block, item or entity of its own (D3).
 */
@Mod(MicRtsimDestroy.MODID)
public final class MicRtsimDestroy {
    public static final String MODID = "mic_rtsim_destroy";

    public MicRtsimDestroy(IEventBus modBus, ModContainer container) {
        IndustryApi.register(new DestroyPlugin());
    }
}
