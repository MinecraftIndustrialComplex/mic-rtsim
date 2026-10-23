package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * The far feed's message, server to client (payload {@code mic_rtsim:far}, docs/boundary.md §12.2): what one
 * subscriber's far view gained, changed and lost since the last message. Plain Java (no Minecraft class), so
 * it is unit tested as such.
 *
 * <p>A message is a header and records. Objects are named by a small {@code handle} the server gives each one
 * per subscriber in its {@link Intro}; every later record of that object uses the handle. Positions are
 * relative to the header's origin (the subscriber's block when the message was made), so they stay a few bytes
 * long: legs in whole blocks, samples in eighths of a block, a sample's trailing point relative to its leading
 * one. Times are game time, relative to the header's.
 *
 * <pre>
 *   u8 version, varint flags, varlong gameTime, string dimension, zigzag ox, oy, oz, varint count, records
 *   INTRO  1: handle, i64 id, u8 kind, u8 style, u8 variant, u8 size, varint rgb, i64 shape, zig age
 *   LEG    2: handle, u8 state, zig y-oy, zig x0-ox, zig z0-oz, zig x1-ox, zig z1-oz, zig t0-gameTime,
 *             varlong t1-t0, varint radius
 *   SAMPLE 3: handle, u8 state, zig lx-8ox, zig ly-8oy, zig lz-8oz, zig tx-lx, zig ty-ly, zig tz-lz
 *   REAL   4: handle, zig entity (-1: none)
 *   GONE   5: handle
 * </pre>
 * Handles, counts and lengths are unsigned LEB128 varints; {@code zig} is a zigzag varint.
 *
 * <p>Version 2 (docs/choices-dh-feed.md DHT60) added the INTRO's {@code shape}, the content hash of what the
 * object looks like ({@link FarShape}, 0 for none: drawn as a box), whose bytes go out in their own payload,
 * and its {@code age} (a dragon's age in days, for its size between stages; -1 when the source does not say).
 */
public final class FarWire {
    public static final int VERSION = 2;
    /** A message flag: forget every object of the feed before applying this one (a new view). */
    public static final int RESET = 1;
    /** Sample coordinates are in eighths of a block. */
    public static final int SAMPLE_SCALE = 8;

    public static final int INTRO = 1, LEG = 2, SAMPLE = 3, REAL = 4, GONE = 5;

    private FarWire() {}

    /** One record of a message. */
    public sealed interface Rec permits Intro, Leg, Sample, Real, Gone {
        int handle();
    }

    /**
     * A new object: what it is. Its position follows in a LEG or a SAMPLE of the same message. {@code shape} is
     * its look's content hash ({@link FarShape}), 0 for none; {@code age} its age in days, -1 for unknown.
     */
    public record Intro(int handle, long id, int kind, int style, int variant, int size, int rgb, long shape, int age)
            implements Rec {
        /** An object with no shape and no age. */
        public Intro(int handle, long id, int kind, int style, int variant, int size, int rgb) {
            this(handle, id, kind, style, variant, size, rgb, 0L, -1);
        }
    }

    /** A leg in game time: from (x0, y, z0) at t0 to (x1, y, z1) at t1; a hunt circles (x0, z0) at radius. */
    public record Leg(int handle, int state, int y, int x0, int z0, int x1, int z1, long t0, long t1, int radius)
            implements Rec {
    }

    /** Where a sampled object is now, in eighths of a block ({@link #quantise}): leading then trailing. */
    public record Sample(int handle, int state, int lx, int ly, int lz, int tx, int ty, int tz) implements Rec {
        public static Sample of(int handle, int state, double[] seg) {
            return new Sample(handle, state, quantise(seg[0]), quantise(seg[1]), quantise(seg[2]),
                    quantise(seg[3]), quantise(seg[4]), quantise(seg[5]));
        }

        /** The segment in blocks (6 values). */
        public double[] segment() {
            return new double[]{lx / 8.0, ly / 8.0, lz / 8.0, tx / 8.0, ty / 8.0, tz / 8.0};
        }
    }

    /** The real entity's network id on the client, or -1 when there is none. */
    public record Real(int handle, int entity) implements Rec {
    }

    /** The object left the subscriber's view: forget its handle. */
    public record Gone(int handle) implements Rec {
    }

    /** A whole message. */
    public record Message(int flags, long gameTime, String dimension, int ox, int oy, int oz, List<Rec> recs) {
        public Message {
            recs = List.copyOf(recs);
        }
    }

    /** A coordinate in eighths of a block, rounded. */
    public static int quantise(double v) {
        return (int) Math.round(v * SAMPLE_SCALE);
    }

    // ---------------------------------------------------------------------------------------------------------
    // Encoding

    public static byte[] encode(Message m) {
        FarBytes.Out o = new FarBytes.Out(32 + m.recs().size() * 24);
        o.u8(VERSION);
        o.var(m.flags());
        o.var(m.gameTime());
        byte[] dim = m.dimension().getBytes(StandardCharsets.UTF_8);
        o.var(dim.length);
        o.bytes(dim);
        o.zig(m.ox());
        o.zig(m.oy());
        o.zig(m.oz());
        o.var(m.recs().size());
        long ox8 = (long) m.ox() * SAMPLE_SCALE, oy8 = (long) m.oy() * SAMPLE_SCALE, oz8 = (long) m.oz() * SAMPLE_SCALE;
        for (Rec r : m.recs()) {
            switch (r) {
                case Intro i -> {
                    o.u8(INTRO);
                    o.var(i.handle());
                    o.i64(i.id());
                    o.u8(i.kind());
                    o.u8(i.style());
                    o.u8(i.variant());
                    o.u8(i.size());
                    o.var(i.rgb() & 0xFFFF_FFFFL);
                    o.i64(i.shape());
                    o.zig(i.age());
                }
                case Leg l -> {
                    o.u8(LEG);
                    o.var(l.handle());
                    o.u8(l.state());
                    o.zig((long) l.y() - m.oy());
                    o.zig((long) l.x0() - m.ox());
                    o.zig((long) l.z0() - m.oz());
                    o.zig((long) l.x1() - m.ox());
                    o.zig((long) l.z1() - m.oz());
                    o.zig(l.t0() - m.gameTime());
                    o.var(Math.max(0L, l.t1() - l.t0()));
                    o.var(Math.max(0, l.radius()));
                }
                case Sample s -> {
                    o.u8(SAMPLE);
                    o.var(s.handle());
                    o.u8(s.state());
                    o.zig(s.lx() - ox8);
                    o.zig(s.ly() - oy8);
                    o.zig(s.lz() - oz8);
                    o.zig((long) s.tx() - s.lx());
                    o.zig((long) s.ty() - s.ly());
                    o.zig((long) s.tz() - s.lz());
                }
                case Real e -> {
                    o.u8(REAL);
                    o.var(e.handle());
                    o.zig(e.entity());
                }
                case Gone g -> {
                    o.u8(GONE);
                    o.var(g.handle());
                }
            }
        }
        return o.done();
    }

    // ---------------------------------------------------------------------------------------------------------
    // Decoding

    /** Reads a message; throws IllegalArgumentException when it is not one (a short or unknown record). */
    public static Message decode(byte[] b) {
        FarBytes.In in = new FarBytes.In(b, "the far feed message");
        int version = in.u8();
        if (version != VERSION)
            throw new IllegalArgumentException("far feed version " + version + ", this side reads " + VERSION);
        int flags = (int) in.var();
        long gameTime = in.var();
        int dimLen = in.len();
        String dimension = new String(in.bytes(dimLen), StandardCharsets.UTF_8);
        int ox = (int) in.zig(), oy = (int) in.zig(), oz = (int) in.zig();
        int count = in.len();
        long ox8 = (long) ox * SAMPLE_SCALE, oy8 = (long) oy * SAMPLE_SCALE, oz8 = (long) oz * SAMPLE_SCALE;
        List<Rec> recs = new ArrayList<>(Math.min(count, 4096));
        for (int k = 0; k < count; k++) {
            int type = in.u8();
            int handle = in.len();
            recs.add(switch (type) {
                case INTRO -> new Intro(handle, in.i64(), in.u8(), in.u8(), in.u8(), in.u8(), (int) in.var(), in.i64(),
                        in.zigInt());
                case LEG -> {
                    int state = in.u8();
                    int y = (int) (in.zig() + oy);
                    int x0 = (int) (in.zig() + ox), z0 = (int) (in.zig() + oz);
                    int x1 = (int) (in.zig() + ox), z1 = (int) (in.zig() + oz);
                    long t0 = in.zig() + gameTime;
                    long t1 = t0 + in.var();
                    yield new Leg(handle, state, y, x0, z0, x1, z1, t0, t1, (int) in.var());
                }
                case SAMPLE -> {
                    int state = in.u8();
                    int lx = (int) (in.zig() + ox8), ly = (int) (in.zig() + oy8), lz = (int) (in.zig() + oz8);
                    int tx = (int) (in.zig() + lx), ty = (int) (in.zig() + ly), tz = (int) (in.zig() + lz);
                    yield new Sample(handle, state, lx, ly, lz, tx, ty, tz);
                }
                case REAL -> new Real(handle, (int) in.zig());
                case GONE -> new Gone(handle);
                default -> throw new IllegalArgumentException("far feed record type " + type + " at record " + k);
            });
        }
        if (!in.done())
            throw new IllegalArgumentException(in.left() + " bytes after the far feed's last record");
        return new Message(flags, gameTime, dimension, ox, oy, oz, recs);
    }
}
