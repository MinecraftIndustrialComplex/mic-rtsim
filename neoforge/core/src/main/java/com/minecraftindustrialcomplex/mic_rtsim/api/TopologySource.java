package com.minecraftindustrialcomplex.mic_rtsim.api;

import com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology;
import net.minecraft.server.level.ServerLevel;

/**
 * Something that knows how a level's edges are joined better than the core can see for itself: Deep
 * Time's layout (its Petroff–Guyou world, glued east and west by Toroidal World and north and south by
 * its wrap engine) or another world mod's. The bridge asks the registered sources for every level,
 * first answer wins, then Toroidal World's own shape, then the vanilla world border (TP18). Register
 * with {@link RtsimApi#registerTopology} during mod construction; the core registers Deep Time's when
 * Deep Time is loaded.
 *
 * <p>When a level's topology changes in play (Deep Time's wrap engine installing a layout ends its
 * edge_stop border), call {@link RtsimApi#topologyChanged}; the bridge also looks again every few
 * seconds.
 */
public interface TopologySource {

    /** A short name for logs. */
    String name();

    /** The level's topology, or null when this source does not describe it. Server thread. */
    LevelTopology topologyOf(ServerLevel level);
}
