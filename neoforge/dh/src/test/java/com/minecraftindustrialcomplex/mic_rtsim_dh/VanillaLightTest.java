package com.minecraftindustrialcomplex.mic_rtsim_dh;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** The band's boxes lit and fogged as vanilla's sums say (LightTexture.updateLightTexture, linear_fog). */
class VanillaLightTest {
    @Test
    void fullSkyLightIsBrightByDayAndBlueAtNight() {
        float[] day = new float[3], night = new float[3];
        // Vanilla's default brightness option is 0.5; getSkyDarken is 1.0 at noon and 0.2 at midnight
        VanillaLight.sky(1.0f, false, 0.5f, day);
        VanillaLight.sky(0.2f, false, 0.5f, night);
        for (int c = 0; c < 3; c++)
            assertEquals(0.985f, day[c], 0.002f, "day channel " + c);
        // Worked by hand through the lightmap's steps
        assertEquals(0.316f, night[0], 0.002f);
        assertEquals(0.316f, night[1], 0.002f);
        assertEquals(0.491f, night[2], 0.002f);
        float[] flash = new float[3];
        VanillaLight.sky(0.2f, true, 0.5f, flash);
        assertTrue(flash[0] > night[0], "lightning lights the sky");
    }

    @Test
    void fogFadesInBetweenItsStartAndEnd() {
        assertEquals(0.0f, VanillaLight.fog(100, 0, 0, 172.8f, 192, false));
        assertEquals(1.0f, VanillaLight.fog(200, 0, 0, 172.8f, 192, false));
        assertEquals(0.5f, VanillaLight.fog(0, 0, 182.4f, 172.8f, 192, false), 1e-4f);
        // The cylinder takes the larger of the level distance and the height; the sphere both
        assertEquals(0.0f, VanillaLight.fog(120, 120, 0, 172.8f, 192, false));
        assertEquals(1.0f, VanillaLight.fog(140, 140, 0, 172.8f, 192, true));
    }
}
