package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.client.FarLook;
import com.minecraftindustrialcomplex.mic_rtsim.client.FarLooks;
import com.minecraftindustrialcomplex.mic_rtsim.client.FarModel;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKey;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.mojang.blaze3d.platform.Lighting;
import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexBuffer;
import com.mojang.blaze3d.vertex.VertexSorting;
import com.seibel.distanthorizons.api.methods.events.sharedParameterObjects.DhApiRenderParam;
import com.seibel.distanthorizons.api.objects.math.DhApiMat4f;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.renderer.RenderType;
import net.minecraft.client.renderer.ShaderInstance;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.entity.EntityRenderDispatcher;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.util.Mth;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.client.event.RenderLevelStageEvent;
import org.joml.FrustumIntersection;
import org.joml.Matrix4f;
import org.joml.Matrix4fStack;
import org.joml.Vector3f;
import org.lwjgl.opengl.GL30;

/**
 * Far movers as the real thing (docs/choices-dh-feed.md DHT63, DECISIONS.md DH-6): a carriage as the blocks its
 * player built, a dragon as Ice and Fire's own model, wherever a plugin's {@link FarLook} has the model. The
 * boxes ({@link FarTrainView}, {@link FarDragonView}, {@link FarBandRenderer}) stay for the rest: a mover whose
 * look has nothing, or whose model is still baking after {@link #GRACE_NANOS}.
 *
 * <ul>
 *   <li><b>In DH's pass</b>: drawn from DH's {@code DhApiBeforeGenericRenderCleanupEvent}, the first time it
 *       fires in a frame (the generic pass before DH's SSAO, after its opaque LODs and before its transparent
 *       ones and its fog), with DH's framebuffer, projection, model view and depth function. So DH terrain in
 *       front hides them, they hide DH terrain behind them, and DH's own fog pass fades them by the depth they
 *       wrote, as it fades the terrain. Vanilla's fog is off for the draw (it would end at vanilla's render
 *       distance) and every GL state the draw touches is put back for DH ({@link GlState}).</li>
 *   <li><b>In the band</b> (the loaded chunks where the real thing has no entity): drawn after vanilla's
 *       entities with vanilla's matrices, depth and fog, as the band's boxes were.</li>
 *   <li><b>Who draws what</b> is the boxes' rule ({@link FarHide}, {@link HandOverRule}, {@link FadeRing}): no
 *       model while the client has the real entity (a dragon's body is copied into DH's pass toward the fade,
 *       {@link FarHide#bodyCopied}); DH's pass beyond the loaded chunks and in DH's vanilla fade;
 *       the band inside them. Each kind is held to the tier's count ({@link FarDetail#dragons},
 *       {@link FarDetail#feedObjects}), nearest first by horizontal distance as the boxes rank theirs, a dragon
 *       reaching as far as its boxes do; a dragon still drawn as a box takes its place in that count here, and a
 *       model in {@link FarDragonView}'s, so in DH's pass the models and the boxes together keep to it. The
 *       band's boxes have no count of their own, so there only the models keep to it.</li>
 *   <li><b>Live</b>: the nearest {@link #LIVE_DRAGONS} dragons within {@link #LIVE_DISTANCE} blocks are drawn by
 *       their own renderer each frame (smooth wings), the rest from baked frames. Live once a frame: in DH's
 *       vanilla fade, where both passes draw, both draw the baked frame the live one is at, never two poses.</li>
 * </ul>
 *
 * <p>Render thread only.
 */
final class FarModelRenderer {
    /** How a mover is drawn this frame. */
    enum Cover {
        /** By the box renderers: no look, or nothing to show. */
        BOX,
        /** By this renderer. */
        MODEL,
        /** By nobody, for a moment: its model bakes (a box drawn for two frames would only flicker). */
        WAIT
    }

    /** How long a mover may wait for its model before its box is drawn instead. PLACEHOLDER. */
    static final long GRACE_NANOS = 2_000_000_000L;
    /** Dragons drawn live: the nearest this many, within this distance. PLACEHOLDERS (live costs CPU). */
    static final int LIVE_DRAGONS = 2;
    static final double LIVE_DISTANCE = 300.0;
    /** The models off, for the bench's before and after ({@code -Dmic_rtsim.farbench.models=false}). */
    static final boolean OFF = FarBench.ENABLED && "false".equals(System.getProperty("mic_rtsim.farbench.models"));
    /** Lighting's two level lights (world axes), turned per model into its baked frame. */
    private static final Vector3f LIGHT0 = new Vector3f(0.2f, 1.0f, -0.7f).normalize(),
            LIGHT1 = new Vector3f(-0.2f, 1.0f, 0.7f).normalize();

