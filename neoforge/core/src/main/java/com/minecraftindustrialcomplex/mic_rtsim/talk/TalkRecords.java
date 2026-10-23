package com.minecraftindustrialcomplex.mic_rtsim.talk;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.List;

/**
 * TALK's two records (docs/boundary.md §13), offsets from the start of the record. Mirrors
 * {@code crates/rtsim-jni/src/talk_wire.rs} (the reference codec): the bytes of one ask and one facts
 * record are pinned on both sides ({@code TalkRecordsTest} here). Pure: no Minecraft types, so a unit
 * test loads it.
 *
 * <ul>
 *   <li><b>TALK_ASK</b> (0x00C0, Java to the engine, latest wins, aux = n): Java's open questions,
 *       {@code n} asks of {@link Ask#SIZE} bytes from offset 8.</li>
 *   <li><b>TALK_FACTS</b> (0x8A00, engine to Java, aux = news n): one answer, 48 bytes of facts and then
 *       {@code n} news items of {@link News#SIZE} bytes, each with three refs of {@link Ref#SIZE} bytes.</li>
 * </ul>
 *
 * Every value is little-endian. A decode of a record that is too short for its own {@code n}, or that
 * names more news than the engine ever sends, returns null: it never throws.
 */
public final class TalkRecords {
    private TalkRecords() {}

    public static final int TALK_ASK = 0x00C0;
    public static final int TALK_FACTS = 0x8A00;

    public static boolean isTalkCommand(int kind) {
        return kind >= 0x8A00 && kind <= 0x8AFF;
    }

    /** The record header: u16 kind, u16 size, u32 aux. */
    public static final int HEADER = 8;
    private static final int R_KIND = 0, R_SIZE = 2, R_AUX = 4;

    // ---- ask ------------------------------------------------------------------------------------

    /** Holder kinds of an ask. */
    public static final int HOLDER_VILLAGE = 1, HOLDER_CARAVAN = 2, HOLDER_FACTION = 3, HOLDER_SHIP = 4;
    /** Ask flags: 1 news, 2 faction facts (slice 4). */
    public static final int ASK_NEWS = 1, ASK_FACTION = 2;

    /**
     * One ask. {@code holder} is a village's Java id, a puppet's agent or a camp's key; {@code dimension}
     * is Java's dictionary id of the talker's dimension; {@code x} and {@code z} are the talker's block.
     */
    public record Ask(long id, long playerMost, long playerLeast, int holderKind, int dimension, long holder,
                      int x, int z, int flags) {
        public static final int FIRST = 8;
        public static final int SIZE = 56;
        public static final int ID = 0, PLAYER_MOST = 8, PLAYER_LEAST = 16, HOLDER_KIND = 24, DIMENSION = 28,
                HOLDER = 32, X = 40, Z = 44, FLAGS = 48, RESERVED = 52;
    }

    /** The size of a whole TALK_ASK record of n asks. */
    public static int askRecordSize(int n) {
        return Ask.FIRST + n * Ask.SIZE;
    }

    /** Writes one ask at {@code at} (the start of the ask, not of the record) with absolute puts. */
    public static void writeAsk(ByteBuffer b, int at, Ask a) {
        b.putLong(at + Ask.ID, a.id);
        b.putLong(at + Ask.PLAYER_MOST, a.playerMost);
        b.putLong(at + Ask.PLAYER_LEAST, a.playerLeast);
        b.putInt(at + Ask.HOLDER_KIND, a.holderKind);
        b.putInt(at + Ask.DIMENSION, a.dimension);
        b.putLong(at + Ask.HOLDER, a.holder);
        b.putInt(at + Ask.X, a.x);
        b.putInt(at + Ask.Z, a.z);
        b.putInt(at + Ask.FLAGS, a.flags);
        b.putInt(at + Ask.RESERVED, 0);
    }

    /** A whole TALK_ASK record of these asks (what the plugin writes through the boundary; for tests and tools). */
    public static byte[] encodeAsks(List<Ask> asks) {
        byte[] rec = new byte[(askRecordSize(asks.size()) + 7) & ~7];
        ByteBuffer b = ByteBuffer.wrap(rec).order(ByteOrder.LITTLE_ENDIAN);
        b.putShort(R_KIND, (short) TALK_ASK);
        b.putShort(R_SIZE, (short) rec.length);
        b.putInt(R_AUX, asks.size());
        for (int i = 0; i < asks.size(); i++)
            writeAsk(b, Ask.FIRST + i * Ask.SIZE, asks.get(i));
        return rec;
    }

