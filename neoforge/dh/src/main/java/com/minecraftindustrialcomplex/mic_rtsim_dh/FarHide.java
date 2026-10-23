package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.SectionPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

/**
 * Who draws a far mover, if anyone (research/distant-horizons-2026-10-21.md §5; docs/choices-dh-feed.md DHT45):
 * the client's side of {@link HandOverRule}.
 *
 * <ul>
 *   <li><b>The real thing is on the client</b> and vanilla draws it from here ({@link #entityPresent}): nobody;
 *       vanilla draws the entity.</li>
 *   <li><b>Its midpoint is in a chunk the client has loaded</b> ({@link #loaded}): the band renderer
 *       ({@link FarBandRenderer}), in vanilla's own pass with vanilla's depth, because vanilla draws over DH in
 *       loaded chunks. That is the band between the server's simulation distance and the client's render
 *       distance, where a train or a dragon has no entity yet. Toward the band's far edge DH draws it as well
 *       ({@link #dhDraws}), so DH's vanilla fade has it to fade into.</li>
 *   <li><b>Anywhere else in DH's radius</b>: DH ({@link FarTrainView}, {@link FarDragonView}).</li>
 * </ul>
 */
final class FarHide {
    /** An entity the server named counts only this near the mover's midpoint: an id is not a promise. */
    static final double ENTITY_NEAR = 64.0;

    private FarHide() {}

    /** True when DH draws the mover whose midpoint is (x, y, z) and farthest point {@code reach} from it. */
    static boolean dhDraws(FarMover m, ClientLevel level, double x, double y, double z, double reach, Vec3 cam) {
        if (entityPresent(m, level, x, y, z))
            return false;
        if (!loaded(level, x, z))
            return true;
        double dx = x - cam.x, dy = y - cam.y, dz = z - cam.z;
        return HandOverRule.dh(false, true, Math.sqrt(dx * dx + dy * dy + dz * dz), reach, FadeRing.start());
    }

    /** True when the band renderer draws the mover whose midpoint is (x, y, z). */
    static boolean bandDraws(FarMover m, ClientLevel level, double x, double y, double z) {
        boolean loaded = loaded(level, x, z);
        // Out of the loaded chunks the band has nothing to draw, entity or not
        return HandOverRule.band(loaded && entityPresent(m, level, x, y, z), loaded);
    }

    /**
     * True when the client has the real thing: its source said so (Create's map, or the server's REAL record,
     * sent as the entity joins its level), or the client's level has the entity the server named for it, near
     * where the mover is, and vanilla would draw it from the camera (docs/choices-dh-feed.md DHT46).
     *
     * <p>The last is vanilla's own distance rule ({@link Entity#shouldRender}: the entity's box size times 64
     * times the entity distance setting), which it checks before anything else it draws an entity by. Ice and
     * Fire sets a dragon's size only in its tick, so a new body keeps its type's 0.78 by 1.2 block box until its
     * first client tick, and vanilla leaves it out beyond about 59 blocks at the default entity distance: hidden
     * for its entity then, the far box left a gap of a frame or two. Render thread only.
     */
    static boolean entityPresent(FarMover m, ClientLevel level, double x, double y, double z) {
        if (m.real())
            return true;
        if (m.entity() < 0)
            return false;
        Entity e = level.getEntity(m.entity());
        if (e == null || e.isRemoved() || e.distanceToSqr(x, y, z) >= ENTITY_NEAR * ENTITY_NEAR)
            return false;
        Vec3 cam = Minecraft.getInstance().gameRenderer.getMainCamera().getPosition();
        return e.shouldRender(cam.x, cam.y, cam.z);
    }

    /** True when the client has loaded the chunk at (x, z). */
    static boolean loaded(ClientLevel level, double x, double z) {
        return level.getChunkSource().hasChunk(SectionPos.blockToSectionCoord(x), SectionPos.blockToSectionCoord(z));
    }
}
