package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vertex.BufferBuilder;
import com.mojang.blaze3d.vertex.ByteBufferBuilder;
import com.mojang.blaze3d.vertex.MeshData;
import com.mojang.blaze3d.vertex.VertexConsumer;
import java.util.IdentityHashMap;
import java.util.LinkedHashMap;
import java.util.Map;
import net.minecraft.client.renderer.MultiBufferSource;
import net.minecraft.client.renderer.RenderType;

/**
 * A buffer source for the far models drawn live (the nearest dragons, by their own renderer): one buffer per
 * render type, drawn in the order the types were first asked for. In DH's pass ({@code dh}) its types set up as
 * theirs do and then put DH's depth function and framebuffer back ({@link FarModelRenderer#dhState}).
 * Render thread.
 */
final class Batch implements MultiBufferSource {
    private static final Map<RenderType, RenderType> WRAPPED = new IdentityHashMap<>();
    private final Map<RenderType, ByteBufferBuilder> memory = new LinkedHashMap<>();
    private final Map<RenderType, BufferBuilder> started = new LinkedHashMap<>();
    private final boolean dh;
    long vertices;

    Batch(boolean dh) {
        this.dh = dh;
    }

    @Override
    public VertexConsumer getBuffer(RenderType rt) {
        RenderType w = dh ? inDh(rt) : rt;
        BufferBuilder b = started.get(w);
        if (b == null) {
            ByteBufferBuilder m = memory.computeIfAbsent(w, x -> new ByteBufferBuilder(256 * 1024));
            b = new BufferBuilder(m, w.mode(), w.format());
            started.put(w, b);
        }
        return b;
    }

    /** Draws what was written, opaque types first and the sorted (translucent) ones after. */
    void endBatch() {
        try {
            for (int pass = 0; pass < 2; pass++)
                for (Map.Entry<RenderType, BufferBuilder> e : started.entrySet()) {
                    RenderType rt = e.getKey();
                    if (rt.sortOnUpload() != (pass == 1))
                        continue;
                    MeshData md = e.getValue().build();
                    if (md == null)
                        continue;
                    vertices += md.drawState().vertexCount();
                    if (rt.sortOnUpload())
                        md.sortQuads(memory.get(rt), RenderSystem.getVertexSorting());
                    rt.draw(md);
                }
        } finally {
            started.clear();
        }
    }

    /** Drops what was written and not drawn (a draw that failed half way). */
    void discard() {
        for (BufferBuilder b : started.values()) {
            MeshData md = b.build();
            if (md != null)
                md.close();
        }
        started.clear();
    }

    /** Frees the buffers' memory (the client left its world). */
    void close() {
        discard();
        for (ByteBufferBuilder m : memory.values())
            m.close();
        memory.clear();
    }

    /** A render type that sets up as {@code rt} does, then puts DH's depth function and framebuffer back. */
    private static RenderType inDh(RenderType rt) {
        return WRAPPED.computeIfAbsent(rt, o -> new RenderType(MicRtsimDh.MODID + "_dh_" + o, o.format(), o.mode(),
                o.bufferSize(), false, o.sortOnUpload(), () -> {
                    o.setupRenderState();
                    FarModelRenderer.dhState();
                }, o::clearRenderState) {
        });
    }
}
