package com.minecraftindustrialcomplex.mic_rtsim;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import net.minecraft.client.Minecraft;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.client.event.ClientPlayerNetworkEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.common.NeoForge;

/**
 * mic_rtsim's client side: the far-object registry's clock, its reset on leaving a world
 * (research/distant-horizons-2026-10-21.md §6.1), and the far feed's client end ({@code client.FarFeedClient},
 * §6.3). Loaded on clients only, so a dedicated server never sees a client class.
 */
@Mod(value = MicRtsim.MODID, dist = Dist.CLIENT)
public final class MicRtsimClient {
    public MicRtsimClient() {
        com.minecraftindustrialcomplex.mic_rtsim.client.FarFeedClient.register();
        NeoForge.EVENT_BUS.addListener(MicRtsimClient::onClientTick);
        NeoForge.EVENT_BUS.addListener(MicRtsimClient::onLoggingOut);
    }

    private static void onClientTick(ClientTickEvent.Post event) {
        // Paused single player stops the far movers with the world (Create's map clock pauses the same way)
        if (!Minecraft.getInstance().isPaused())
            FarClock.advance();
        com.minecraftindustrialcomplex.mic_rtsim.client.FarFeedClient.tick();
    }

    private static void onLoggingOut(ClientPlayerNetworkEvent.LoggingOut event) {
        com.minecraftindustrialcomplex.mic_rtsim.client.FarFeedClient.loggingOut();
        FarObjects.clearAll();
        // The far looks' meshes (DHT63): GPU buffers, freed here on the render thread
        com.minecraftindustrialcomplex.mic_rtsim.client.FarLooks.clearAll();
    }
}
