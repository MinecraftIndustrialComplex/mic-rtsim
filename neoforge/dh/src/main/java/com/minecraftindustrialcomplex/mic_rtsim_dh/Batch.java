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
    /** Past this many render types' buffers, the ones not drawn this frame are freed. */
    private static final int MAX_TYPES = 16;
    private final Map<RenderType, RenderType> wrapped = new IdentityHashMap<>();
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

    /**
     * Draws what was written, opaque types first and the sorted (translucent) ones after. A type whose build or
     * draw throws loses its buffer (it may hold half a vertex) and the others still draw; the first failure is
     * thrown after.
     */
    void endBatch() {
        RuntimeException failed = null;
        for (int pass = 0; pass < 2; pass++)
            for (Map.Entry<RenderType, BufferBuilder> e : started.entrySet()) {
                RenderType rt = e.getKey();
                if (rt.sortOnUpload() != (pass == 1))
                    continue;
                MeshData md = null;
                try {
                    md = e.getValue().build();
                    if (md == null)
                        continue;
                    vertices += md.drawState().vertexCount();
                    if (rt.sortOnUpload())
                        md.sortQuads(memory.get(rt), RenderSystem.getVertexSorting());
                    rt.draw(md); // uploads it, which closes it
                } catch (RuntimeException | LinkageError x) {
                    drop(rt);
                    if (failed == null)
                        failed = x instanceof RuntimeException r ? r : new IllegalStateException(x);
                } finally {
                    if (md != null)
                        md.close(); // a second close does nothing
                }
            }
        trim();
        started.clear();
        if (failed != null)
            throw failed;
    }

    /** Drops what was written and not drawn (a draw that failed half way): every started type loses its buffer. */
    void discard() {
        for (RenderType rt : started.keySet())
            drop(rt);
        started.clear();
    }

    /** Frees the buffers' memory (the client left its world). */
    void close() {
        started.clear();
        for (ByteBufferBuilder m : memory.values())
            m.close();
        memory.clear();
        wrapped.clear();
    }

    /** Frees {@code rt}'s buffer; the next draw of that type starts a new one. */
    private void drop(RenderType rt) {
        ByteBufferBuilder m = memory.remove(rt);
        if (m != null)
            m.close();
    }

    /** Past {@link #MAX_TYPES}, frees the buffers of the types not drawn this frame. */
    private void trim() {
        if (memory.size() <= MAX_TYPES)
            return;
        memory.entrySet().removeIf(e -> {
            if (started.containsKey(e.getKey()))
                return false;
            e.getValue().close();
            return true;
        });
        if (wrapped.size() > 4 * MAX_TYPES)
            wrapped.values().removeIf(w -> !memory.containsKey(w));
    }

    /** A render type that sets up as {@code rt} does, then puts DH's depth function and framebuffer back. */
    private RenderType inDh(RenderType rt) {
        return wrapped.computeIfAbsent(rt, o -> new RenderType(MicRtsimDh.MODID + "_dh_" + o, o.format(), o.mode(),
                o.bufferSize(), false, o.sortOnUpload(), () -> {
                    o.setupRenderState();
                    FarModelRenderer.dhState();
                }, o::clearRenderState) {
        });
    }
}
