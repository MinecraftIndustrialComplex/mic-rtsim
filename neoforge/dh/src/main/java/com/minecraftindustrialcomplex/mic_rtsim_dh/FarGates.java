package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.client.FarFeedClient;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.seibel.distanthorizons.api.DhApi;
import com.seibel.distanthorizons.api.interfaces.config.IDhApiConfig;
import java.util.Locale;
import net.minecraft.client.Minecraft;

/**
 * Whether the far view runs, checked every client tick (research/distant-horizons-2026-10-21.md §3.5, §8).
 *
 * <p>It runs only when DH is up and draws generic objects, this is not a Mac (DH draws no generic objects
 * there), the detail tier is not OFF and the client is in a world. Otherwise it wants nothing from the far
 * registry, so no source asks its mod or the server for anything. It never changes a DH setting; when a
 * setting or the platform stops it, it says so once in the log.
 *
 * <p>While it runs it subscribes to the server's far feed ({@link FarFeedClient}) out to DH's radius, at its
 * tier's object budget, for dragons and carriages; the core sends that only to a server that has the feed.
 */
final class FarGates {
    enum State {
        READY,
        NO_WORLD,
        DH_NOT_READY,
        MAC,
        TIER_OFF,
        DH_RENDERING_OFF,
        GENERIC_RENDERING_OFF
    }

    static final boolean MAC = isMac(System.getProperty("os.name", ""));

    private static State state;
    private static double radiusBlocks;

    private FarGates() {}

    static boolean isMac(String osName) {
        String os = osName.toLowerCase(Locale.ROOT);
        return os.contains("mac") || os.contains("darwin");
    }

    /** Re-checks the gates and tells the far registry how far this renderer wants movers. */
    static void tick() {
        State s;
        double r = 0.0;
        IDhApiConfig cfg = DhApi.Delayed.configs;
        if (MAC)
            s = State.MAC;
        else if (DhFarConfig.detail() == FarDetail.OFF)
            s = State.TIER_OFF;
        else if (cfg == null)
            s = State.DH_NOT_READY;
        else if (!Boolean.TRUE.equals(cfg.graphics().renderingEnabled().getValue()))
            s = State.DH_RENDERING_OFF;
        else if (!Boolean.TRUE.equals(cfg.graphics().genericRendering().renderingEnabled().getValue()))
            s = State.GENERIC_RENDERING_OFF;
        else if (Minecraft.getInstance().level == null)
            s = State.NO_WORLD;
        else {
            s = State.READY;
            Integer chunks = cfg.graphics().chunkRenderDistance().getValue();
            r = (chunks == null ? 0 : chunks) * 16.0;
        }
        if (s != state) {
            log(s);
            state = s;
        }
        radiusBlocks = r;
        FarObjects.want(MicRtsimDh.MODID, s == State.READY ? r : 0.0);
        FarDetail tier = DhFarConfig.detail();
        boolean feed = s == State.READY && !FarBench.NO_FEED;
        FarFeedClient.want(MicRtsimDh.MODID, feed ? (int) Math.ceil(r) : 0, tier.ordinal(), tier.feedObjects,
                FarKind.DRAGON.bit() | FarKind.CARRIAGE.bit());
    }

    private static void log(State s) {
        switch (s) {
            case READY -> MicRtsimDh.LOGGER.info("mic_rtsim_dh: far view on ({} detail)", DhFarConfig.detail());
            case MAC -> MicRtsimDh.LOGGER.info("mic_rtsim_dh: far view off: Distant Horizons draws no generic objects on macOS");
            case TIER_OFF -> MicRtsimDh.LOGGER.info("mic_rtsim_dh: far view off: its detail setting is OFF");
            case DH_RENDERING_OFF -> MicRtsimDh.LOGGER.info("mic_rtsim_dh: far view off: Distant Horizons rendering is off");
            case GENERIC_RENDERING_OFF -> MicRtsimDh.LOGGER.info(
                    "mic_rtsim_dh: far view off: Distant Horizons' generic rendering is off "
                            + "(Advanced > Graphics > Generic Rendering); not changing it");
            default -> {}
        }
    }

    static boolean ready() {
        return state == State.READY;
    }

    static State state() {
        return state;
    }

    /** DH's LOD radius in blocks while ready, else 0. */
    static double radiusBlocks() {
        return radiusBlocks;
    }
}
