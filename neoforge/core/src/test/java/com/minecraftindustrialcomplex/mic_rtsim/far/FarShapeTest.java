package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Random;
import java.util.Set;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Far shapes (docs/choices-dh-feed.md DHT60 to DHT62): a carriage's layout round trips, is small, and a decoder
 * refuses anything malformed with IllegalArgumentException and nothing else; the feed's INTRO names the shape
 * and the age, and is sent again when either changes.
 */
class FarShapeTest {
    /** A coach: a floor of planks, walls of glass, a roof of slabs, two bogeys four blocks apart. */
    static CarriageShape coach() {
        List<CarriageShape.Block> blocks = new ArrayList<>();
        for (int x = -1; x <= 1; x++)
            for (int z = -1; z <= 5; z++) {
                blocks.add(new CarriageShape.Block(x, 1, z, 15));
                blocks.add(new CarriageShape.Block(x, 4, z, 11_234));
                if (x != 0)
                    for (int y = 2; y <= 3; y++)
                        blocks.add(new CarriageShape.Block(x, y, z, 5_120));
            }
        return new CarriageShape(3, blocks, List.of(
                new CarriageShape.Bogey(0, 0, 0, "create:standard", "create:small", false),
                new CarriageShape.Bogey(0, 0, 4, "create:standard", "create:large", true)));
    }

    @Test
    void aCarriageRoundTripsSmall() {
        CarriageShape c = coach();
        byte[] raw = c.encode();
        CarriageShape back = CarriageShape.decode(raw);
        assertEquals(c.assembly(), back.assembly());
        assertEquals(c.bogeys(), back.bogeys());
        assertEquals(sorted(c.blocks()), sorted(back.blocks()));
        FarShape s = c.shape();
        assertTrue(s.bytes().length < 80, s.toString());
        assertEquals(s.hash(), FarShape.hash(raw));
        assertArrayEquals(raw, FarShape.raw(s.hash(), s.bytes()));
        assertEquals(sorted(c.blocks()), sorted(CarriageShape.decode(s.hash(), s.bytes()).blocks()));
        // The same blocks in another order are the same shape
        List<CarriageShape.Block> shuffled = new ArrayList<>(c.blocks());
        java.util.Collections.shuffle(shuffled, new Random(7));
        assertEquals(s, new CarriageShape(3, shuffled, c.bogeys()).shape());
        assertNotEquals(s.hash(), new CarriageShape(2, c.blocks(), c.bogeys()).shape().hash());
    }

    @Test
    void edgeLayoutsRoundTrip() {
        CarriageShape empty = new CarriageShape(0, List.of(), List.of());
        assertEquals(empty, CarriageShape.decode(empty.encode()));
        CarriageShape one = new CarriageShape(5, List.of(new CarriageShape.Block(-40, 7, 1_000_000, Integer.MAX_VALUE)), List.of());
        assertEquals(one, CarriageShape.decode(one.encode()));
        // Two blocks at one place: the first is kept
        CarriageShape twice = new CarriageShape(1, List.of(new CarriageShape.Block(0, 0, 0, 1), new CarriageShape.Block(0, 0, 0, 2)),
                List.of());
        assertEquals(List.of(new CarriageShape.Block(0, 0, 0, 1)), CarriageShape.decode(twice.encode()).blocks());
    }

    @Test
    void limitsAreRefusedOnTheWayOut() {
        assertThrows(IllegalArgumentException.class, () -> new CarriageShape(6, List.of(), List.of()));
        CarriageShape wide = new CarriageShape(0, List.of(new CarriageShape.Block(0, 0, 0, 1),
                new CarriageShape.Block(CarriageShape.MAX_SIDE, 0, 0, 1)), List.of());
        assertThrows(IllegalArgumentException.class, wide::encode);
        CarriageShape longId = new CarriageShape(0, List.of(), List.of(
                new CarriageShape.Bogey(0, 0, 0, "x".repeat(CarriageShape.MAX_ID + 1), "s", false)));
        assertThrows(IllegalArgumentException.class, longId::encode);
    }

    @Test
    void everyTruncationAndBitFlipIsRefusedCleanly() {
        byte[] raw = coach().encode();
        for (int n = 0; n < raw.length; n++) {
            byte[] cut = Arrays.copyOf(raw, n);
            assertThrows(IllegalArgumentException.class, () -> CarriageShape.decode(cut), "cut at " + n);
        }
        assertThrows(IllegalArgumentException.class, () -> CarriageShape.decode(Arrays.copyOf(raw, raw.length + 1)));
        // A flipped bit decodes to another layout or is refused; it never throws anything else
        Random r = new Random(11);
        for (int k = 0; k < 2000; k++) {
            byte[] b = raw.clone();
            b[r.nextInt(b.length)] ^= (byte) (1 << r.nextInt(8));
            try {
                CarriageShape.decode(b);
            } catch (IllegalArgumentException expected) {
                // fine
            }
        }
    }

    @Test
    void aHostileLayoutIsRefusedNotAllocated() {
        // A huge palette or block count, or a gap that leaves the box
        FarBytes.Out o = new FarBytes.Out(16);
        o.u8(CarriageShape.VERSION);
        o.u8(0);
        o.var(Integer.MAX_VALUE);
        assertThrows(IllegalArgumentException.class, () -> CarriageShape.decode(o.done()));
        FarBytes.Out g = new FarBytes.Out(16);
        g.u8(CarriageShape.VERSION);
        g.u8(0);
        g.var(1);
        g.var(9);
        g.zig(0);
        g.zig(0);
        g.zig(0);
        g.var(2);
        g.var(2);
        g.var(2);
        g.var(1);
        g.var(8); // index 8 of a box of 8
        g.var(0);
        g.u8(0);
        assertThrows(IllegalArgumentException.class, () -> CarriageShape.decode(g.done()));
    }