    /** One mover's model this frame. */
    private static final class Item {
        FarMover mover;
        /** Its baked frame, and, when {@link #live}, its live model. */
        final FarModel model = new FarModel(), liveModel = new FarModel();
        double x, y, z, distance, d2;
        boolean live;
        /** Drawn live in the pass being drawn. */
        boolean drawLive;
        /** A dragon still drawn as a box: never drawn here, only counted against the tier. */
        boolean placeholder;
        /** A dragon's body vanilla draws, copied into DH's pass here ({@link FarHide#bodyCopied}); else null. */
        Entity body;
        final Matrix4f mv = new Matrix4f();
    }

    private static final double[] SEG = new double[6];
    private static final List<Item> POOL = new ArrayList<>(), ITEMS = new ArrayList<>(), DRAWN = new ArrayList<>();
    private static final Map<RenderType, List<Object[]>> BY_TYPE = new LinkedHashMap<>();
    private static final Map<FarKey, long[]> WAITING = new HashMap<>();
    private static final Batch DH_BATCH = new Batch(true), BAND_BATCH = new Batch(false);
    private static final GlState SAVED = new GlState();
    private static final Vector3f L0 = new Vector3f(), L1 = new Vector3f();

    private static long frameNanos, lastPrune;
    /** The level the items were made in (replaced the frame it changes, so it never outlives it). */
    private static ClientLevel lastLevel;
    /** The looks' cold first draws are done (Create's bogeys): at the first frame the far view runs. */
    private static boolean prewarmed;
    private static long preparedFrame = -1, dhDrawnFrame = -1;
    private static int depthFunc, dhDrawFbo;
    private static int errors;

    // What the last frame did, for the bench
    static final GpuTimer DH_GPU = new GpuTimer(), BAND_GPU = new GpuTimer();
    static int lastCandidates, lastDh, lastBand, lastLive, lastWaiting, lastCopies;
    /** The most a dragon model's radius passed the reach its hand-over uses (FarDragonView#reach), blocks. */
    static double overReach = Double.NEGATIVE_INFINITY;
    static long lastVertices;
    static long dhNanos, bandNanos, prepareNanos, lookNanos, timedFrames, dhMaxNanos, lookMaxNanos;

    private FarModelRenderer() {}

    // -----------------------------------------------------------------------------------------------------
    // The frame

    /**
     * A new frame (RenderFrameEvent.Pre, after {@link FarTrainView#newFrame}): the looks bake within their
     * budget, outside every render pass.
     */
    static void newFrame() {
        frameNanos = System.nanoTime();
        ClientLevel level = Minecraft.getInstance().level;
        if (level != lastLevel) {
            // A new level (another dimension, another world): nothing here holds the old one (its stand-ins)
            lastLevel = level;
            POOL.clear();
            ITEMS.clear();
            DRAWN.clear();
            WAITING.clear();
        }
        if (OFF || !FarGates.ready())
            return;
        if (!prewarmed) {
            // Behind the loading screen when it joins a world, or in the options screen when the detail goes on
            prewarmed = true;
            FarLooks.prewarmAll();
        }
        FarLooks.frame();
        long now = System.nanoTime();
        lookNanos += now - frameNanos;
        lookMaxNanos = Math.max(lookMaxNanos, now - frameNanos);
        if (now - lastPrune > 1_000_000_000L) {
            lastPrune = now;
            WAITING.values().removeIf(t -> frameNanos - t[1] > 5_000_000_000L);
        }
    }

    /** The client left its world: the looks free their meshes, and nothing waits. */
    static void releaseAll() {
        prewarmed = false;
        WAITING.clear();
        ITEMS.clear();
        DRAWN.clear();
        POOL.clear(); // their models hold the old world's stand-ins
        DH_BATCH.close();
        BAND_BATCH.close();
        FarLooks.clearAll();
    }

