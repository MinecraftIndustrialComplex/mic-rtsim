package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.mojang.logging.LogUtils;
import com.seibel.distanthorizons.api.DhApi;
import com.seibel.distanthorizons.api.methods.events.abstractEvents.DhApiBeforeGenericRenderSetupEvent;
import com.seibel.distanthorizons.api.methods.events.abstractEvents.DhApiLevelLoadEvent;
import com.seibel.distanthorizons.api.methods.events.abstractEvents.DhApiLevelUnloadEvent;
import com.seibel.distanthorizons.api.methods.events.sharedParameterObjects.DhApiEventParam;
import com.seibel.distanthorizons.api.methods.events.sharedParameterObjects.DhApiRenderParam;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.client.event.ClientPlayerNetworkEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.RenderFrameEvent;
import net.neoforged.neoforge.client.gui.ConfigurationScreen;
import net.neoforged.neoforge.client.gui.IConfigScreenFactory;
import net.neoforged.neoforge.common.NeoForge;
import org.slf4j.Logger;

/**
 * mic_rtsim_dh: mic-rtsim's far view in Distant Horizons (research/distant-horizons-2026-10-21.md §6.1,
 * DECISIONS.md DH-1 and DH-2). It knows DH and the core's far registry and nothing else: it draws what the
 * registry's sources put there (phase 1: Create trains, from mic_rtsim_create) as DH boxes beyond the vanilla
 * view. Client only: servers never load it.
 */
@Mod(value = MicRtsimDh.MODID, dist = Dist.CLIENT)
public final class MicRtsimDh {
    public static final String MODID = "mic_rtsim_dh";
    public static final Logger LOGGER = LogUtils.getLogger();

    public MicRtsimDh(IEventBus modBus, ModContainer container) {
        DhFarConfig.register(container);
        container.registerExtensionPoint(IConfigScreenFactory.class, ConfigurationScreen::new);

        // DH's events (DhApi.events is ready as soon as DH's classes load)
        DhApi.events.bind(DhApiLevelLoadEvent.class, new DhApiLevelLoadEvent() {
            @Override
            public void onLevelLoad(DhApiEventParam<EventParam> event) {
                if (event.value.levelWrapper.getRenderRegister() != null)
                    FarTrainView.levelChanged();
            }
        });
        DhApi.events.bind(DhApiLevelUnloadEvent.class, new DhApiLevelUnloadEvent() {
            @Override
            public void onLevelUnload(DhApiEventParam<EventParam> event) {
                FarTrainView.levelUnloaded(event.value.levelWrapper.getRenderRegister());
            }
        });
        DhApi.events.bind(DhApiBeforeGenericRenderSetupEvent.class, new DhApiBeforeGenericRenderSetupEvent() {
            @Override
            public void beforeSetup(DhApiEventParam<DhApiRenderParam> event) {
                FarTrainView.beforeSetup(event.value);
            }
        });

        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post e) -> FarGates.tick());
        NeoForge.EVENT_BUS.addListener((RenderFrameEvent.Pre e) -> FarTrainView.newFrame());
        NeoForge.EVENT_BUS.addListener((ClientPlayerNetworkEvent.LoggingOut e) -> FarTrainView.releaseAll());
        if (FarBench.ENABLED)
            FarBench.register();
    }
}
