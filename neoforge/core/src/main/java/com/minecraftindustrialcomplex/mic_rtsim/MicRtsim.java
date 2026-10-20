package com.minecraftindustrialcomplex.mic_rtsim;

import com.minecraftindustrialcomplex.mic_rtsim.bridge.BridgeEvents;
import com.minecraftindustrialcomplex.mic_rtsim.config.RtsimConfig;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.mojang.logging.LogUtils;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.ModContainer;
import net.neoforged.fml.common.Mod;
import org.slf4j.Logger;

/**
 * mic_rtsim: the core bridge between Minecraft and the mic-rtsim engine (PLAN.md §1, §6).
 *
 * <p>It depends on NeoForge only (D4) and is the only Java that talks to the engine. It owns the
 * puppet protocol (puppets are never saved, stale copies are discarded, groups move as one unit), the
 * journal that keeps the sim and the world save consistent, the module switches, and the vanilla body
 * adapters. Until rtsim-jni exists, the engine is the pure-Java {@code StubEngine}.
 */
@Mod(MicRtsim.MODID)
public final class MicRtsim {
    public static final String MODID = "mic_rtsim";
    public static final Logger LOGGER = LogUtils.getLogger();

    public MicRtsim(IEventBus modBus, ModContainer container) {
        RtsimConfig.register(container);
        RtsimAttachments.register(modBus);
        BridgeEvents.register(modBus);
    }
}
