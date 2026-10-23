package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.client;

import com.minecraftindustrialcomplex.mic_rtsim.client.FarLooks;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.FMLClientSetupEvent;

/**
 * mic_rtsim_iceandfire's client side: far dragons drawn as Ice and Fire's own model (docs/choices-dh-feed.md
 * DHT65), for a far renderer that asks (mic_rtsim_dh). Loaded on clients only; a server never loads a client
 * class.
 */
@Mod(value = MicRtsimIceAndFire.MODID, dist = Dist.CLIENT)
public final class MicRtsimIceAndFireClient {
    public MicRtsimIceAndFireClient(IEventBus modBus) {
        modBus.addListener(FMLClientSetupEvent.class,
                e -> e.enqueueWork(() -> FarLooks.register(FarKind.DRAGON, new DragonLook())));
    }
}
