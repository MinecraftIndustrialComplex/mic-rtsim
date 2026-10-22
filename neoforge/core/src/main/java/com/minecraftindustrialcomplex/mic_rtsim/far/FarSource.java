package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.util.function.Consumer;
import net.minecraft.server.level.ServerLevel;

/**
 * Something the far feed draws for distant players (docs/boundary.md §12.3): Create's trains
 * (mic_rtsim_create), dragons in the game (mic_rtsim_iceandfire), the engine's dragon legs (the core's
 * {@link FarLegs}). Registered with {@code RtsimApi.registerFarSource}.
 */
public interface FarSource {
    /** For the log. */
    String name();

    /**
     * Adds what moves in {@code level} now. The feed calls it once a second for each level a subscriber is
     * in, on the server thread; {@code gameTime} is the overworld's (the engine's clock).
     */
    void collect(ServerLevel level, long gameTime, Consumer<FarItem> out);
}
