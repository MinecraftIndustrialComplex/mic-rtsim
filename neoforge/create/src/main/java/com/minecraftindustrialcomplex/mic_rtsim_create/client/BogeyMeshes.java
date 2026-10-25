package com.minecraftindustrialcomplex.mic_rtsim_create.client;

import com.minecraftindustrialcomplex.mic_rtsim.client.NullVertexConsumer;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.mojang.blaze3d.vertex.DefaultVertexFormat;
import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import com.mojang.math.Axis;
import com.simibubi.create.AllBogeyStyles;
import com.simibubi.create.content.trains.bogey.BogeySizes;
import com.simibubi.create.content.trains.bogey.BogeyStyle;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Locale;
import java.util.Map;
import net.minecraft.client.renderer.LightTexture;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.texture.OverlayTexture;
import net.minecraft.core.Direction;
import net.minecraft.resources.ResourceLocation;

/**
 * Create's bogeys for far carriage meshes (docs/choices-dh-feed.md DHT64). Create draws a bogey with its own
 * renderer, not as a block model, so the carriage's block list has none of its geometry: each style, size,
 * axis and orientation is drawn once by Create's renderer (wheels at rest, open sky) and its vertices kept, then
 * copied into every carriage mesh that has it. Only the block vertex format is kept, in the same four layers as
 * the blocks. Render thread.
 */
final class BogeyMeshes {
    /** A bogey's vertices as Create's renderer wrote them about its block's corner, per layer. */
    static final class Mesh {
        static final int FS = 8, IS = 2; // x y z u v nx ny nz; colour, light
        final float[][] f = new float[CarriageLook.LAYERS.length][];
        final int[][] i = new int[CarriageLook.LAYERS.length][];
        final int[] count = new int[CarriageLook.LAYERS.length];

        void replay(int layer, VertexConsumer to, float ox, float oy, float oz) {
            float[] d = f[layer];
            int[] e = i[layer];
            for (int k = 0; k < count[layer]; k++) {
                int o = k * FS, p = k * IS;
                to.addVertex(d[o] + ox, d[o + 1] + oy, d[o + 2] + oz).setColor(e[p]).setUv(d[o + 3], d[o + 4])
                        .setLight(e[p + 1]).setNormal(d[o + 5], d[o + 6], d[o + 7]);
            }
        }
    }

    /** Records vertices of the block format: position, colour, texture, light, normal. */
    private static final class Recorder extends NullVertexConsumer {
        float[] f = new float[1024 * Mesh.FS];
        int[] i = new int[1024 * Mesh.IS];
        int n = -1;

        @Override
        public VertexConsumer addVertex(float x, float y, float z) {
            n++;
            if ((n + 1) * Mesh.FS > f.length) {
                f = Arrays.copyOf(f, f.length * 2);
                i = Arrays.copyOf(i, i.length * 2);
            }
            int o = n * Mesh.FS, p = n * Mesh.IS;
            f[o] = x;
            f[o + 1] = y;
            f[o + 2] = z;
            f[o + 3] = 0f;
            f[o + 4] = 0f;
            f[o + 5] = 0f;
            f[o + 6] = 1f;
            f[o + 7] = 0f;
            i[p] = -1;
            i[p + 1] = 0;
            return this;
        }

        @Override
        public VertexConsumer setColor(int r, int g, int b, int alpha) {
            i[n * Mesh.IS] = alpha << 24 | r << 16 | g << 8 | b;
            return this;
        }

        @Override
        public VertexConsumer setUv(float u, float v) {
            f[n * Mesh.FS + 3] = u;
            f[n * Mesh.FS + 4] = v;
            return this;
        }

        @Override
        public VertexConsumer setUv2(int u, int v) {
            i[n * Mesh.IS + 1] = u & 0xFFFF | v << 16;
            return this;
        }

        @Override
        public VertexConsumer setNormal(float x, float y, float z) {
            int o = n * Mesh.FS;
            f[o + 5] = x;
            f[o + 6] = y;
            f[o + 7] = z;
            return this;
        }
    }

    /**
     * How far from its block's middle a bogey's vertices may be, blocks; a bogey drawn with a vertex past that, or
     * not finite, is left out of the far meshes (DHT73).
     */
    static final float REACH = 5f;
    /** By style, size, axis and orientation; a null value is a bogey that would not draw. */
    private static final Map<String, Mesh> MESHES = new HashMap<>();
    static int drawn, failed, outside;
    static long drawNanos;

    private BogeyMeshes() {}