    @Test
    void shapeBytesAreChecked() {
        FarShape s = coach().shape();
        assertThrows(IllegalArgumentException.class, () -> FarShape.raw(s.hash() ^ 1, s.bytes()), "another hash");
        assertThrows(IllegalArgumentException.class, () -> FarShape.raw(s.hash(), Arrays.copyOf(s.bytes(), s.bytes().length - 2)));
        assertThrows(IllegalArgumentException.class, () -> FarShape.raw(s.hash(), new byte[]{1, 2, 3, 4, 5}));
        // A bomb: a few kilobytes that inflate past the limit
        byte[] bomb = FarShape.deflate(new byte[FarShape.MAX_RAW + 1]);
        assertTrue(bomb.length < 4096);
        assertThrows(IllegalArgumentException.class, () -> FarShape.raw(1, bomb));
        assertNull(FarShape.of(new byte[FarShape.MAX_RAW + 1]), "too big to send");
        // Incompressible bytes past the wire's limit are not sent either
        byte[] noise = new byte[FarShape.MAX_BYTES + 100];
        new Random(3).nextBytes(noise);
        assertNull(FarShape.of(noise));
        assertNotEquals(0L, FarShape.hash(new byte[0]));
    }

    @Test
    void theIntroCarriesTheShapeAndTheAge() {
        FarWire.Intro i = new FarWire.Intro(3, 77L, 2, 0, 0, 0, 0x3A3B3F, 0x8000_0000_0000_0001L, -1);
        FarWire.Intro d = new FarWire.Intro(4, 78L, 1, 2, 3, 4, 0x4060C0, 0L, 113);
        FarWire.Message m = new FarWire.Message(0, 9L, "d", 0, 0, 0, List.of(i, d));
        assertEquals(m, FarWire.decode(FarWire.encode(m)));
        assertEquals(new FarWire.Intro(5, 1L, 1, 0, 0, 0, 0, 0L, -1), new FarWire.Intro(5, 1L, 1, 0, 0, 0, 0));
    }

    @Test
    void theViewSendsTheIntroAgainWhenTheShapeOrTheAgeChanges() {
        FarShape a = coach().shape(), b = new CarriageShape(0, coach().blocks(), List.of()).shape();
        FarView v = new FarView();
        List<FarWire.Rec> first = v.step(List.of(carriage(100, a, -1)));
        assertEquals(a.hash(), ((FarWire.Intro) first.get(0)).shape());
        assertTrue(v.step(List.of(carriage(100, a, -1))).isEmpty());
        List<FarWire.Rec> rebuilt = v.step(List.of(carriage(100, b, -1)));
        assertEquals(1, rebuilt.size());
        assertEquals(b.hash(), ((FarWire.Intro) rebuilt.get(0)).shape());
        assertEquals(5, ((FarWire.Intro) v.step(List.of(carriage(100, b, 5))).get(0)).age());
    }

    @Test
    void shapesGoNearestFirstOnceAndWithinTheSweepsBudget() {
        Random r = new Random(7);
        List<FarShape> big = new ArrayList<>();
        for (int i = 0; i < 4; i++) {
            byte[] raw = new byte[20_000];
            r.nextBytes(raw);
            big.add(FarShape.of(raw));
        }
        assertTrue(big.get(0).bytes().length > 19_000, "random bytes do not deflate");
        int budget = FarFeed.SHAPE_BYTES_PER_SWEEP;
        assertTrue(3 * big.get(0).bytes().length < budget && 4 * big.get(0).bytes().length > budget);
        Set<Long> sent = new HashSet<>();
        // The filter's order is nearest first; a second carriage of the first design is not sent again
        List<FarItem> chosen = List.of(carriage(1, big.get(0), -1), carriage(2, big.get(0), -1),
                carriage(3, big.get(1), -1), carriage(4, null, -1), carriage(5, big.get(2), -1),
                carriage(6, big.get(3), -1));
        assertEquals(big.subList(0, 3), FarShape.pick(chosen, sent, budget));
        assertEquals(List.of(big.get(3)), FarShape.pick(chosen, sent, budget));
        assertTrue(FarShape.pick(chosen, sent, budget).isEmpty());
        assertEquals(4, sent.size());
        // A RESET empties what the subscriber holds, and they go again
        sent.clear();
        assertEquals(big.subList(0, 3), FarShape.pick(chosen, sent, budget));
        // One bigger than the whole budget goes alone
        assertEquals(List.of(big.get(3)), FarShape.pick(chosen, sent, 100));
    }

    private static FarItem carriage(long id, FarShape shape, int age) {
        return new FarItem(id, FarKind.CARRIAGE, 0, 0, 0, 0x3A3B3F, FarState.MOVING, new double[]{503, 64, 0, 497, 64, 0},
                null, -1, 0, shape, age);
    }

    private static List<CarriageShape.Block> sorted(List<CarriageShape.Block> b) {
        List<CarriageShape.Block> s = new ArrayList<>(b);
        s.sort((p, q) -> p.y() != q.y() ? Integer.compare(p.y(), q.y()) : p.z() != q.z() ? Integer.compare(p.z(), q.z())
                : Integer.compare(p.x(), q.x()));
        return s;
    }
}