    /** Who draws mover {@code m} this frame. Asks its look for the model when it has none yet. */
    static Cover cover(FarMover m) {
        if (OFF)
            return Cover.BOX;
        FarLook look = FarLooks.of(m.kind());
        if (look == null)
            return Cover.BOX;
        FarLook.Status s;
        try {
            s = look.status(m);
        } catch (RuntimeException | LinkageError e) {
            if (errors++ < 5)
                MicRtsimDh.LOGGER.error("mic_rtsim_dh: a far look failed", e);
            return Cover.BOX;
        }
        if (s == FarLook.Status.READY) {
            WAITING.remove(m.key());
            return Cover.MODEL;
        }
        if (s == FarLook.Status.NONE)
            return Cover.BOX;
        long[] t = WAITING.computeIfAbsent(m.key(), k -> new long[]{frameNanos, frameNanos});
        t[1] = frameNanos;
        return frameNanos - t[0] < GRACE_NANOS ? Cover.WAIT : Cover.BOX;
    }

    /**
     * This frame's models, once (the first pass to draw asks): the movers in this level and DH's radius whose
     * look has a model and whose entity the client does not have, placed, nearest first; the nearest dragons
     * live. The dragons drawn as boxes come too, as placeholders for the tier's count.
     */
    private static void prepare(ClientLevel level, Vec3 cam, double clock) {
        long frame = FarTrainView.frame();
        if (preparedFrame == frame)
            return;
        preparedFrame = frame;
        long t0 = System.nanoTime();
        for (Item it : ITEMS) {
            // Last frame's copies hold their entity, and their live render it: not past this frame
            it.body = null;
            it.liveModel.reset();
        }
        ITEMS.clear();
        String dimension = level.dimension().location().toString();
        double radius = FarGates.radiusBlocks(), radiusSq = radius * radius;
        int waiting = 0;
        for (FarMover m : FarObjects.movers()) {
            if (m.kind() != FarKind.CARRIAGE && m.kind() != FarKind.DRAGON || !dimension.equals(m.dimension()))
                continue;
            m.segmentAt(clock, SEG);
            double x = (SEG[0] + SEG[3]) / 2.0, y = (SEG[1] + SEG[4]) / 2.0, z = (SEG[2] + SEG[5]) / 2.0;
            double dx = x - cam.x, dy = y - cam.y, dz = z - cam.z;
            if (!(dx * dx + dz * dz <= radiusSq))
                continue;
            Entity body = FarHide.bodyCopied(m, level, x, y, z, cam);
            if (body != null) {
                copyBody(m, body, cam);
                continue;
            }
            Cover c = cover(m);
            if (c == Cover.WAIT)
                waiting++;
            boolean placeholder = c == Cover.BOX && m.kind() == FarKind.DRAGON;
            if (c != Cover.MODEL && !placeholder || FarHide.entityPresent(m, level, x, y, z, cam))
                continue;
            Item it = ITEMS.size() < POOL.size() ? POOL.get(ITEMS.size()) : null;
            if (it == null)
                POOL.add(it = new Item());
            if (!placeholder && !place(m, clock, false, it.model.reset()))
                continue;
            it.mover = m;
            it.x = x;
            it.y = y;
            it.z = z;
            it.d2 = dx * dx + dz * dz;
            it.distance = Math.sqrt(it.d2 + dy * dy);
            it.live = false;
            it.liveModel.reset();
            it.placeholder = placeholder;
            it.body = null;
            if (!placeholder && m.kind() == FarKind.DRAGON)
                overReach = Math.max(overReach, it.model.radius - FarDragonView.reach(m.size()));
            ITEMS.add(it);
        }
        ITEMS.sort((a, b) -> Double.compare(a.d2, b.d2));
        int live = 0, models = 0;
        for (Item it : ITEMS) {
            if (it.placeholder || it.body != null)
                continue;
            models++;
            if (live >= LIVE_DRAGONS || it.distance >= LIVE_DISTANCE || it.mover.kind() != FarKind.DRAGON)
                continue;
            it.mover.segmentAt(clock, SEG);
            if (place(it.mover, clock, true, it.liveModel.reset()) && it.liveModel.live != null) {
                it.live = true;
                live++;
            } else
                it.liveModel.reset();
        }
        lastCandidates = models;
        lastLive = live;
        lastWaiting = waiting;
        prepareNanos += System.nanoTime() - t0;
    }