    /**
     * Draws every bogey style in every size it has, on both axes and both ways up, once. Create's first bogey
     * draw is cold (about 180 ms with Sodium: its partial models and renderer classes load then), so the far
     * renderer asks for this as the client joins a world, behind the loading screen, and no carriage bake pays it
     * inside a frame.
     */
    static void prewarm() {
        long t0 = System.nanoTime();
        int n = 0;
        for (BogeyStyle style : AllBogeyStyles.BOGEY_STYLES.values())
            for (BogeySizes.BogeySize size : style.validSizes())
                for (Direction.Axis axis : new Direction.Axis[]{Direction.Axis.X, Direction.Axis.Z})
                    for (boolean upsideDown : new boolean[]{false, true})
                        if (get(style.id.toString(), size.id().toString(), axis, upsideDown) != null)
                            n++;
        MicRtsimCreate.LOGGER.info("[mic_rtsim_create] {} far bogey meshes drawn in {} ms", n,
                String.format(Locale.ROOT, "%.1f", (System.nanoTime() - t0) / 1e6));
    }

    static void clear() {
        MESHES.clear();
    }

    /** The bogey's mesh, drawn now if it never was; null when Create does not know it or it would not draw. */
    static Mesh get(String styleId, String sizeId, Direction.Axis axis, boolean upsideDown) {
        String key = styleId + "|" + sizeId + "|" + axis + "|" + upsideDown;
        if (MESHES.containsKey(key))
            return MESHES.get(key);
        long t0 = System.nanoTime();
        Mesh mesh = null;
        try {
            mesh = draw(styleId, sizeId, axis, upsideDown);
        } catch (RuntimeException | LinkageError e) {
            if (failed++ < 3)
                MicRtsimCreate.LOGGER.warn("[mic_rtsim_create] bogey {} would not draw for the far view", key, e);
        }
        MESHES.put(key, mesh);
        drawn++;
        drawNanos += System.nanoTime() - t0;
        return mesh;
    }

    private static Mesh draw(String styleId, String sizeId, Direction.Axis axis, boolean upsideDown) {
        ResourceLocation styleLoc = ResourceLocation.tryParse(styleId), sizeLoc = ResourceLocation.tryParse(sizeId);
        BogeyStyle style = styleLoc == null ? null : AllBogeyStyles.BOGEY_STYLES.get(styleLoc);
        BogeySizes.BogeySize size = sizeLoc == null ? null : BogeySizes.all().get(sizeLoc);
        if (style == null || size == null)
            return null;
        Recorder[] rec = new Recorder[CarriageLook.LAYERS.length];
        MultiBufferSource capture = rt -> {
            if (rt.format() != DefaultVertexFormat.BLOCK)
                return NullVertexConsumer.INSTANCE;
            int layer = CarriageLook.layerOf(rt);
            if (rec[layer] == null)
                rec[layer] = new Recorder();
            return rec[layer];
        };
        // As Create places a bogey on its block: about the block's middle, turned to its axis, flipped about the
        // middle when upside down (CarriageContraptionEntityRenderer.translateBogey; the server sends each
        // bogey at its own block, so the height between a flipped and an upright one is in its position)
        PoseStack ps = new PoseStack();
        ps.translate(0.5, 0.5, 0.5);
        if (axis == Direction.Axis.X)
            ps.mulPose(Axis.YP.rotationDegrees(90));
        if (upsideDown)
            ps.mulPose(Axis.ZP.rotationDegrees(180));
        style.render(size, 0f, ps, capture, LightTexture.pack(0, 15), OverlayTexture.NO_OVERLAY, 0f, null, true);
        Mesh mesh = new Mesh();
        boolean any = false;
        for (int l = 0; l < rec.length; l++)
            for (int k = 0; rec[l] != null && k <= rec[l].n; k++) {
                float x = rec[l].f[k * Mesh.FS] - 0.5f, y = rec[l].f[k * Mesh.FS + 1] - 0.5f,
                        z = rec[l].f[k * Mesh.FS + 2] - 0.5f;
                // Written so that NaN fails too
                if (!(Math.abs(x) <= REACH && Math.abs(y) <= REACH && Math.abs(z) <= REACH)) {
                    if (outside++ < 5)
                        MicRtsimCreate.LOGGER.warn("[mic_rtsim_create] bogey {} {} drew a vertex at {} {} {} from its"
                                + " block's middle (past {}); left out of the far view", styleId, sizeId, x, y, z, REACH);
                    return null;
                }
            }
        for (int l = 0; l < rec.length; l++)
            if (rec[l] != null && rec[l].n >= 0) {
                mesh.count[l] = rec[l].n + 1;
                mesh.f[l] = Arrays.copyOf(rec[l].f, mesh.count[l] * Mesh.FS);
                mesh.i[l] = Arrays.copyOf(rec[l].i, mesh.count[l] * Mesh.IS);
                any = true;
            }
        return any ? mesh : null;
    }
}
