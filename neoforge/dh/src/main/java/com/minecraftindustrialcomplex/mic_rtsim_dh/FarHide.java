package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.SectionPos;
import net.minecraft.world.entity.Entity;

/**
 * Who draws a far mover, if anyone (research/distant-horizons-2026-10-21.md §5; docs/choices-dh-feed.md).
 *
 * <ul>
 *   <li><b>The real thing is on the client</b> ({@link #entityPresent}): nobody; vanilla draws the entity.</li>
 *   <li><b>Its midpoint is in a chunk the client has loaded</b> ({@link #loaded}): the band renderer
 *       ({@link FarBandRenderer}), in vanilla's own pass with vanilla's depth, because vanilla draws over DH in
 *       loaded chunks. That is the band between the server's simulation distance and the client's render
 *       distance, where a train or a dragon has no entity yet.</li>
 *   <li><b>Anywhere else in DH's radius</b>: DH ({@link FarTrainView}, {@link FarDragonView}).</li>
 * </ul>
 */
final class FarHide {
    /** An entity the server named counts only this near the mover's midpoint: an id is not a promise. */
    static final double ENTITY_NEAR = 64.0;

    private FarHide() {}

    /**
     * True when the client has the real thing: its source said so (Create's map), or the client's level has
     * the entity the server named for it, near where the mover is.
     */
    static boolean entityPresent(FarMover m, ClientLevel level, double x, double y, double z) {
        if (m.real())
            return true;
        if (m.entity() < 0)
            return false;
        Entity e = level.getEntity(m.entity());
        return e != null && !e.isRemoved() && e.distanceToSqr(x, y, z) < ENTITY_NEAR * ENTITY_NEAR;
    }

    /** True when the client has loaded the chunk at (x, z). */
    static boolean loaded(ClientLevel level, double x, double z) {
        return level.getChunkSource().hasChunk(SectionPos.blockToSectionCoord(x), SectionPos.blockToSectionCoord(z));
    }
}
