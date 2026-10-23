package com.minecraftindustrialcomplex.mic_rtsim.client;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;

/** A far feed RESET starts the looks over (docs/choices-dh-feed.md DHT70), and leaving the world clears them. */
class FarLooksTest {
    /** A look that counts what it was told. */
    private static class Counting implements FarLook {
        int clears, resets;

        @Override
        public Status status(FarMover m) {
            return Status.NONE;
        }

        @Override
        public boolean place(FarMover m, double[] seg, double clock, boolean live, FarModel out) {
            return false;
        }

        @Override
        public void frame(long now, long deadline) {
        }

        @Override
        public void clear() {
            clears++;
        }

        @Override
        public String stats() {
            return "";
        }
    }

    /** One that keeps its prewarm across a reset, as the dragons' and carriages' do. */
    private static final class Keeping extends Counting {
        @Override
        public void reset() {
            resets++;
        }
    }

    @AfterEach
    void unregister() {
        FarLooks.register(FarKind.DRAGON, null);
        FarLooks.register(FarKind.CARRIAGE, null);
    }

    @Test
    void aResetResetsEveryLookAndClearsOnlyThoseWithNoResetOfTheirOwn() {
        Counting plain = new Counting();
        Keeping keeping = new Keeping();
        FarLooks.register(FarKind.DRAGON, keeping);
        FarLooks.register(FarKind.CARRIAGE, plain);
        FarLooks.resetAll();
        assertEquals(1, keeping.resets);
        assertEquals(0, keeping.clears, "a look with a reset of its own keeps the rest");
        assertEquals(1, plain.clears, "by default a reset is a clear");
        FarLooks.clearAll();
        assertEquals(1, keeping.clears);
        assertEquals(1, keeping.resets);
    }

    @Test
    void aLookThatThrowsOnResetDoesNotStopTheOthers() {
        Keeping after = new Keeping();
        FarLooks.register(FarKind.DRAGON, new Counting() {
            @Override
            public void reset() {
                throw new IllegalStateException("broken");
            }
        });
        FarLooks.register(FarKind.CARRIAGE, after);
        FarLooks.resetAll();
        assertEquals(1, after.resets);
    }
}
