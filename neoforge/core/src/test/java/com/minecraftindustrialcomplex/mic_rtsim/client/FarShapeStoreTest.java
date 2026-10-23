package com.minecraftindustrialcomplex.mic_rtsim.client;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** The client's far shapes (docs/choices-dh-feed.md DHT62): kept by hash, bounded, emptied on a RESET. */
class FarShapeStoreTest {
    @Test
    void shapesAreKeptBoundedAndCleared() {
        FarShapeStore s = new FarShapeStore();
        assertFalse(s.put(0L, new byte[3]), "0 is no shape");
        assertTrue(s.put(5L, new byte[]{1, 2}));
        assertArrayEquals(new byte[]{1, 2}, s.get(5L));
        assertTrue(s.put(5L, new byte[]{3}), "sent again after a RESET the client missed: replaced");
        assertEquals(1, s.bytes());
        for (long h = 1; s.size() < FarShapeStore.MAX_SHAPES; h += 2)
            assertTrue(s.put(h + 100, new byte[1]));
        assertFalse(s.put(4L, new byte[1]), "past the count");
        assertEquals(1, s.dropped);
        assertTrue(s.put(5L, new byte[2]), "replacing one is not a new one");
        s.clear();
        assertNull(s.get(5L));
        assertEquals(0, s.bytes());
        assertFalse(s.put(6L, new byte[FarShapeStore.MAX_BYTES + 1]), "past the bytes");
    }

    @Test
    void manyShapesFillTheBytesAndNoMore() {
        FarShapeStore s = new FarShapeStore();
        byte[] mib = new byte[1 << 20];
        int n = FarShapeStore.MAX_BYTES / mib.length;
        for (long h = 1; h <= n; h++)
            assertTrue(s.put(h, mib), "shape " + h);
        assertEquals((long) n * mib.length, s.bytes());
        assertFalse(s.put(n + 1L, new byte[1]), "one byte past");
        assertTrue(s.put(1L, new byte[1]), "a smaller copy of a held one fits");
        assertTrue(s.put(n + 1L, new byte[1]), "and so does one more now");
    }
}
