package com.minecraftindustrialcomplex.mic_rtsim.client;

import com.mojang.blaze3d.vertex.VertexConsumer;

/**
 * A vertex consumer that keeps nothing: for a bake's warm-up renders and the render types a far mesh leaves out.
 * Subclasses override what they record.
 */
public class NullVertexConsumer implements VertexConsumer {
    public static final NullVertexConsumer INSTANCE = new NullVertexConsumer();

    @Override
    public VertexConsumer addVertex(float x, float y, float z) {
        return this;
    }

    @Override
    public VertexConsumer setColor(int r, int g, int b, int a) {
        return this;
    }

    @Override
    public VertexConsumer setUv(float u, float v) {
        return this;
    }

    @Override
    public VertexConsumer setUv1(int u, int v) {
        return this;
    }

    @Override
    public VertexConsumer setUv2(int u, int v) {
        return this;
    }

    @Override
    public VertexConsumer setNormal(float x, float y, float z) {
        return this;
    }
}
