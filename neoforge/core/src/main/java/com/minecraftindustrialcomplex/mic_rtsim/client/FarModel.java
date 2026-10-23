package com.minecraftindustrialcomplex.mic_rtsim.client;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexBuffer;
import java.util.List;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;
import org.joml.Matrix4f;

/**
 * A far mover's model for one frame, as a {@link FarLook} places it (docs/choices-dh-feed.md DHT63): baked
 * vertex buffers, one per render type, and the turn that puts them about the mover's midpoint; or a live
 * render. The renderer owns the holder and fills it again each frame.
 */
public final class FarModel {
    /** One baked buffer and the render type it is drawn with. The look owns the buffer. */
    public record Part(RenderType type, VertexBuffer buffer, int vertices) {
    }

    /** A model drawn live, each frame, by its own renderer (the nearest dragons, whose wings then beat smoothly). */
    public interface Live {
        /**
         * Draws it into {@code buffers}; {@code ps} is at the mover's midpoint, in world axes, camera relative.
         * Called once in each pass that draws the mover, so twice a frame in DH's vanilla fade: each call must
         * draw the same pose and do nothing but draw (no ticking, counting or effects of its own).
         */
        void render(PoseStack ps, MultiBufferSource buffers);
    }

    /** The baked parts to draw; empty when {@link #live} draws it. */
    public List<Part> parts = List.of();
    /** Model space to world axes at the mover's midpoint (no translation to the midpoint itself). */
    public final Matrix4f local = new Matrix4f();
    /** A sphere about the midpoint that holds the whole model, for the frustum and the hand-over. */
    public float radius;
    /**
     * Radians the model is turned by about +y, for render types lit by the level's two lights: the renderer turns
     * the lights the other way, since the baked normals do not turn with the model. NaN for unlit parts.
     */
    public float lightYaw = Float.NaN;
    /** When set, this draws the model and {@link #parts} is ignored. */
    public Live live;

    public FarModel reset() {
        parts = List.of();
        local.identity();
        radius = 0f;
        lightYaw = Float.NaN;
        live = null;
        return this;
    }
}