    /**
     * A copy of a dragon's body that vanilla draws, for DH's pass (docs/choices-dh-feed.md DHT47): drawn as
     * vanilla's entity pass draws it ({@code LevelRenderer.renderEntity}, {@code EntityRenderDispatcher.render}),
     * at the same partial tick, place, heading and light, with its own renderer, so it is the body's pose to the
     * pixel. Its fire and shadow are left out: a dragon does not burn, and vanilla's shadow is gone 16 blocks
     * out. Not held to the tier's count: vanilla draws the body anyway.
     */
    private static void copyBody(FarMover m, Entity body, Vec3 cam) {
        Minecraft mc = Minecraft.getInstance();
        float partial = mc.getTimer().getGameTimeDeltaPartialTick(!body.level().tickRateManager().isEntityFrozen(body));
        EntityRenderDispatcher dispatcher = mc.getEntityRenderDispatcher();
        EntityRenderer<? super Entity> r = dispatcher.getRenderer(body);
        Vec3 offset = r.getRenderOffset(body, partial);
        float yaw = Mth.lerp(partial, body.yRotO, body.getYRot());
        int light = dispatcher.getPackedLightCoords(body, partial);
        Item it = ITEMS.size() < POOL.size() ? POOL.get(ITEMS.size()) : null;
        if (it == null)
            POOL.add(it = new Item());
        it.mover = m;
        it.body = body;
        it.placeholder = false;
        it.x = Mth.lerp(partial, body.xOld, body.getX()) + offset.x;
        it.y = Mth.lerp(partial, body.yOld, body.getY()) + offset.y;
        it.z = Mth.lerp(partial, body.zOld, body.getZ()) + offset.z;
        double dx = it.x - cam.x, dy = it.y - cam.y, dz = it.z - cam.z;
        it.d2 = dx * dx + dz * dz;
        it.distance = Math.sqrt(it.d2 + dy * dy);
        // For the frustum only: about its feet, the box's wingspan, the most a model's wings reached past it, and
        // its height
        it.model.reset().radius = (float) (FarDragonView.reach(m.size()) + 16.0 + body.getBbHeight());
        it.live = true;
        it.liveModel.reset().live = (ps, buffers) -> r.render(body, yaw, partial, ps, buffers, light);
        ITEMS.add(it);
    }

    /**
     * How far {@code it} reaches from its midpoint, for the hand-over: a dragon's as its boxes' (so both views
     * count the same dragons), a carriage's its model's.
     */
    private static double reach(Item it) {
        return it.mover.kind() == FarKind.DRAGON ? FarDragonView.reach(it.mover.size()) : it.model.radius;
    }

    /** {@code m}'s look places its model at {@link #SEG}; false when it has none now. */
    private static boolean place(FarMover m, double clock, boolean live, FarModel out) {
        FarLook look = FarLooks.of(m.kind());
        try {
            return look != null && look.place(m, SEG, clock, live, out);
        } catch (RuntimeException | LinkageError e) {
            if (errors++ < 5)
                MicRtsimDh.LOGGER.error("mic_rtsim_dh: a far look would not place its model", e);
            return false;
        }
    }

    // -----------------------------------------------------------------------------------------------------
    // DH's pass

    /** DhApiBeforeGenericRenderCleanupEvent: fired twice a frame; draws the first time. */
    static void onDhCleanup(DhApiRenderParam p) {
        long frame = FarTrainView.frame();
        if (dhDrawnFrame == frame)
            return;
        dhDrawnFrame = frame;
        lastDh = lastCopies = 0;
        if (OFF || !FarGates.ready())
            return;
        Minecraft mc = Minecraft.getInstance();
        ClientLevel level = mc.level;
        if (level == null)
            return;
        long t0 = System.nanoTime();
        try {
            drawDh(mc, level, p);
        } catch (RuntimeException | LinkageError e) {
            if (errors++ < 5)
                MicRtsimDh.LOGGER.error("mic_rtsim_dh: far models failed in DH's pass this frame", e);
        }
        long spent = System.nanoTime() - t0;
        dhNanos += spent;
        dhMaxNanos = Math.max(dhMaxNanos, spent);
        timedFrames++;
    }

