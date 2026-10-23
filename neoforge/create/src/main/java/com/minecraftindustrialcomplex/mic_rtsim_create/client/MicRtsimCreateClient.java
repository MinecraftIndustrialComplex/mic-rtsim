package com.minecraftindustrialcomplex.mic_rtsim_create.client;

import com.minecraftindustrialcomplex.mic_rtsim.client.FarLooks;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.bench.FarTrainBench;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.FMLClientSetupEvent;
import net.neoforged.neoforge.common.NeoForge;

/**
 * mic_rtsim_create's client side: the Create train source of the far view
 * (research/distant-horizons-2026-10-21.md §6.1, §6.2), and far carriages drawn as the blocks their players
 * built, for a far renderer that asks (docs/choices-dh-feed.md DHT64). Loaded on clients only; the plugin's dependency lines
 * stay BOTH, and a server never loads a client class.
 */
@Mod(value = MicRtsimCreate.MODID, dist = Dist.CLIENT)
public final class MicRtsimCreateClient {
    public MicRtsimCreateClient(IEventBus modBus) {
        modBus.addListener(FMLClientSetupEvent.class,
                e -> e.enqueueWork(() -> FarLooks.register(FarKind.CARRIAGE, new CarriageLook())));
        NeoForge.EVENT_BUS.addListener(TrainFarSource::onClientTick);
        NeoForge.EVENT_BUS.addListener(TrainFarSource::onJoin);
        NeoForge.EVENT_BUS.addListener(TrainFarSource::onLeave);
        NeoForge.EVENT_BUS.addListener(TrainFarSource::onLoggingOut);
        if (FarTrainBench.ENABLED) {
            NeoForge.EVENT_BUS.addListener(TrainSyncMeter::onClientTick);
            NeoForge.EVENT_BUS.addListener(CarriageProbe::onClientTick);
        }
    }
}
