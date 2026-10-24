package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.mojang.blaze3d.shaders.FogShape;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.Tesselator;
import com.mojang.blaze3d.vertex.VertexFormat;
import com.mojang.blaze3d.vertex.VertexSorting;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.client.event.RenderLevelStageEvent;
import org.joml.Matrix4f;
import org.joml.Matrix4fStack;

/**
 * The band renderer (docs/choices-dh-feed.md): far movers inside the chunks the client has loaded that have no
 * entity there, drawn by vanilla's renderer with vanilla's depth, after the entities.
 *
 * <p>That band lies between the server's simulation distance and the client's render distance. Create spawns a
 * carriage's entity only in entity-ticking chunks, so a train there has none, and an engine dragon that is not
 * embodied has none anywhere; vanilla draws over DH in loaded chunks, so DH cannot show them either
 * ({@link FarHide}). DH-2 says the far view shows what the eye would see, so they are drawn here, as the same
 * boxes DH draws ({@link StaircaseCutter}, {@link DragonModel}), flat coloured with a shade per face, lit as DH
 * lights its boxes (vanilla's lightmap at full sky light, {@link VanillaLight}) and faded into vanilla's fog as
 * the terrain around them is.
 *
 * <p>Toward the band's far edge DH draws the mover too ({@link HandOverRule}), and DH's vanilla fade mixes these
 * boxes toward DH's there. So where DH draws it, the band draws DH's own cut ({@link FarTrainView#shownCut},
 * {@link FarDragonView#shown}): the two match box for box and the fade shows one (DHT45).
 *
 * <p>Movers drawn as the real thing ({@link FarModelRenderer}) are drawn by that in the band too, with the same
 * hand-over; these boxes are for the rest. The dragons, models and boxes together, are held to the tier's count
 * ({@link FarDetail#dragons}), nearest first by horizontal distance, as the model renderer holds them
 * (docs/choices-dh-feed.md DHT69): a slow machine on the lowest tier draws 8 at most, however many fly over its
 * loaded chunks. The carriages' boxes are not counted here.
 *
 * <p>Render thread only.
 */
final class FarBandRenderer {
    /** Faces' brightness: top, bottom, north and south, east and west (vanilla's block shading). */
    private static final float TOP = 1.0f, BOTTOM = 0.5f, NS = 0.8f, EW = 0.6f;
    /** The staircase step in the band where DH does not draw the mover too: it is near, so the finest. */
    private static final double STEP = 1.0;
    private static final double[] SEG = new double[6];
    /** This frame's light (red, green, blue) and fog (start, end, shape 0 sphere or 1 cylinder, then its colour). */
    private static final float[] LIGHT = new float[3], FOG = new float[7];

    /** A dragon the band would draw this frame: where, how far (horizontally, squared), its heading, a box or not. */
    private record Dragon(FarMover mover, double x, double y, double z, double d2, double hx, double hz, boolean box) {
    }

    private static final List<Dragon> DRAGONS = new ArrayList<>();

    // What the last frame did, for the log and the bench
    static int lastDrawn, lastBoxes;
    /** Dragons the band counted last frame (models and boxes), and how many the tier's count left out. */
    static int lastDragons, lastDragonsOver;
    static long nanos, frames;
    private static int errors;

    private FarBandRenderer() {}

    static void onStage(RenderLevelStageEvent event) {
        if (event.getStage() != RenderLevelStageEvent.Stage.AFTER_ENTITIES || !FarGates.ready())
            return;
        ClientLevel level = Minecraft.getInstance().level;
        if (level == null)
            return;
        // The stage's own matrices, not whatever RenderSystem holds by now: what the entities left there need not
        // be the level's (a zoomed view drew the band's movers off where they are)
        Matrix4f mv = event.getModelViewMatrix(), proj = event.getProjectionMatrix();
        if (FarBench.ENABLED) {
            mvDrift = Math.max(mvDrift, drift(RenderSystem.getModelViewMatrix(), mv));
            projDrift = Math.max(projDrift, drift(RenderSystem.getProjectionMatrix(), proj));
        }
        Matrix4f savedProj = new Matrix4f(RenderSystem.getProjectionMatrix());
        VertexSorting savedSorting = RenderSystem.getVertexSorting();
        Matrix4fStack stack = RenderSystem.getModelViewStack();
        stack.pushMatrix();
        try {
            stack.set(mv);
            RenderSystem.applyModelViewMatrix();
            RenderSystem.setProjectionMatrix(proj, VertexSorting.DISTANCE_TO_ORIGIN);
            FarModelRenderer.onBand(event, level);
            long t0 = System.nanoTime();
            try {
                draw(event, level);
            } catch (RuntimeException e) {
                if (errors++ < 5)
                    MicRtsimDh.LOGGER.error("mic_rtsim_dh: band renderer failed this frame", e);
            }
            nanos += System.nanoTime() - t0;
            frames++;
        } finally {
            stack.popMatrix();
            RenderSystem.applyModelViewMatrix();
            RenderSystem.setProjectionMatrix(savedProj, savedSorting);
        }
    }

