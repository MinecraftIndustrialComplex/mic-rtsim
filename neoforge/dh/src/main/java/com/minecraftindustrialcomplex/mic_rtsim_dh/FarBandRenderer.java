package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.Tesselator;
import com.mojang.blaze3d.vertex.VertexFormat;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.client.event.RenderLevelStageEvent;

/**
 * The band renderer (docs/choices-dh-feed.md): far movers inside the chunks the client has loaded that have no
 * entity there, drawn by vanilla's renderer with vanilla's depth, after the entities.
 *
 * <p>That band lies between the server's simulation distance and the client's render distance. Create spawns a
 * carriage's entity only in entity-ticking chunks, so a train there has none, and an engine dragon that is not
 * embodied has none anywhere; vanilla draws over DH in loaded chunks, so DH cannot show them either
 * ({@link FarHide}). DH-2 says the far view shows what the eye would see, so they are drawn here, as the same
 * boxes DH draws ({@link StaircaseCutter}, {@link DragonModel}), flat coloured with a shade per face.
 *
 * <p>Render thread only.
 */
final class FarBandRenderer {
    /** Faces' brightness: top, bottom, north and south, east and west (vanilla's block shading). */
    private static final float TOP = 1.0f, BOTTOM = 0.5f, NS = 0.8f, EW = 0.6f;
    /** The staircase step in the band: it is near, so the finest. */
    private static final double STEP = 1.0;
    private static final double[] SEG = new double[6];

    // What the last frame did, for the log and the bench
    static int lastDrawn, lastBoxes;
    static long nanos, frames;
    private static int errors;

    private FarBandRenderer() {}

    static void onStage(RenderLevelStageEvent event) {
        if (event.getStage() != RenderLevelStageEvent.Stage.AFTER_ENTITIES || !FarGates.ready())
            return;
        ClientLevel level = Minecraft.getInstance().level;
        if (level == null)
            return;
        long t0 = System.nanoTime();
        try {
            draw(event, level);
        } catch (RuntimeException e) {
            if (errors++ < 5)
                MicRtsimDh.LOGGER.error("mic_rtsim_dh: band renderer failed this frame", e);
        }
        nanos += System.nanoTime() - t0;
        frames++;
    }

    private static void draw(RenderLevelStageEvent event, ClientLevel level) {
        Vec3 cam = event.getCamera().getPosition();
        Frustum frustum = event.getFrustum();
        String dimension = level.dimension().location().toString();
        double clock = FarClock.ticks() + event.getPartialTick().getGameTimeDeltaPartialTick(false);
        FarDetail tier = DhFarConfig.detail();
        BufferBuilder buf = null;
        int drawn = 0, boxes = 0;
        for (FarMover m : FarObjects.movers()) {
            if (!dimension.equals(m.dimension()))
                continue;
            m.segmentAt(clock, SEG);
            double x = (SEG[0] + SEG[3]) / 2.0, y = (SEG[1] + SEG[4]) / 2.0, z = (SEG[2] + SEG[5]) / 2.0;
            if (!FarHide.loaded(level, x, z) || FarHide.entityPresent(m, level, x, y, z))
                continue;
            List<double[]> cut;
            int rgb = m.rgb(), wingFrom = Integer.MAX_VALUE, wingRgb = rgb;
            double reach;
            if (m.kind() == FarKind.CARRIAGE) {
                double rx = SEG[0] - SEG[3], ry = SEG[1] - SEG[4], rz = SEG[2] - SEG[5];
                reach = Math.sqrt(rx * rx + ry * ry + rz * rz) / 2.0 + FarTrainView.OVERHANG + FarTrainView.WIDTH;
                if (!frustum.isVisible(new AABB(x - reach, y - 1, z - reach, x + reach, y + FarTrainView.HEIGHT + 1,
                        z + reach)))
                    continue;
                cut = StaircaseCutter.cut(rx, ry, rz, new StaircaseCutter.Shape(FarTrainView.WIDTH,
                        FarTrainView.HEIGHT, FarTrainView.OVERHANG, STEP));
            } else if (m.kind() == FarKind.DRAGON) {
                int stage = Math.max(1, Math.min(5, m.size()));
                reach = DragonModel.length(stage) * DragonModel.WINGSPAN;
                if (!frustum.isVisible(new AABB(x - reach, y - reach, z - reach, x + reach, y + reach, z + reach)))
                    continue;
                int h = DragonModel.headingStep(SEG[0] - SEG[3], SEG[2] - SEG[5]);
                cut = new ArrayList<>(DragonModel.body(stage, h, STEP));
                DragonModel.Pose pose = DragonModel.poseAt(clock, tier.wingPoses, FarDragonView.flapSeed(m.key()),
                        m.state());
                if (pose != null) {
                    wingFrom = cut.size();
                    wingRgb = FarDragonView.shade(rgb, FarDragonView.WING_SHADE);
                    cut.addAll(DragonModel.wings(stage, h, pose, STEP));
                }
            } else {
                continue;
            }
            if (buf == null)
                buf = Tesselator.getInstance().begin(VertexFormat.Mode.QUADS, DefaultVertexFormat.POSITION_COLOR);
            float ox = (float) (x - cam.x), oy = (float) (y - cam.y), oz = (float) (z - cam.z);
            for (int i = 0; i < cut.size(); i++)
                box(buf, cut.get(i), ox, oy, oz, i >= wingFrom ? wingRgb : rgb);
            drawn++;
            boxes += cut.size();
        }
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
        int r = (rgb >> 16) & 0xFF, g = (rgb >> 8) & 0xFF, bl = rgb & 0xFF;
        quad(buf, r, g, bl, TOP, x0, y1, z0, x0, y1, z1, x1, y1, z1, x1, y1, z0);
        quad(buf, r, g, bl, BOTTOM, x0, y0, z0, x1, y0, z0, x1, y0, z1, x0, y0, z1);
        quad(buf, r, g, bl, NS, x0, y0, z0, x0, y1, z0, x1, y1, z0, x1, y0, z0);
        quad(buf, r, g, bl, NS, x0, y0, z1, x1, y0, z1, x1, y1, z1, x0, y1, z1);
        quad(buf, r, g, bl, EW, x0, y0, z0, x0, y0, z1, x0, y1, z1, x0, y1, z0);
        quad(buf, r, g, bl, EW, x1, y0, z0, x1, y1, z0, x1, y1, z1, x1, y0, z1);
    }

    private static void quad(BufferBuilder buf, int r, int g, int b, float shade, float ax, float ay, float az,
            float bx, float by, float bz, float cx, float cy, float cz, float dx, float dy, float dz) {
        int sr = Math.round(r * shade), sg = Math.round(g * shade), sb = Math.round(b * shade);
        buf.addVertex(ax, ay, az).setColor(sr, sg, sb, 255);
        buf.addVertex(bx, by, bz).setColor(sr, sg, sb, 255);
        buf.addVertex(cx, cy, cz).setColor(sr, sg, sb, 255);
        buf.addVertex(dx, dy, dz).setColor(sr, sg, sb, 255);
    }
}
