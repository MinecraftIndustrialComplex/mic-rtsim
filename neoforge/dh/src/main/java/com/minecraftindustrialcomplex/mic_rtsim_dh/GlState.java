package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.mojang.blaze3d.platform.GlStateManager;
import com.mojang.blaze3d.vertex.BufferUploader;
import org.lwjgl.opengl.GL11;
import org.lwjgl.opengl.GL13;
import org.lwjgl.opengl.GL14;
import org.lwjgl.opengl.GL15;
import org.lwjgl.opengl.GL20;
import org.lwjgl.opengl.GL30;

/**
 * The GL state a draw inside Distant Horizons' pass changes, read raw (DH binds its program, vertex array
 * and buffers without going through Minecraft's {@link GlStateManager}) and put back through both, so DH finds its
 * state as it left it and vanilla's caches match what GL holds. Setting a cached value to something else and then
 * to the wanted value makes the second call reach GL whatever the cache held, which is how {@link #apply} keeps
 * the two in step.
 */
final class GlState {
    static final int UNITS = 4;
    private static final java.nio.ByteBuffer MASK = org.lwjgl.BufferUtils.createByteBuffer(16);

    int program, vao, arrayBuffer, drawFbo, readFbo, activeTexture;
    final int[] textures = new int[UNITS];
    boolean depthTest, depthMask, blend, cull;
    int depthFunc, blendSrcRgb, blendDstRgb, blendSrcAlpha, blendDstAlpha, blendEquation, cullFace;
    final int[] viewport = new int[4];
    final boolean[] colorMask = new boolean[4];

    /** Reads the state from GL. */
    GlState read() {
        program = GL11.glGetInteger(GL20.GL_CURRENT_PROGRAM);
        vao = GL11.glGetInteger(GL30.GL_VERTEX_ARRAY_BINDING);
        arrayBuffer = GL11.glGetInteger(GL15.GL_ARRAY_BUFFER_BINDING);
        drawFbo = GL11.glGetInteger(GL30.GL_DRAW_FRAMEBUFFER_BINDING);
        readFbo = GL11.glGetInteger(GL30.GL_READ_FRAMEBUFFER_BINDING);
        activeTexture = GL11.glGetInteger(GL13.GL_ACTIVE_TEXTURE);
        for (int u = 0; u < UNITS; u++) {
            GL13.glActiveTexture(GL13.GL_TEXTURE0 + u);
            textures[u] = GL11.glGetInteger(GL11.GL_TEXTURE_BINDING_2D);
        }
        GL13.glActiveTexture(activeTexture);
        depthTest = GL11.glIsEnabled(GL11.GL_DEPTH_TEST);
        depthMask = GL11.glGetBoolean(GL11.GL_DEPTH_WRITEMASK);
        depthFunc = GL11.glGetInteger(GL11.GL_DEPTH_FUNC);
        blend = GL11.glIsEnabled(GL11.GL_BLEND);
        blendSrcRgb = GL11.glGetInteger(GL14.GL_BLEND_SRC_RGB);
        blendDstRgb = GL11.glGetInteger(GL14.GL_BLEND_DST_RGB);
        blendSrcAlpha = GL11.glGetInteger(GL14.GL_BLEND_SRC_ALPHA);
        blendDstAlpha = GL11.glGetInteger(GL14.GL_BLEND_DST_ALPHA);
        blendEquation = GL11.glGetInteger(GL20.GL_BLEND_EQUATION_RGB);
        cull = GL11.glIsEnabled(GL11.GL_CULL_FACE);
        cullFace = GL11.glGetInteger(GL11.GL_CULL_FACE_MODE);
        GL11.glGetIntegerv(GL11.GL_VIEWPORT, viewport);
        java.nio.ByteBuffer b = MASK;
        b.clear();
        GL11.glGetBooleanv(GL11.GL_COLOR_WRITEMASK, b);
        for (int i = 0; i < 4; i++)
            colorMask[i] = b.get(i) != 0;
        return this;
    }

    /** Puts this state into GL and into vanilla's caches. */
    void apply() {
        BufferUploader.invalidate();
        GL30.glBindFramebuffer(GL30.GL_DRAW_FRAMEBUFFER, drawFbo);
        GL30.glBindFramebuffer(GL30.GL_READ_FRAMEBUFFER, readFbo);
        for (int u = 0; u < UNITS; u++) {
            activeTexture(GL13.GL_TEXTURE0 + u);
            if (textures[u] == 0)
                GL11.glBindTexture(GL11.GL_TEXTURE_2D, 0);
            GlStateManager._bindTexture(0);
            GlStateManager._bindTexture(textures[u]);
        }
        activeTexture(activeTexture);
        if (depthTest) {
            GlStateManager._disableDepthTest();
            GlStateManager._enableDepthTest();
        } else {
            GlStateManager._enableDepthTest();
            GlStateManager._disableDepthTest();
        }
        GlStateManager._depthMask(!depthMask);
        GlStateManager._depthMask(depthMask);
        GlStateManager._depthFunc(depthFunc == GL11.GL_LEQUAL ? GL11.GL_LESS : GL11.GL_LEQUAL);
        GlStateManager._depthFunc(depthFunc);
        if (blend) {
            GlStateManager._disableBlend();
            GlStateManager._enableBlend();
        } else {
            GlStateManager._enableBlend();
            GlStateManager._disableBlend();
        }
        GlStateManager._blendFuncSeparate(blendSrcRgb == GL11.GL_ZERO ? GL11.GL_ONE : GL11.GL_ZERO, GL11.GL_ZERO, GL11.GL_ZERO, GL11.GL_ZERO);
        GlStateManager._blendFuncSeparate(blendSrcRgb, blendDstRgb, blendSrcAlpha, blendDstAlpha);
        GL20.glBlendEquation(blendEquation);
        if (cull) {
            GlStateManager._disableCull();
            GlStateManager._enableCull();
        } else {
            GlStateManager._enableCull();
            GlStateManager._disableCull();
        }
        GL11.glCullFace(cullFace);
        GlStateManager._colorMask(!colorMask[0], !colorMask[1], !colorMask[2], !colorMask[3]);
        GlStateManager._colorMask(colorMask[0], colorMask[1], colorMask[2], colorMask[3]);
        GlStateManager._viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
        GlStateManager._glUseProgram(program);
        GlStateManager._glBindVertexArray(vao);
        GlStateManager._glBindBuffer(GL15.GL_ARRAY_BUFFER, arrayBuffer);
    }

    /** Makes vanilla's caches match GL as it is now, without changing GL (the first step of a draw). */
    static void syncCaches(GlState now) {
        now.apply();
    }

    private static void activeTexture(int unit) {
        GlStateManager._activeTexture(unit == GL13.GL_TEXTURE0 ? GL13.GL_TEXTURE1 : GL13.GL_TEXTURE0);
        GlStateManager._activeTexture(unit);
    }
}
