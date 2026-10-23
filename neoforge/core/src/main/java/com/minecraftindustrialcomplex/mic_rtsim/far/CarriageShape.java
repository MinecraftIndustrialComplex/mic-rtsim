package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A Create carriage's look, the raw layout of a carriage's {@link FarShape} (docs/boundary.md §12.2,
 * docs/choices-dh-feed.md DHT61): its blocks as Create keeps them ({@code Contraption.Blocks}: a palette and a
 * block list, in the contraption's own coordinates), without what a far mesh does not draw (block entity data,
 * actors, storage, seats, glue), and its bogeys. Plain Java, so the codec is unit tested; mic_rtsim_create
 * makes it from a carriage on the server and bakes it into a mesh on the client.
 *
 * <pre>
 *   u8 version, u8 assembly (a Direction's 3D data value), varint palette size, palette (varint global block
 *   state ids), zig min x y z, varint size x y z, varint block count,
 *   per block in (y, z, x) order: varint gap in the box's linear index since the last block, varint palette index,
 *   u8 bogey count, per bogey: zig x y z, string style id, string size id, u8 upside down
 * </pre>
 * Block state ids are the global ids chunk packets use: they hold for one server and its mod list, which is
 * what a client joined to that server shares.
 */
public record CarriageShape(int assembly, List<Block> blocks, List<Bogey> bogeys) {
    public static final int VERSION = 1;
    /** Limits a decoder holds a layout to (a malformed or hostile one is refused, not allocated). */
    public static final int MAX_PALETTE = 4096, MAX_SIDE = 1024, MAX_BLOCKS = 1 << 16, MAX_BOGEYS = 4, MAX_ID = 256;

    /** One block at (x, y, z), contraption coordinates, of global block state id {@code state}. */
    public record Block(int x, int y, int z, int state) {
    }

    /** A bogey at (x, y, z), contraption coordinates; {@code style} and {@code size} are Create's ids. */
    public record Bogey(int x, int y, int z, String style, String size, boolean upsideDown) {
    }

    public CarriageShape {
        if (assembly < 0 || assembly > 5)
            throw new IllegalArgumentException("an assembly direction is 0 to 5, got " + assembly);
        blocks = List.copyOf(blocks);
        bogeys = List.copyOf(bogeys);
    }

    /** The far shape of this layout, or null when it is too big to send. */
    public FarShape shape() {
        return FarShape.of(encode());
    }

    /**
     * The raw layout. Blocks at the same place are kept once (the first). Throws IllegalArgumentException past
     * the limits (the source then sends no shape, and the carriage is a box).
     */
    public byte[] encode() {
        if (bogeys.size() > MAX_BOGEYS)
            throw new IllegalArgumentException(bogeys.size() + " bogeys, at most " + MAX_BOGEYS);
        int minX = 0, minY = 0, minZ = 0, maxX = 0, maxY = 0, maxZ = 0;
        if (!blocks.isEmpty()) {
            minX = minY = minZ = Integer.MAX_VALUE;
            maxX = maxY = maxZ = Integer.MIN_VALUE;
        }
        for (Block k : blocks) {
            minX = Math.min(minX, k.x);
            minY = Math.min(minY, k.y);
            minZ = Math.min(minZ, k.z);
            maxX = Math.max(maxX, k.x);
            maxY = Math.max(maxY, k.y);
            maxZ = Math.max(maxZ, k.z);
        }
        long lx = (long) maxX - minX + 1, ly = (long) maxY - minY + 1, lz = (long) maxZ - minZ + 1;
        if (lx > MAX_SIDE || ly > MAX_SIDE || lz > MAX_SIDE)
            throw new IllegalArgumentException("a carriage of " + lx + " by " + ly + " by " + lz + ", at most " + MAX_SIDE);
        int sx = (int) lx, sz = (int) lz;
        final int fx = minX, fy = minY, fz = minZ;
        List<Block> sorted = new ArrayList<>(blocks);
        sorted.sort(Comparator.comparingLong(k -> index(k.x - fx, k.y - fy, k.z - fz, sx, sz)));
        List<Block> unique = new ArrayList<>(sorted.size());
        long prev = -1;
        for (Block k : sorted) {
            long i = index(k.x - fx, k.y - fy, k.z - fz, sx, sz);
            if (i != prev)
                unique.add(k);
            prev = i;
        }
        if (unique.size() > MAX_BLOCKS)
            throw new IllegalArgumentException(unique.size() + " blocks, at most " + MAX_BLOCKS);
        Map<Integer, Integer> palette = new LinkedHashMap<>();
        for (Block k : unique)
            palette.putIfAbsent(k.state, palette.size());
        if (palette.size() > MAX_PALETTE)
            throw new IllegalArgumentException(palette.size() + " block states, at most " + MAX_PALETTE);
        FarBytes.Out o = new FarBytes.Out(32 + unique.size() * 2 + palette.size() * 3);
        o.u8(VERSION);
        o.u8(assembly);
        o.var(palette.size());
        for (int id : palette.keySet())
            o.var(id & 0xFFFF_FFFFL);
        o.zig(minX);
        o.zig(minY);
        o.zig(minZ);
        o.var(lx);
        o.var(ly);
        o.var(lz);
        o.var(unique.size());
        long last = -1;
        for (Block k : unique) {
            long i = index(k.x - fx, k.y - fy, k.z - fz, sx, sz);
            o.var(i - last - 1);
            last = i;
            o.var(palette.get(k.state));
        }
        o.u8(bogeys.size());
        for (Bogey g : bogeys) {
            o.zig(g.x);
            o.zig(g.y);
            o.zig(g.z);
            o.utf(id(g.style));
            o.utf(id(g.size));
            o.u8(g.upsideDown ? 1 : 0);
        }
        return o.done();
    }

