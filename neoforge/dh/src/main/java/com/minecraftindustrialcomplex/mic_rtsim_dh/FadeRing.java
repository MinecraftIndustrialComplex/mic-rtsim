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
 * <p>DH leaves the fade out when {@code vanillaFadeMode} is NONE, when its renderer mode is not DEFAULT, and
 * while an Iris shader pack is in use ({@code ClientApi.shouldRenderFade}); then nothing fades and the ring is
 * nowhere ({@link #start()} infinite), so DH draws a far mover only outside the loaded chunks, as before the
 * fade hand-over.
 *
 * <p>Read once a frame, from DH itself; if that ever fails (another DH version), from the same formula with
 * the API's overdraw setting, leaving out the fast-camera lowering, and with the API's renderer mode and Iris's
 * own API for whether the fade runs. Render thread only.
 */
final class FadeRing {
    /** The frustum corner factor's tangent: DH takes the field of view as 70 degrees, whatever it is. */
    private static final double TAN_HALF_FOV = Math.tan(Math.toRadians(35.0));

    private static double start = Double.POSITIVE_INFINITY;
    private static boolean fromDhFailed;
    private static boolean fadeFromDhFailed;
    private static java.lang.reflect.Method irisInUse;
    private static Object iris;
    private static boolean irisLooked;

    private FadeRing() {}

    /** Reads this frame's ring (the DH controller, once a frame, before the views decide). */
    static void update() {
        if (!fadeRuns()) {
            start = Double.POSITIVE_INFINITY;
            return;
        }
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

    /** True when DH fades vanilla's pixels this frame (DH 3.3.3's {@code ClientApi.renderFadeTransparent}). */
    private static boolean fadeRuns() {
        if (!fadeFromDhFailed) {
            try {
                if (com.seibel.distanthorizons.core.config.Config.Client.Advanced.Debugging.rendererMode.get()
                        != com.seibel.distanthorizons.api.enums.rendering.EDhApiRendererMode.DEFAULT)
                    return false;
                if (com.seibel.distanthorizons.core.config.Config.Client.Advanced.Graphics.Quality.vanillaFadeMode.get()
                                == com.seibel.distanthorizons.api.enums.config.EDhApiMcRenderingFadeMode.NONE
                        && !Boolean.TRUE.equals(
                                com.seibel.distanthorizons.core.config.Config.Client.Advanced.Debugging.lodOnlyMode.get()))
                    return false;
                var shaders = com.seibel.distanthorizons.core.dependencyInjection.ModAccessorInjector.INSTANCE.get(
                        com.seibel.distanthorizons.core.wrapperInterfaces.modAccessor.IIrisAccessor.class);
                return shaders == null || !shaders.isShaderPackInUse();
            } catch (RuntimeException | LinkageError e) {
                fadeFromDhFailed = true;
                MicRtsimDh.LOGGER.warn("mic_rtsim_dh: DH's fade settings are not readable ({}); whether the fade "
                        + "runs is worked out from the API and Iris", e.toString());
            }
        }
        try {
            if (DhApi.Delayed.configs.graphics().renderingMode().getValue()
                    != com.seibel.distanthorizons.api.enums.rendering.EDhApiRendererMode.DEFAULT)
                return false;
        } catch (RuntimeException | LinkageError e) {
            // taken as DEFAULT
        }
        return !irisShaderPackInUse();
    }

    /** Iris's own API, looked up once by reflection: no compile-time Iris dependency. */
    private static boolean irisShaderPackInUse() {
        if (!irisLooked) {
            irisLooked = true;
            try {
                Class<?> api = Class.forName("net.irisshaders.iris.api.v0.IrisApi");
                iris = api.getMethod("getInstance").invoke(null);
                irisInUse = api.getMethod("isShaderPackInUse");
            } catch (ReflectiveOperationException | RuntimeException | LinkageError e) {
                iris = null; // no Iris
            }
        }
        if (iris == null)
            return false;
        try {
            return (Boolean) irisInUse.invoke(iris);
        } catch (ReflectiveOperationException | RuntimeException e) {
            return false;
        }
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
