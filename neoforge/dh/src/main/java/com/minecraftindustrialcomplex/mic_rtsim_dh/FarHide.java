package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.SectionPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;

/**
 * Who draws a far mover, if anyone (research/distant-horizons-2026-10-21.md §5; docs/choices-dh-feed.md DHT45,
 * DHT47): the client's side of {@link HandOverRule}.
 *
 * <ul>
 *   <li><b>The real thing is on the client</b> ({@link #entityPresent}): nobody; vanilla draws the entity. A
 *       dragon's body that reaches toward DH's vanilla fade is also drawn in DH's pass ({@link #bodyCopied}), so
 *       the fade has the body to mix it toward.</li>
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
        if (entityPresent(m, level, x, y, z, cam))
            return false;
        if (!loaded(level, x, z))
            return true;
        double dx = x - cam.x, dy = y - cam.y, dz = z - cam.z;
        return HandOverRule.dh(false, true, Math.sqrt(dx * dx + dy * dy + dz * dz), reach, FadeRing.start());
    }

    /** True when the band renderer draws the mover whose midpoint is (x, y, z). */
    static boolean bandDraws(FarMover m, ClientLevel level, double x, double y, double z, Vec3 cam) {
        boolean loaded = loaded(level, x, z);
        // Out of the loaded chunks the band has nothing to draw, entity or not
        return HandOverRule.band(loaded && entityPresent(m, level, x, y, z, cam), loaded);
    }

    /**
     * True when the client has the real thing: its source said so (Create's map, or the server's REAL record,
     * sent as the entity joins its level), or the client's level has the entity the server named for it, near
     * where the mover is, and, for a dragon, vanilla draws its body ({@link #bodyDrawn}).
     */
    static boolean entityPresent(FarMover m, ClientLevel level, double x, double y, double z, Vec3 cam) {
        if (m.real())
            return true;
        Entity e = named(m, level, x, y, z);
        if (e == null)
            return false;
        return m.kind() != FarKind.DRAGON || bodyDrawn(e, cam.x, cam.y, cam.z);
    }

    /**
     * Whether vanilla draws a dragon's body this frame, the camera at (cx, cy, cz), once the body is near its
     * mover (docs/choices-dh-feed.md DHT46): vanilla's distance rule ({@link Entity#shouldRender}: the box's
     * mean edge times 64 times vanilla's view scale, render distance over 8 clamped to 1 to 2.5 times the entity
     * distance setting). Ice and Fire sets a dragon's size only in its tick, so a new body keeps its type's 0.78
     * by 1.2 block box until its first client tick, and vanilla leaves it out beyond about 74 blocks at render
     * distance 10, 88 at 12: hidden for its entity then, the far box left a gap of a frame or two.
     *
     * <p>Not for a carriage: Create hands its carriages to Flywheel, which draws them with no distance rule
     * while a new carriage entity's box is Create's 1 by 1 by 1 until its first client tick, and past
     * vanilla's own entity pass. Render thread only.
     */
    private static boolean bodyDrawn(Entity e, double cx, double cy, double cz) {
        return e.shouldRender(cx, cy, cz);
    }

    /**
     * The body of dragon {@code m}, whose midpoint is drawn at (x, y, z), when DH's pass draws a copy of it
     * (docs/choices-dh-feed.md DHT47), else null: vanilla draws the body ({@link #entityPresent}) and
     * {@link HandOverRule#bodyCopied} holds for it, by its own place, with the box's reach. Vanilla still draws
     * the body; the copy, drawn where it is and as it is ({@link FarModelRenderer}), is what DH's vanilla fade
     * mixes its pixels toward, so the body keeps its colour across the ring. Render thread only.
     */
    static Entity bodyCopied(FarMover m, ClientLevel level, double x, double y, double z, Vec3 cam) {
        if (m.kind() != FarKind.DRAGON)
            return null;
        Entity e = named(m, level, x, y, z);
        if (e == null || !bodyDrawn(e, cam.x, cam.y, cam.z))
            return null;
        double dx = e.getX() - cam.x, dy = e.getY() - cam.y, dz = e.getZ() - cam.z;
        return HandOverRule.bodyCopied(loaded(level, e.getX(), e.getZ()), Math.sqrt(dx * dx + dy * dy + dz * dz),
                FarDragonView.reach(m.size()), FadeRing.start()) ? e : null;
    }

    /** The entity the server named for {@code m}, when the client has it near where the mover is drawn. */
    private static Entity named(FarMover m, ClientLevel level, double x, double y, double z) {
        if (m.entity() < 0)
            return null;
        Entity e = level.getEntity(m.entity());
        if (e == null || e.isRemoved() || e.distanceToSqr(x, y, z) >= ENTITY_NEAR * ENTITY_NEAR)
            return null;
        return e;
    }

    /** True when the client has loaded the chunk at (x, z). */
    static boolean loaded(ClientLevel level, double x, double z) {
        return level.getChunkSource().hasChunk(SectionPos.blockToSectionCoord(x), SectionPos.blockToSectionCoord(z));
    }
}