    private static void drawDh(Minecraft mc, ClientLevel level, DhApiRenderParam p) {
        Vec3 cam = mc.gameRenderer.getMainCamera().getPosition();
        prepare(level, cam, FarClock.ticks() + p.partialTicks);
        if (ITEMS.isEmpty())
            return;
        Matrix4f proj = joml(p.dhProjectionMatrix), mv = joml(p.dhModelViewMatrix);
        FrustumIntersection frustum = new FrustumIntersection(new Matrix4f(proj).mul(mv));
        FarDetail tier = DhFarConfig.detail();
        DRAWN.clear();
        int dragons = 0, carriages = 0;
        for (Item it : ITEMS) {
            FarMover m = it.mover;
            if (it.body != null) {
                // Vanilla draws the body; this is its copy for DH's fade, not a far model of the tier's
                it.drawLive = true;
                float rx = (float) (it.x - cam.x), ry = (float) (it.y - cam.y), rz = (float) (it.z - cam.z);
                if (frustum.testSphere(rx, ry, rz, it.model.radius)) {
                    DRAWN.add(it);
                    lastCopies++;
                }
                continue;
            }
            if (!FarHide.dhDraws(m, level, it.x, it.y, it.z, reach(it), cam))
                continue;
            if (m.kind() == FarKind.DRAGON ? dragons++ >= tier.dragons : carriages++ >= tier.feedObjects)
                continue;
            if (it.placeholder)
                continue;
            // Live once a frame, and where the band draws it too (DH's vanilla fade) both draw the baked frame
            it.drawLive = it.live && !FarHide.bandDraws(m, level, it.x, it.y, it.z, cam);
            float rx = (float) (it.x - cam.x), ry = (float) (it.y - cam.y), rz = (float) (it.z - cam.z);
            if (!frustum.testSphere(rx, ry, rz, it.model.radius))
                continue;
            it.mv.set(mv).translate(rx, ry, rz).mul(it.model.local);
            DRAWN.add(it);
        }
        lastDh = DRAWN.size();
        if (DRAWN.isEmpty())
            return;

        boolean timed = FarBench.ENABLED;
        SAVED.read();
        depthFunc = SAVED.depthFunc;
        dhDrawFbo = SAVED.drawFbo;
        GlState.syncCaches(SAVED);
        float fogStart = RenderSystem.getShaderFogStart(), fogEnd = RenderSystem.getShaderFogEnd();
        RenderSystem.setShaderFogStart(Float.MAX_VALUE);
        RenderSystem.setShaderFogEnd(Float.MAX_VALUE);
        // Saved here, not in RenderSystem's one backup slot, which a caller of DH's may be using
        Matrix4f savedProj = new Matrix4f(RenderSystem.getProjectionMatrix());
        VertexSorting savedSorting = RenderSystem.getVertexSorting();
        RenderSystem.setProjectionMatrix(proj, VertexSorting.DISTANCE_TO_ORIGIN);
        Matrix4fStack stack = RenderSystem.getModelViewStack();
        stack.pushMatrix();
        stack.set(mv);
        RenderSystem.applyModelViewMatrix();
        // ShaderInstance remembers the last program it bound; DH bound its own behind its back
        ShaderInstance any = GameRenderer.getPositionTexShader();
        if (any != null)
            any.clear();
        if (timed)
            DH_GPU.begin();
        try {
            Lighting.setupLevel();
            draw(mc, DRAWN, mv, proj, true, cam, DH_BATCH);
        } finally {
            if (timed)
                DH_GPU.end();
            stack.popMatrix();
            RenderSystem.applyModelViewMatrix();
            RenderSystem.setProjectionMatrix(savedProj, savedSorting);
            RenderSystem.setShaderFogStart(fogStart);
            RenderSystem.setShaderFogEnd(fogEnd);
            SAVED.apply();
        }
    }

    /**
     * After a render type's own setup: DH's depth function and DH's framebuffer, whatever the type set. The bind
     * is unconditional: asking GL which is bound waits for the driver's thread (Mesa's glthread), once a type.
     */
    static void dhState() {
        RenderSystem.depthFunc(depthFunc);
        com.mojang.blaze3d.platform.GlStateManager._glBindFramebuffer(GL30.GL_FRAMEBUFFER, dhDrawFbo);
    }

