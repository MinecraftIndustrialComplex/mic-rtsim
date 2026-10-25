package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.client;

import com.minecraftindustrialcomplex.mic_rtsim.client.FarLooks;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim_iceandfire.MicRtsimIceAndFire;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.FMLClientSetupEvent;
import net.neoforged.neoforge.client.event.RenderFrameEvent;
import net.neoforged.neoforge.client.event.RenderNameTagEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.common.util.TriState;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import software.bernie.geckolib.event.GeoRenderEvent;

/**
 * mic_rtsim_iceandfire's client side: far dragons drawn as Ice and Fire's own model (docs/choices-dh-feed.md
 * DHT65), for a far renderer that asks (mic_rtsim_dh), and a new body held back while its animation settles
 * (DHT68: GeckoLib's cancelable render event, on NeoForge's bus, and NeoForge's name tag event). Loaded on clients
 * only; a server never loads a client class.
 */
@Mod(value = MicRtsimIceAndFire.MODID, dist = Dist.CLIENT)
public final class MicRtsimIceAndFireClient {
    public MicRtsimIceAndFireClient(IEventBus modBus) {
        DragonLook look = new DragonLook();
        modBus.addListener(FMLClientSetupEvent.class, e -> e.enqueueWork(() -> FarLooks.register(FarKind.DRAGON, look)));
        // Both sides' joins reach this listener in single player; the arrivals keep only the client's
        NeoForge.EVENT_BUS.addListener(EntityJoinLevelEvent.class, e -> look.arrivals.joined(e.getEntity()));
        NeoForge.EVENT_BUS.addListener(GeoRenderEvent.Entity.Pre.class, e -> {
            if (look.arrivals.cancels(e.getEntity()))
                e.setCanceled(true);
        });
        // A held body's name tag goes with its model (GeckoLib still runs vanilla's render for it after a cancel); last,
        // so a listener that forces name tags on does not bring it back
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, false, RenderNameTagEvent.class, e -> {
            if (look.arrivals.holds(e.getEntity()))
                e.setCanRender(TriState.FALSE);
        });
        NeoForge.EVENT_BUS.addListener(RenderFrameEvent.Post.class, e -> look.arrivals.endFrame());
    }
}
