package com.minecraftindustrialcomplex.mic_rtsim_dh;

/**
 * Vanilla's light and fog sums, for the boxes the band renderer draws ({@link FarBandRenderer}) to match DH's and
 * the terrain's. Plain Java, so it is unit tested.
 */
final class VanillaLight {
    private VanillaLight() {}

    /**
     * Vanilla's lightmap at full sky light and no block light (LightTexture.updateLightTexture's sums), which is
     * what DH's box shader multiplies its colours by. Left out: night vision, the darkness effect, a boss
     * darkening the world and a dimension's own lightmap adjustments; a lamp beside a band box does not light it.
     */
    static void sky(float skyDarken, boolean flash, float gamma, float[] out) {
        float sky = flash ? 1.0f : skyDarken * 0.95f + 0.05f;
        for (int c = 0; c < 3; c++) {
            float tint = c == 2 ? 1.0f : skyDarken + (1.0f - skyDarken) * 0.35f;
            float x = clamp(tint * sky + (0.75f - tint * sky) * 0.04f);
            float inv = 1.0f - x;
            float bright = 1.0f - inv * inv * inv * inv;
            x += (bright - x) * Math.max(0.0f, gamma);
            out[c] = clamp(x + (0.75f - x) * 0.04f);
        }
    }

    private static float clamp(float x) {
        return Math.max(0.0f, Math.min(1.0f, x));
    }

    /** How far into vanilla's fog a point this far off is (its shaders' linear_fog and fog_distance). */
    static float fog(float dx, float dy, float dz, float start, float end, boolean sphere) {
        float d = sphere ? (float) Math.sqrt(dx * dx + dy * dy + dz * dz)
                : Math.max((float) Math.sqrt(dx * dx + dz * dz), Math.abs(dy));
        if (d <= start)
            return 0.0f;
        if (d >= end)
            return 1.0f;
        float t = (d - start) / (end - start);
        return t * t * (3.0f - 2.0f * t);
    }
}