    /** The largest difference between two matrices' elements (the bench's check of what RenderSystem held). */
    private static float drift(Matrix4f a, Matrix4f b) {
        float d = 0;
        for (int c = 0; c < 4; c++)
            for (int r = 0; r < 4; r++)
                d = Math.max(d, Math.abs(a.get(c, r) - b.get(c, r)));
        return d;
    }

    /** Draws {@code cut}'s boxes about (x, y, z), those from {@code wingFrom} on in {@code wingRgb}; returns the buffer. */
    private static BufferBuilder boxes(BufferBuilder buf, List<double[]> cut, double x, double y, double z, Vec3 cam,
            int rgb, int wingFrom, int wingRgb) {
        if (buf == null)
            buf = Tesselator.getInstance().begin(VertexFormat.Mode.QUADS, DefaultVertexFormat.POSITION_COLOR);
        float ox = (float) (x - cam.x), oy = (float) (y - cam.y), oz = (float) (z - cam.z);
        for (int i = 0; i < cut.size(); i++)
            box(buf, cut.get(i), ox, oy, oz, i >= wingFrom ? wingRgb : rgb);
        return buf;
    }

    /** For the bench: how far RenderSystem's matrices were from the stage's at the band, at most. */
    static float mvDrift, projDrift;

    private static void draw(RenderLevelStageEvent event, ClientLevel level) {
        Vec3 cam = event.getCamera().getPosition();
        Frustum frustum = event.getFrustum();
        String dimension = level.dimension().location().toString();
        double clock = FarClock.ticks() + event.getPartialTick().getGameTimeDeltaPartialTick(false);
        FarDetail tier = DhFarConfig.detail();
        BufferBuilder buf = null;
        int drawn = 0, boxes = 0;
        if (FarObjects.movers().isEmpty()) {
            lastDrawn = lastBoxes = lastDragons = lastDragonsOver = 0;
            return;
        }
        Minecraft mc = Minecraft.getInstance();
        VanillaLight.sky(level.getSkyDarken(1.0f), level.getSkyFlashTime() > 0,
                mc.options.gamma().get().floatValue(), LIGHT);
        float[] fogColour = RenderSystem.getShaderFogColor();
        FOG[0] = RenderSystem.getShaderFogStart();
        FOG[1] = RenderSystem.getShaderFogEnd();
        FOG[2] = RenderSystem.getShaderFogShape() == FogShape.SPHERE ? 0 : 1;
        FOG[3] = fogColour[0];
        FOG[4] = fogColour[1];
        FOG[5] = fogColour[2];
        FOG[6] = fogColour[3];
        DRAGONS.clear();
        try {
            for (FarMover m : FarObjects.movers()) {
                if (!dimension.equals(m.dimension()) || m.kind() != FarKind.CARRIAGE && m.kind() != FarKind.DRAGON)
                    continue;
                // Drawn as the real thing by the model renderer, or waiting for it a moment. A dragon model still
                // takes its place in the dragons' count
                FarModelRenderer.Cover c = FarModelRenderer.cover(m);
                if (c == FarModelRenderer.Cover.WAIT || c == FarModelRenderer.Cover.MODEL && m.kind() != FarKind.DRAGON)
                    continue;
                m.segmentAt(clock, SEG);
                // A carriage run longer than any carriage is a bad sample: not drawn (FarTrainView logs it; DHT73)
                if (m.kind() == FarKind.CARRIAGE && FarTrainView.tooLong(SEG))
                    continue;
                double x = (SEG[0] + SEG[3]) / 2.0, y = (SEG[1] + SEG[4]) / 2.0, z = (SEG[2] + SEG[5]) / 2.0;
                if (!FarHide.bandDraws(m, level, x, y, z, cam))
                    continue;
                if (m.kind() == FarKind.DRAGON) {
                    double dx = x - cam.x, dz = z - cam.z;
                    DRAGONS.add(new Dragon(m, x, y, z, dx * dx + dz * dz, SEG[0] - SEG[3], SEG[2] - SEG[5],
                            c == FarModelRenderer.Cover.BOX));
                    continue;
                }
                double rx = SEG[0] - SEG[3], ry = SEG[1] - SEG[4], rz = SEG[2] - SEG[5];
                double reach = FarTrainView.reach(rx, ry, rz);
                if (!frustum.isVisible(new AABB(x - reach, y - 1, z - reach, x + reach,
                        y + FarTrainView.HEIGHT + 1, z + reach)))
                    continue;
                List<double[]> cut = FarTrainView.shownCut(m.key());
                if (cut == null)
                    cut = StaircaseCutter.cut(rx, ry, rz, new StaircaseCutter.Shape(FarTrainView.WIDTH,
                            FarTrainView.HEIGHT, FarTrainView.OVERHANG, STEP));
                buf = boxes(buf, cut, x, y, z, cam, m.rgb(), Integer.MAX_VALUE, m.rgb());
                drawn++;
                boxes += cut.size();
            }
            // The tier's dragons, nearest first by horizontal distance, models and boxes together, as the model
            // renderer counts them in the band (docs/choices-dh-feed.md DHT69); counted before the frustum
            DRAGONS.sort((a, b) -> Double.compare(a.d2, b.d2));
            for (int n = 0; n < DRAGONS.size() && n < tier.dragons; n++) {
                Dragon d = DRAGONS.get(n);
                if (!d.box)
                    continue;
                FarMover m = d.mover;
                double x = d.x, y = d.y, z = d.z, reach = FarDragonView.reach(m.size());
                if (!frustum.isVisible(new AABB(x - reach, y - reach, z - reach, x + reach, y + reach, z + reach)))
                    continue;
                // DH's cut where DH draws it too; the finest, for the heading of the moment, elsewhere
                FarDragonView.Shown dh = FarDragonView.shown(m.key());
                int stage = dh != null ? dh.stage() : Math.max(1, Math.min(5, m.size()));
                int h = dh != null ? dh.heading() : DragonModel.headingStep(d.hx, d.hz);
                double step = dh != null ? dh.step() : STEP;
                List<double[]> cut = new ArrayList<>(DragonModel.body(stage, h, step));
                int rgb = m.rgb(), wingFrom = Integer.MAX_VALUE, wingRgb = rgb;
                DragonModel.Pose pose = DragonModel.poseAt(clock, tier.wingPoses, FarDragonView.flapSeed(m.key()),
                        m.state());
                if (pose != null) {
                    wingFrom = cut.size();
                    wingRgb = FarDragonView.shade(rgb, FarDragonView.WING_SHADE);
                    cut.addAll(DragonModel.wings(stage, h, pose, step));
                }
                buf = boxes(buf, cut, x, y, z, cam, rgb, wingFrom, wingRgb);
                drawn++;
                boxes += cut.size();
            }
            lastDragons = Math.min(DRAGONS.size(), tier.dragons);
            lastDragonsOver = Math.max(0, DRAGONS.size() - tier.dragons);
        } catch (RuntimeException e) {
            DRAGONS.clear();
            // Leave the shared Tesselator as it was found, or the next renderer to begin one draws this garbage
            if (buf != null) {
                MeshData half = buf.build();
                if (half != null)
                    half.close();
            }
            throw e;
        }
        DRAGONS.clear(); // holds no mover past the frame
        lastDrawn = drawn;
        lastBoxes = boxes;
        if (buf == null)
            return;
        MeshData mesh = buf.build();
        if (mesh != null)
            RenderType.debugQuads().draw(mesh);
    }

