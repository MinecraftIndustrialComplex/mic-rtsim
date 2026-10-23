package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import net.minecraft.client.Minecraft;
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
 *       dragon's body counts only where vanilla draws it short of DH's vanilla fade; past it vanilla leaves the
 *       body out ({@link #bodyHidden}) and the box is drawn as below.</li>
 *   <li><b>Its midpoint is in a chunk the client has loaded</b> ({@link #loaded}): the band renderer
 *       ({@link FarBandRenderer}), in vanilla's own pass with vanilla's depth, because vanilla draws over DH in
 *       loaded chunks. That is the band between the server's simulation distance and the client's render
 *       distance, where a train or a dragon has no entity yet. Toward the band's far edge DH draws it as well
 *       ({@link #dhDraws}), so DH's vanilla fade has it to fade into.</li>
 *   <li><b>Anywhere else in DH's radius</b>: DH ({@link FarTrainView}, {@link FarDragonView}).</li>
 * </ul>
 */
public final class FarHide {
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
        if (m.entity() < 0)
            return false;
        Entity e = level.getEntity(m.entity());
        if (e == null || e.isRemoved() || e.distanceToSqr(x, y, z) >= ENTITY_NEAR * ENTITY_NEAR)
            return false;
        return m.kind() != FarKind.DRAGON || bodyDrawn(m, e, cam.x, cam.y, cam.z);
    }

    /**
     * Whether vanilla draws a dragon's body this frame, the camera at (cx, cy, cz), once the body is near its
     * mover (docs/choices-dh-feed.md DHT46, DHT47):
     *
     * <ul>
     *   <li><b>Vanilla's distance rule</b> ({@link Entity#shouldRender}: the box's mean edge times 64 times
     *       vanilla's view scale, render distance over 8 clamped to 1 to 2.5 times the entity distance setting).
     *       Ice and Fire sets a dragon's size only in its tick, so a new body keeps its type's 0.78 by 1.2 block
     *       box until its first client tick, and vanilla leaves it out beyond about 74 blocks at render distance
     *       10, 88 at 12: hidden for its entity then, the far box left a gap of a frame or two.</li>
     *   <li><b>Short of DH's vanilla fade</b> ({@link HandOverRule#bodyShown}). Past it {@link #bodyHidden}
     *       leaves the body out and the far box stays, so the fade has the box to mix it toward.</li>
     * </ul>
     *
     * <p>Not for a carriage: Create hands its carriages to Flywheel, which draws them with no distance rule
     * while a new carriage entity's box is Create's 1 by 1 by 1 until its first client tick, and past
     * vanilla's own entity pass. Render thread only.
     */
    private static boolean bodyDrawn(FarMover m, Entity e, double cx, double cy, double cz) {
        return e.shouldRender(cx, cy, cz) && shortOfFade(m, e, cx, cy, cz);
    }

    /** {@link HandOverRule#bodyShown} for the body of dragon {@code m}, by the body's own position. */
    private static boolean shortOfFade(FarMover m, Entity e, double cx, double cy, double cz) {
        double dx = e.getX() - cx, dy = e.getY() - cy, dz = e.getZ() - cz;
        return HandOverRule.bodyShown(Math.sqrt(dx * dx + dy * dy + dz * dz), FarDragonView.reach(m.size()),
                FadeRing.start());
    }

    /** The dragons by their client entity id, at {@link FarObjects#version()} {@code byIdVersion}. */
    private static final Int2ObjectOpenHashMap<FarMover> DRAGON_BY_ID = new Int2ObjectOpenHashMap<>();
    private static long byIdVersion = Long.MIN_VALUE;
    private static final double[] SEG = new double[6];

    /**
     * True when vanilla leaves out entity {@code e} this frame, the camera at (cx, cy, cz): a dragon a far mover
     * names, near the mover, that vanilla would draw by its distance rule, at or past where DH's fade starts
     * (docs/choices-dh-feed.md DHT47). The far box is drawn instead, as {@link #entityPresent} is false for it
     * then. Called for every entity vanilla thinks of drawing, at the head of
     * {@code EntityRenderDispatcher.shouldRender}: one map lookup when no far dragon names it. Render thread only.
     */
    public static boolean bodyHidden(Entity e, double cx, double cy, double cz) {
        long v = FarObjects.version();
        if (v != byIdVersion) {
            byIdVersion = v;
            DRAGON_BY_ID.clear();
            for (FarMover m : FarObjects.movers())
                if (m.kind() == FarKind.DRAGON && m.entity() >= 0 && !m.real())
                    DRAGON_BY_ID.put(m.entity(), m);
        }
        if (DRAGON_BY_ID.isEmpty())
            return false;
        FarMover m = DRAGON_BY_ID.get(e.getId());
        if (m == null || e.isRemoved() || !m.dimension().equals(e.level().dimension().location().toString()))
            return false;
        float partial = Minecraft.getInstance().getTimer().getGameTimeDeltaPartialTick(false);
        m.segmentAt(FarClock.ticks() + partial, SEG);
        double x = (SEG[0] + SEG[3]) / 2.0, y = (SEG[1] + SEG[4]) / 2.0, z = (SEG[2] + SEG[5]) / 2.0;
        if (e.distanceToSqr(x, y, z) >= ENTITY_NEAR * ENTITY_NEAR || !e.shouldRender(cx, cy, cz))
            return false;
        return !shortOfFade(m, e, cx, cy, cz);
    }

    /** True when the client has loaded the chunk at (x, z). */
    static boolean loaded(ClientLevel level, double x, double z) {
        return level.getChunkSource().hasChunk(SectionPos.blockToSectionCoord(x), SectionPos.blockToSectionCoord(z));
    }
}
