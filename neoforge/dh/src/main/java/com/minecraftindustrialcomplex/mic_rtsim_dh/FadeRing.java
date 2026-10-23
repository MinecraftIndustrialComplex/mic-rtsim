package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.seibel.distanthorizons.api.DhApi;
import net.minecraft.client.Minecraft;

/**
 * Where Distant Horizons starts fading vanilla's pixels into its own image (docs/choices-dh-feed.md DHT45).
 *
 * <p>DH 3.3.3's vanilla fade ({@code vanillaFadeMode} SINGLE_PASS or DOUBLE_PASS, DOUBLE_PASS by default) runs
 * after vanilla's entities and mixes every vanilla pixel that has depth toward DH's own colour there, by a
 * smoothstep of its 3D distance from the camera from {@code (near + 16) * 1.5} to {@code (near + 16) * 1.9}
 * blocks, {@code near} being DH's near clip plane ({@code RenderUtil.getNearClipPlaneInBlocks}: vanilla's render
 * distance times the overdraw share, over the frustum's corner factor, lowered when the camera moves fast).
 * At render distance 10 on a 16:9 screen that is about 134 to 170 blocks. So a far box the band renderer draws
 * there fades into whatever DH drew behind it: DH must draw the same box there too, or it fades to nothing.
 *
 * <p>Read once a frame, from DH itself; if that ever fails (another DH version), from the same formula with
 * the API's overdraw setting, leaving out the fast-camera lowering. Render thread only.
 */
final class FadeRing {
    /** The frustum corner factor's tangent: DH takes the field of view as 70 degrees, whatever it is. */
    private static final double TAN_HALF_FOV = Math.tan(Math.toRadians(35.0));

    private static double start = Double.POSITIVE_INFINITY;
    private static boolean fromDhFailed;

    private FadeRing() {}

    /** Reads this frame's ring (the DH controller, once a frame, before the views decide). */
    static void update() {
        float near = Float.NaN;
        if (!fromDhFailed) {
            try {
                near = com.seibel.distanthorizons.core.util.RenderUtil.getNearClipPlaneInBlocks();
            } catch (RuntimeException | LinkageError e) {
                fromDhFailed = true;
                MicRtsimDh.LOGGER.warn("mic_rtsim_dh: DH's near clip plane is not readable ({}); the fade ring "
                        + "is worked out without its fast-camera lowering", e.toString());
            }
        }
        if (!Float.isFinite(near))
            near = (float) nearFromApi();
        start = fadeStart(near);
    }

    /** The 3D distance from the camera where the fade starts, in blocks; infinite before the first frame. */
    static double start() {
        return start;
    }

    /** DH's fade start for a near clip plane of {@code near} blocks. */
    static double fadeStart(double near) {
        return (near + 16.0) * 1.5;
    }

    /** DH's near clip plane worked out as {@code RenderUtil} does, with the API's overdraw setting. */
    private static double nearFromApi() {
        Minecraft mc = Minecraft.getInstance();
        int rd = mc.options.getEffectiveRenderDistance();
        double overdraw = -1.0;
        try {
            Float o = DhApi.Delayed.configs.graphics().overdrawPreventionRadius().getValue();
            overdraw = o == null ? -1.0 : o;
        } catch (RuntimeException | LinkageError e) {
            // the automatic share below
        }
        if (overdraw < 0.0)
            overdraw = rd <= 2 ? 0.2 : rd <= 4 ? 0.3 : rd <= 6 ? 0.6 : rd <= 10 ? 0.8 : 0.9;
        else
            overdraw = Math.max(0.05, Math.min(1.0, overdraw));
        double plane = Math.max(1.0, rd * 16.0 * overdraw);
        int w = mc.getWindow().getWidth(), h = mc.getWindow().getHeight();
        double aspect = h > 0 ? (double) w / h : 16.0 / 9.0;
        return plane / Math.sqrt(1.0 + TAN_HALF_FOV * TAN_HALF_FOV * (aspect * aspect + 1.0));
    }
}