    /** Decodes a whole TALK_ASK record into its asks; null when it is not one or is too short for its n. */
    public static List<Ask> decodeAsks(byte[] rec) {
        if (rec == null || rec.length < HEADER)
            return null;
        ByteBuffer b = ByteBuffer.wrap(rec).order(ByteOrder.LITTLE_ENDIAN);
        if (Short.toUnsignedInt(b.getShort(R_KIND)) != TALK_ASK)
            return null;
        long n = Integer.toUnsignedLong(b.getInt(R_AUX));
        if (n > (rec.length - Ask.FIRST) / Ask.SIZE)
            return null;
        List<Ask> out = new ArrayList<>((int) n);
        for (int i = 0; i < n; i++) {
            int at = Ask.FIRST + i * Ask.SIZE;
            out.add(new Ask(b.getLong(at + Ask.ID), b.getLong(at + Ask.PLAYER_MOST), b.getLong(at + Ask.PLAYER_LEAST),
                    b.getInt(at + Ask.HOLDER_KIND), b.getInt(at + Ask.DIMENSION), b.getLong(at + Ask.HOLDER),
                    b.getInt(at + Ask.X), b.getInt(at + Ask.Z), b.getInt(at + Ask.FLAGS)));
        }
        return out;
    }

    // ---- facts ----------------------------------------------------------------------------------

    /** TALK_FACTS flags. */
    public static final int F_HOLDER = 1, F_PORT_CLOSED = 2, F_BLOCKADED = 4, F_FACTION = 8;
    /** News items in one answer, at most (TK20). */
    public static final int MAX_NEWS = 4;

    /** The size of a whole TALK_FACTS record of n news items. */
    public static int factsRecordSize(int n) {
        return Facts.NEWS + n * News.SIZE;
    }

    /** A ref's classes. */
    public static final int C_UNKNOWN = 0, C_PLAYER = 1, C_VILLAGE = 2, C_BASE = 3, C_CARAVAN = 4, C_CAMP = 5,
            C_CHIEF = 6, C_DRAGON = 7, C_LAIR = 8, C_HAVEN = 9, C_SHIP = 10;
    /** A ref's flags. */
    public static final int REF_DEAD = 1, REF_POS = 2;

    /**
     * An actor as the engine describes it (TK30). {@code what} is the engine's dictionary id of the
     * culture (villages, camps, chiefs) or of the body (dragons, lairs, ships); {@code a} and {@code b}
     * are the class's two ids; {@code dx} and {@code dz} say where it is from the talker when {@link #hasPos}.
     */
    public record Ref(int cls, int flags, int what, long a, long b, int dx, int dz) {
        public static final int SIZE = 32;
        public static final int CLASS = 0, FLAGS = 1, WHAT = 4, A = 8, B = 16, DX = 24, DZ = 28;
        public static final Ref NONE = new Ref(C_UNKNOWN, 0, 0, 0, 0, 0, 0);

        public boolean hasPos() {
            return (flags & REF_POS) != 0;
        }

        public boolean dead() {
            return (flags & REF_DEAD) != 0;
        }

        static Ref read(ByteBuffer b, int at) {
            return new Ref(b.get(at + CLASS) & 0xFF, b.get(at + FLAGS) & 0xFF, b.getInt(at + WHAT),
                    b.getLong(at + A), b.getLong(at + B), b.getInt(at + DX), b.getInt(at + DZ));
        }

        void write(ByteBuffer buf, int at) {
            buf.put(at + CLASS, (byte) cls);
            buf.put(at + FLAGS, (byte) flags);
            buf.putInt(at + WHAT, what);
            buf.putLong(at + A, a);
            buf.putLong(at + B, b);
            buf.putInt(at + DX, dx);
            buf.putInt(at + DZ, dz);
        }
    }

    /** Talk flags of a news item. */
    public static final int YOU_DID = 1, TO_YOU = 2, TO_US = 4, BY_US = 8, SEEN = 16;

    /**
     * One piece of news. {@code ago} is game ticks since the report (the engine saturates it at 2^32 - 1);
     * {@code dx} and {@code dz} are the place from the talker, 0 and 0 across frames (TK34).
     */
    public record News(int kind, int severity, int reportFlags, int talkFlags, long ago, int dx, int dz,
                       Ref who, Ref whom, Ref near) {
        public static final int SIZE = 120;
        public static final int KIND = 0, SEVERITY = 2, REPORT_FLAGS = 3, TALK_FLAGS = 4, AGO = 8, DX = 12, DZ = 16,
                RESERVED = 20, WHO = 24, WHOM = 56, NEAR = 88;

        public boolean has(int flag) {
            return (talkFlags & flag) != 0;
        }

        static News read(ByteBuffer b, int at) {
            return new News(Short.toUnsignedInt(b.getShort(at + KIND)), b.get(at + SEVERITY) & 0xFF,
                    b.get(at + REPORT_FLAGS) & 0xFF, b.getInt(at + TALK_FLAGS), Integer.toUnsignedLong(b.getInt(at + AGO)),
                    b.getInt(at + DX), b.getInt(at + DZ), Ref.read(b, at + WHO), Ref.read(b, at + WHOM),
                    Ref.read(b, at + NEAR));
        }

        void write(ByteBuffer b, int at) {
            b.putShort(at + KIND, (short) kind);
            b.put(at + SEVERITY, (byte) severity);
            b.put(at + REPORT_FLAGS, (byte) reportFlags);
            b.putInt(at + TALK_FLAGS, talkFlags);
            b.putInt(at + AGO, (int) ago);
            b.putInt(at + DX, dx);
            b.putInt(at + DZ, dz);
            b.putInt(at + RESERVED, 0);
            who.write(b, at + WHO);
            whom.write(b, at + WHOM);
            near.write(b, at + NEAR);
        }
    }

