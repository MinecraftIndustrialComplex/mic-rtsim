package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.mojang.blaze3d.pipeline.RenderTarget;
import java.nio.ByteBuffer;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import org.joml.Matrix4f;
import org.lwjgl.opengl.GL11;
import org.lwjgl.opengl.GL15;
import org.lwjgl.opengl.GL21;
import org.lwjgl.opengl.GL30;

/**
 * A dev-only detector for the black far-view artifact (2026-10-24): each frame it reads a few rows of the
 * finished frame back through pixel buffers (a few frames late, so it never waits for the GPU), and when a row
 * has a run of pure (0, 0, 0) pixels it logs that frame with what the far view drew in it and the frames
 * around it. Off unless {@code -Dmic_rtsim.blackprobe=true}. Render thread.
 */
final class BlackProbe {
    static final boolean ENABLED = Boolean.getBoolean("mic_rtsim.blackprobe");
    /** Heights of the rows read, as fractions of the frame from its bottom (GL's rows). */
    private static final float[] AT = {0.97f, 0.9f, 0.8f, 0.7f, 0.6f, 0.5f, 0.4f, 0.3f};
    private static final int ROWS = AT.length, RING = 4, NOTES = 16;
    /** Pure black pixels in one row that make a hit. */
    private static final int MIN_BLACK = 24;
    private static final int[] PBO = new int[RING];
    private static final long[] PBO_FRAME = new long[RING];
    private static final String[] NOTE = new String[NOTES];
    private static final long[] NOTE_FRAME = new long[NOTES];
    private static final StringBuilder NOW = new StringBuilder();
    private static int pw, ph, errors;
    private static long read, hits, lastStatus, maxBlack;

    private BlackProbe() {}

    /** Adds a line to this frame's notes (only while enabled). */
    static void note(String line) {
        if (NOW.length() < 64 * 1024)
            NOW.append("\n    ").append(line);
    }

    static String m(Matrix4f m) {
        return String.format(Locale.ROOT, "[%.4g %.4g %.4g %.4g | %.4g %.4g %.4g %.4g | %.4g %.4g %.4g %.4g | %.4g %.4g %.4g %.4g]",
                m.m00(), m.m10(), m.m20(), m.m30(), m.m01(), m.m11(), m.m21(), m.m31(), m.m02(), m.m12(), m.m22(),
                m.m32(), m.m03(), m.m13(), m.m23(), m.m33());
    }

    /** RenderFrameEvent.Post: the frame is finished in the main target. */
    static void endFrame() {
        try {
            run();
        } catch (RuntimeException | LinkageError e) {
            if (errors++ < 3)
                MicRtsimDh.LOGGER.error("mic_rtsim_dh BLACKPROBE failed", e);
        }
    }

    private static void run() {
        long f = FarTrainView.frame();
        int n = (int) (f % NOTES);
        NOTE[n] = NOW.toString();
        NOTE_FRAME[n] = f;
        NOW.setLength(0);
        Minecraft mc = Minecraft.getInstance();
        if (mc.level == null)
            return;
        RenderTarget t = mc.getMainRenderTarget();
        int w = t.width, h = t.height;
        if (w != pw || h != ph) {
            for (int i = 0; i < RING; i++) {
                if (PBO[i] != 0)
                    GL15.glDeleteBuffers(PBO[i]);
                PBO[i] = GL15.glGenBuffers();
                GL15.glBindBuffer(GL21.GL_PIXEL_PACK_BUFFER, PBO[i]);
                GL15.glBufferData(GL21.GL_PIXEL_PACK_BUFFER, (long) ROWS * w * 4, GL15.GL_STREAM_READ);
                PBO_FRAME[i] = -1;
            }
            pw = w;
            ph = h;
        }
        int slot = (int) (f % RING);
        GL15.glBindBuffer(GL21.GL_PIXEL_PACK_BUFFER, PBO[slot]);
        try {
            if (PBO_FRAME[slot] >= 0)
                check(PBO_FRAME[slot], w);
            GL30.glBindFramebuffer(GL30.GL_READ_FRAMEBUFFER, t.frameBufferId);
            for (int r = 0; r < ROWS; r++)
                GL11.glReadPixels(0, Math.round(AT[r] * (h - 1)), w, 1, GL11.GL_RGBA, GL11.GL_UNSIGNED_BYTE,
                        (long) r * w * 4);
            PBO_FRAME[slot] = f;
        } finally {
            GL15.glBindBuffer(GL21.GL_PIXEL_PACK_BUFFER, 0);
        }
        long now = System.nanoTime();
        if (now - lastStatus > 10_000_000_000L) {
            lastStatus = now;
            MicRtsimDh.LOGGER.info("mic_rtsim_dh BLACKPROBE status: {} frames read, {} hits, most black in a row {}",
                    read, hits, maxBlack);
        }
    }

    private static void check(long frame, int w) {
        ByteBuffer b = GL30.glMapBufferRange(GL21.GL_PIXEL_PACK_BUFFER, 0, (long) ROWS * w * 4, GL30.GL_MAP_READ_BIT);
        if (b == null)
            return;
        int[] count = new int[ROWS], first = new int[ROWS], last = new int[ROWS];
        int most = 0;
        try {
            for (int r = 0; r < ROWS; r++) {
                first[r] = -1;
                for (int x = 0; x < w; x++) {
                    int o = (r * w + x) * 4;
                    if (b.get(o) == 0 && b.get(o + 1) == 0 && b.get(o + 2) == 0) {
                        count[r]++;
                        if (first[r] < 0)
                            first[r] = x;
                        last[r] = x;
                    }
                }
                most = Math.max(most, count[r]);
            }
        } finally {
            GL15.glUnmapBuffer(GL21.GL_PIXEL_PACK_BUFFER);
        }
        read++;
        maxBlack = Math.max(maxBlack, most);
        if (most < MIN_BLACK)
            return;
        hits++;
        if (hits > 60 && hits % 200 != 0)
            return;
        StringBuilder s = new StringBuilder();
        s.append(String.format(Locale.ROOT, "mic_rtsim_dh BLACKPROBE hit %d at frame %d (now %d); black per row, top down:",
                hits, frame, FarTrainView.frame()));
        for (int r = 0; r < ROWS; r++)
            s.append(String.format(Locale.ROOT, " %.2f:%d[%d-%d]", AT[r], count[r], first[r], last[r]));
        for (long g = frame - 2; g <= frame + 1; g++) {
            int k = (int) (g % NOTES);
            s.append("\n  frame ").append(g).append(g == frame ? " (hit)" : "").append(':');
            s.append(NOTE_FRAME[k] == g ? NOTE[k] : " (no notes)");
        }
        MicRtsimDh.LOGGER.info(s.toString());
    }
}