    /** One box, camera relative: the posestack at this stage is the identity, and the camera turn is vanilla's. */
    private static void box(BufferBuilder buf, double[] b, float ox, float oy, float oz, int rgb) {
        float x0 = (float) b[0] + ox, y0 = (float) b[1] + oy, z0 = (float) b[2] + oz;
        float x1 = (float) b[3] + ox, y1 = (float) b[4] + oy, z1 = (float) b[5] + oz;
        float r = ((rgb >> 16) & 0xFF) * LIGHT[0], g = ((rgb >> 8) & 0xFF) * LIGHT[1], bl = (rgb & 0xFF) * LIGHT[2];
        float f = VanillaLight.fog((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2, FOG[0], FOG[1], FOG[2] == 0)
                * FOG[6];
        quad(buf, r, g, bl, f, TOP, x0, y1, z0, x0, y1, z1, x1, y1, z1, x1, y1, z0);
        quad(buf, r, g, bl, f, BOTTOM, x0, y0, z0, x1, y0, z0, x1, y0, z1, x0, y0, z1);
        quad(buf, r, g, bl, f, NS, x0, y0, z0, x0, y1, z0, x1, y1, z0, x1, y0, z0);
        quad(buf, r, g, bl, f, NS, x0, y0, z1, x1, y0, z1, x1, y1, z1, x0, y1, z1);
        quad(buf, r, g, bl, f, EW, x0, y0, z0, x0, y0, z1, x0, y1, z1, x0, y1, z0);
        quad(buf, r, g, bl, f, EW, x1, y0, z0, x1, y1, z0, x1, y1, z1, x1, y0, z1);
    }

    private static void quad(BufferBuilder buf, float r, float g, float b, float fog, float shade, float ax, float ay,
            float az, float bx, float by, float bz, float cx, float cy, float cz, float dx, float dy, float dz) {
        int sr = Math.round(r * shade + (FOG[3] * 255 - r * shade) * fog);
        int sg = Math.round(g * shade + (FOG[4] * 255 - g * shade) * fog);
        int sb = Math.round(b * shade + (FOG[5] * 255 - b * shade) * fog);
        buf.addVertex(ax, ay, az).setColor(sr, sg, sb, 255);
        buf.addVertex(bx, by, bz).setColor(sr, sg, sb, 255);
        buf.addVertex(cx, cy, cz).setColor(sr, sg, sb, 255);
        buf.addVertex(dx, dy, dz).setColor(sr, sg, sb, 255);
    }
}