    /**
     * DH's matrix as JOML's. DH's fields are named row then column ({@code m32} is row 3, column 2); a
     * perspective projection has -1 there.
     */
    static Matrix4f joml(DhApiMat4f d) {
        return new Matrix4f(d.m00, d.m10, d.m20, d.m30, d.m01, d.m11, d.m21, d.m31, d.m02, d.m12, d.m22, d.m32, d.m03,
                d.m13, d.m23, d.m33);
    }

    // -----------------------------------------------------------------------------------------------------
    // The band

    /** RenderLevelStageEvent AFTER_ENTITIES (from {@link FarBandRenderer#onStage}). */
    static void onBand(RenderLevelStageEvent event, ClientLevel level) {
        lastBand = 0;
        if (OFF)
            return;
        long t0 = System.nanoTime();
        try {
            drawBand(event, level);
        } catch (RuntimeException | LinkageError e) {
            if (errors++ < 5)
                MicRtsimDh.LOGGER.error("mic_rtsim_dh: far models failed in the band this frame", e);
        }
        bandNanos += System.nanoTime() - t0;
    }

    private static void drawBand(RenderLevelStageEvent event, ClientLevel level) {
        Vec3 cam = event.getCamera().getPosition();
        prepare(level, cam, FarClock.ticks() + event.getPartialTick().getGameTimeDeltaPartialTick(false));
        if (ITEMS.isEmpty())
            return;
        Frustum frustum = event.getFrustum();
        FarDetail tier = DhFarConfig.detail();
        Matrix4f mv = new Matrix4f(RenderSystem.getModelViewMatrix()), proj = new Matrix4f(RenderSystem.getProjectionMatrix());
        DRAWN.clear();
        int dragons = 0, carriages = 0;
        for (Item it : ITEMS) {
            FarMover m = it.mover;
            if (it.body != null || !FarHide.bandDraws(m, level, it.x, it.y, it.z, cam))
                continue;
            if (m.kind() == FarKind.DRAGON ? dragons++ >= tier.dragons : carriages++ >= tier.feedObjects)
                continue;
            if (it.placeholder)
                continue;
            it.drawLive = it.live && !FarHide.dhDraws(m, level, it.x, it.y, it.z, reach(it), cam);
            double r = it.model.radius;
            if (!frustum.isVisible(new AABB(it.x - r, it.y - r, it.z - r, it.x + r, it.y + r, it.z + r)))
                continue;
            it.mv.set(mv).translate((float) (it.x - cam.x), (float) (it.y - cam.y), (float) (it.z - cam.z))
                    .mul(it.model.local);
            DRAWN.add(it);
        }
        lastBand = DRAWN.size();
        if (DRAWN.isEmpty())
            return;
        boolean timed = FarBench.ENABLED;
        if (timed)
            BAND_GPU.begin();
        try {
            Lighting.setupLevel();
            draw(Minecraft.getInstance(), DRAWN, mv, proj, false, cam, BAND_BATCH);
        } finally {
            if (timed)
                BAND_GPU.end();
        }
    }

    // -----------------------------------------------------------------------------------------------------
    // The draw