    /**
     * An answer. The good ids and every ref's {@code what} are the engine's dictionary ids of this session;
     * decode them through the {@code EngineNames} a plugin's {@code accept} is given.
     */
    public record Facts(long askId, int flags, int sentiment, int scarcest, int scarcestRatio, int glutted,
                        int gluttedRatio, List<News> news) {
        public static final int CMD_SEQ = 8, ASK_ID = 16, FLAGS = 24, SENTIMENT = 28, SCARCEST = 32,
                SCARCEST_RATIO = 36, GLUTTED = 40, GLUTTED_RATIO = 44, NEWS = 48;

        public boolean holder() {
            return (flags & F_HOLDER) != 0;
        }

        public boolean portClosed() {
            return (flags & F_PORT_CLOSED) != 0;
        }

        public boolean blockaded() {
            return (flags & F_BLOCKADED) != 0;
        }
    }

    /**
     * Decodes the TALK_FACTS record at {@code at} of {@code b}: {@code size} and {@code aux} are its header's.
     * Null (refused, never thrown) when it is shorter than the header, names more than {@link #MAX_NEWS} news
     * items, or is shorter than its n items need, or does not fit the buffer.
     */
    public static Facts decodeFacts(ByteBuffer b, int at, int size, int aux) {
        if (b == null || at < 0 || size < Facts.NEWS || at > b.limit() - size)
            return null;
        long n = Integer.toUnsignedLong(aux);
        if (n > MAX_NEWS || size < factsRecordSize((int) n))
            return null;
        List<News> news = new ArrayList<>((int) n);
        for (int i = 0; i < n; i++)
            news.add(News.read(b, at + Facts.NEWS + i * News.SIZE));
        return new Facts(b.getLong(at + Facts.ASK_ID), b.getInt(at + Facts.FLAGS), b.getInt(at + Facts.SENTIMENT),
                b.getInt(at + Facts.SCARCEST), b.getInt(at + Facts.SCARCEST_RATIO), b.getInt(at + Facts.GLUTTED),
                b.getInt(at + Facts.GLUTTED_RATIO), List.copyOf(news));
    }

    /** Decodes a whole TALK_FACTS record (header included); null when it is not one or is refused as above. */
    public static Facts decodeFacts(byte[] rec) {
        if (rec == null || rec.length < Facts.NEWS)
            return null;
        ByteBuffer b = ByteBuffer.wrap(rec).order(ByteOrder.LITTLE_ENDIAN);
        if (Short.toUnsignedInt(b.getShort(R_KIND)) != TALK_FACTS)
            return null;
        return decodeFacts(b, 0, rec.length, b.getInt(R_AUX));
    }

    /** A whole TALK_FACTS record with command sequence {@code seq} (what the engine writes; for tests). At most {@link #MAX_NEWS} news items are written. */
    public static byte[] encodeFacts(Facts f, long seq) {
        int n = Math.min(f.news.size(), MAX_NEWS);
        byte[] rec = new byte[factsRecordSize(n)];
        ByteBuffer b = ByteBuffer.wrap(rec).order(ByteOrder.LITTLE_ENDIAN);
        b.putShort(R_KIND, (short) TALK_FACTS);
        b.putShort(R_SIZE, (short) rec.length);
        b.putInt(R_AUX, n);
        b.putLong(Facts.CMD_SEQ, seq);
        b.putLong(Facts.ASK_ID, f.askId);
        b.putInt(Facts.FLAGS, f.flags);
        b.putInt(Facts.SENTIMENT, f.sentiment);
        b.putInt(Facts.SCARCEST, f.scarcest);
        b.putInt(Facts.SCARCEST_RATIO, f.scarcestRatio);
        b.putInt(Facts.GLUTTED, f.glutted);
        b.putInt(Facts.GLUTTED_RATIO, f.gluttedRatio);
        for (int i = 0; i < n; i++)
            f.news.get(i).write(b, Facts.NEWS + i * News.SIZE);
        return rec;
    }
}
