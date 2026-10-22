package com.minecraftindustrialcomplex.mic_rtsim.far;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

/** The far-object registry: per-source replacement, snapshots, demand, and the mover's interpolation. */
class FarObjectsTest {
    private static FarMover mover(String source, int part, double x) {
        double[] prev = {x, 64, 0, x - 6, 64, 0};
        double[] cur = {x + 5, 64, 0, x - 1, 64, 0};
        return new FarMover(new FarKey(source, new UUID(0, 1), part), FarKind.CARRIAGE, "minecraft:overworld",
                prev, cur, 100.0, 5.0, 0x404040, false);
    }

    @AfterEach
    void reset() {
        FarObjects.clearAll();
        FarObjects.want("a", 0);
        FarObjects.want("b", 0);
    }

    @Test
    void commitReplacesOnlyThatSource() {
        FarObjects.commit("s1", List.of(mover("s1", 0, 0), mover("s1", 1, 10)));
        FarObjects.commit("s2", List.of(mover("s2", 0, 20)));
        assertEquals(3, FarObjects.movers().size());

        FarObjects.commit("s1", List.of(mover("s1", 0, 30)));
        assertEquals(2, FarObjects.movers().size());
        assertTrue(FarObjects.movers().stream().anyMatch(m -> m.key().source().equals("s2")));

        FarObjects.clear("s1");
        assertEquals(1, FarObjects.movers().size());
    }

    @Test
    void snapshotIsStableAndVersionMovesOnlyOnChange() {
        FarObjects.commit("s1", List.of(mover("s1", 0, 0)));
        List<FarMover> before = FarObjects.movers();
        long v = FarObjects.version();
        FarObjects.clear("nobody");
        assertEquals(v, FarObjects.version(), "clearing an empty source is no change");
        FarObjects.commit("s1", List.of(mover("s1", 0, 1), mover("s1", 1, 2)));
        assertEquals(1, before.size(), "an old snapshot never changes");
        assertTrue(FarObjects.version() > v);
    }

    @Test
    void aSourceCannotWriteAnotherSourcesKeys() {
        assertThrows(IllegalArgumentException.class, () -> FarObjects.commit("s1", List.of(mover("s2", 0, 0))));
    }

    @Test
    void demandIsTheLargestRadiusAndWithdraws() {
        assertEquals(0.0, FarObjects.wantedRadius());
        FarObjects.want("a", 512);
        FarObjects.want("b", 2048);
        assertEquals(2048.0, FarObjects.wantedRadius());
        FarObjects.want("b", 0);
        assertEquals(512.0, FarObjects.wantedRadius());
        FarObjects.want("a", -1);
        assertEquals(0.0, FarObjects.wantedRadius());
    }

    @Test
    void moverLerpsOverItsIntervalThenHolds() {
        FarMover m = mover("s1", 0, 0);
        double[] out = new double[6];
        m.segmentAt(90.0, out);
        assertArrayEquals(m.prev(), out, 1e-12, "before the sample: the older point");
        m.segmentAt(102.5, out);
        assertArrayEquals(new double[] {2.5, 64, 0, -3.5, 64, 0}, out, 1e-12, "half way");
        m.segmentAt(1000.0, out);
        assertArrayEquals(m.cur(), out, 1e-12, "after the interval: held at the newer point");
    }
}