    /**
     * Draws the items' baked parts a render type at a time (opaque types first, sorted ones last), then the live
     * ones through their renderer.
     */
    private static void draw(Minecraft mc, List<Item> items, Matrix4f mv, Matrix4f proj, boolean dh, Vec3 cam,
            Batch batch) {
        for (List<Object[]> l : BY_TYPE.values())
            l.clear();
        long vertices = 0;
        boolean anyLive = false;
        for (Item it : items) {
            if (it.drawLive) {
                anyLive = true;
                continue;
            }
            for (FarModel.Part part : it.model.parts) {
                BY_TYPE.computeIfAbsent(part.type(), t -> new ArrayList<>()).add(new Object[]{part, it});
                vertices += part.vertices();
            }
        }
        var window = mc.getWindow();
        for (int pass = 0; pass < 2; pass++)
            for (Map.Entry<RenderType, List<Object[]>> e : BY_TYPE.entrySet()) {
                RenderType rt = e.getKey();
                if (e.getValue().isEmpty() || rt.sortOnUpload() != (pass == 1))
                    continue;
                rt.setupRenderState();
                try {
                    if (dh)
                        dhState();
                    ShaderInstance shader = RenderSystem.getShader();
                    if (shader == null)
                        continue;
                    if (shader.CHUNK_OFFSET != null)
                        shader.CHUNK_OFFSET.set(0f, 0f, 0f);
                    shader.setDefaultUniforms(rt.mode(), mv, proj, window);
                    shader.apply();
                    for (Object[] o : e.getValue()) {
                        FarModel.Part part = (FarModel.Part) o[0];
                        Item it = (Item) o[1];
                        if (shader.MODEL_VIEW_MATRIX != null) {
                            shader.MODEL_VIEW_MATRIX.set(it.mv);
                            shader.MODEL_VIEW_MATRIX.upload();
                        }
                        // The shader lights with the normals as baked: turn the lights the other way instead
                        float turn = Float.isNaN(it.model.lightYaw) ? 0f : -it.model.lightYaw;
                        if (shader.LIGHT0_DIRECTION != null) {
                            shader.LIGHT0_DIRECTION.set(L0.set(LIGHT0).rotateY(turn));
                            shader.LIGHT0_DIRECTION.upload();
                        }
                        if (shader.LIGHT1_DIRECTION != null) {
                            shader.LIGHT1_DIRECTION.set(L1.set(LIGHT1).rotateY(turn));
                            shader.LIGHT1_DIRECTION.upload();
                        }
                        VertexBuffer vb = part.buffer();
                        vb.bind();
                        vb.draw();
                    }
                    shader.clear();
                } finally {
                    VertexBuffer.unbind();
                    rt.clearRenderState();
                }
            }
        if (anyLive) {
            batch.vertices = 0;
            try {
                for (Item it : items) {
                    if (!it.drawLive)
                        continue;
                    // A pose stack of its own: a render that throws half way leaves its pushes behind
                    PoseStack pose = new PoseStack();
                    pose.translate((float) (it.x - cam.x), (float) (it.y - cam.y), (float) (it.z - cam.z));
                    it.liveModel.live.render(pose, batch);
                }
                batch.endBatch();
            } catch (RuntimeException | LinkageError e) {
                batch.discard();
                throw e;
            }
            vertices += batch.vertices;
        }
        lastVertices = vertices;
    }

    // -----------------------------------------------------------------------------------------------------
    // The bench

    /** The window's numbers, a line, and the looks' (each its own line); the sums start again. */
    static List<String> stats() {
        List<String> out = new ArrayList<>();
        DH_GPU.collect();
        BAND_GPU.collect();
        double f = Math.max(1, timedFrames);
        out.add(String.format(Locale.ROOT,
                "models: %d candidates, %d in DH's pass (%d of them bodies' copies), %d in the band, %d live, %d "
                        + "waiting, %d vertices, dragons "
                        + "past their reach by %.1f blocks at most; CPU per frame: DH pass %.3f ms (longest %.2f), "
                        + "band %.3f ms, of which placing %.3f ms, bakes %.3f ms (longest %.2f); GPU per frame: DH "
                        + "pass %.3f ms over %d, band %.3f ms over %d; RenderSystem's matrices at the band off the "
                        + "stage's by %.4f (model-view) and %.4f (projection) at most",
                lastCandidates, lastDh, lastCopies, lastBand, lastLive, lastWaiting, lastVertices, overReach, dhNanos / 1e6 / f,
                dhMaxNanos / 1e6, bandNanos / 1e6 / f, prepareNanos / 1e6 / f, lookNanos / 1e6 / f,
                lookMaxNanos / 1e6,
                DH_GPU.samples == 0 ? 0.0 : DH_GPU.sumNanos / 1e6 / DH_GPU.samples, DH_GPU.samples,
                BAND_GPU.samples == 0 ? 0.0 : BAND_GPU.sumNanos / 1e6 / BAND_GPU.samples, BAND_GPU.samples,
                FarBandRenderer.mvDrift, FarBandRenderer.projDrift));
        out.addAll(FarLooks.statsAll());
        resetSums();
        return out;
    }

    static void resetSums() {
        dhNanos = bandNanos = prepareNanos = lookNanos = timedFrames = dhMaxNanos = lookMaxNanos = 0;
        overReach = Double.NEGATIVE_INFINITY;
        FarBandRenderer.mvDrift = FarBandRenderer.projDrift = 0f;
        DH_GPU.resetSums();
        BAND_GPU.resetSums();
    }
}