    /** Reads a raw layout; throws IllegalArgumentException when it is not one, or past the limits. */
    public static CarriageShape decode(byte[] raw) {
        FarBytes.In in = new FarBytes.In(raw, "a carriage shape");
        int version = in.u8();
        if (version != VERSION)
            throw new IllegalArgumentException("carriage shape version " + version + ", this side reads " + VERSION);
        int assembly = in.u8();
        if (assembly > 5)
            throw new IllegalArgumentException("an assembly direction of " + assembly);
        int[] palette = new int[in.len(MAX_PALETTE, "block states")];
        for (int i = 0; i < palette.length; i++) {
            long id = in.var();
            if (id > Integer.MAX_VALUE)
                throw new IllegalArgumentException("a block state id of " + id);
            palette[i] = (int) id;
        }
        int minX = in.zigInt(), minY = in.zigInt(), minZ = in.zigInt();
        int sx = in.len(MAX_SIDE, "blocks wide"), sy = in.len(MAX_SIDE, "blocks high"), sz = in.len(MAX_SIDE, "blocks long");
        if (sx == 0 || sy == 0 || sz == 0)
            throw new IllegalArgumentException("a carriage box of " + sx + " by " + sy + " by " + sz);
        long volume = (long) sx * sy * sz;
        int n = in.len(MAX_BLOCKS, "blocks");
        if (n > volume)
            throw new IllegalArgumentException(n + " blocks in a box of " + volume);
        List<Block> blocks = new ArrayList<>(n);
        long i = -1;
        for (int k = 0; k < n; k++) {
            long gap = in.var();
            if (gap < 0 || gap >= volume - i - 1)
                throw new IllegalArgumentException("block " + k + " is outside its box");
            i += gap + 1;
            int p = in.len();
            if (p >= palette.length)
                throw new IllegalArgumentException("block " + k + " has palette index " + p + " of " + palette.length);
            int x = (int) (i % sx), z = (int) ((i / sx) % sz), y = (int) (i / ((long) sx * sz));
            blocks.add(new Block(minX + x, minY + y, minZ + z, palette[p]));
        }
        int nb = in.u8();
        if (nb > MAX_BOGEYS)
            throw new IllegalArgumentException(nb + " bogeys, at most " + MAX_BOGEYS);
        List<Bogey> bogeys = new ArrayList<>(nb);
        for (int k = 0; k < nb; k++) {
            int x = in.zigInt(), y = in.zigInt(), z = in.zigInt();
            String style = in.utf(MAX_ID), size = in.utf(MAX_ID);
            int flip = in.u8();
            if (flip > 1)
                throw new IllegalArgumentException("a bogey's upside-down flag of " + flip);
            bogeys.add(new Bogey(x, y, z, style, size, flip == 1));
        }
        if (!in.done())
            throw new IllegalArgumentException(in.left() + " bytes after a carriage shape");
        return new CarriageShape(assembly, blocks, bogeys);
    }

    /** The shape from its payload bytes ({@link FarShape#raw}). */
    public static CarriageShape decode(long hash, byte[] deflated) {
        return decode(FarShape.raw(hash, deflated));
    }

    /** The global block state ids it uses, once each. */
    public int[] states() {
        return blocks.stream().mapToInt(Block::state).distinct().toArray();
    }

    private static long index(int x, int y, int z, int sx, int sz) {
        return ((long) y * sz + z) * sx + x;
    }

    private static String id(String s) {
        if (s.getBytes(java.nio.charset.StandardCharsets.UTF_8).length > MAX_ID)
            throw new IllegalArgumentException("a bogey id of more than " + MAX_ID + " bytes: " + s);
        return s;
    }
}
