package com.minecraftindustrialcomplex.mic_rtsim.sites;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.ReinstallGuard;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.LongArrayTag;
import net.minecraft.nbt.Tag;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.saveddata.SavedData;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * The site ledger (EQUILIBRIUM.md §2.3, slice S3, MZB-1000): where each site stood when its engine last said, so a sim
 * reset or a reinstall hands a farmed site back farmed. World SavedData {@code data/mic_rtsim_sites.dat} on the
 * overworld, outside {@code world/rtsim/}: a reset keeps it, and the reinstall guard already counts it as rtsim's state
 * ({@code ReinstallRules.stateExists}).
 *
 * <p><b>Rows.</b> One per (kind, site key), replaced whole by each SITE_STATE (core command 0x8006, {@link #apply}); nothing
 * else writes one. A row holds what the command said and when: the overworld's game time is its stamp (MZB-1002). The
 * found tail ({@link #writeTail}, docs/boundary.md §2.5) hands it back on the site's found record as relative values: the
 * pressure and the wait left as of the stamp, and the game ticks since. The layouts' one home is {@code
 * rtsim_kernel::sites}; this class mirrors them.
 *
 * <p><b>Gaps</b> (MZB-1003). Only the reinstall guard's reset means rtsim did not run. The first use of the ledger after
 * one ({@link #get}) bumps {@code gaps}; each row keeps the {@code gaps} it was written under, and its tail says GAP while
 * they differ. A plain reset is not a gap: rtsim ran.
 *
 * <p><b>Saves.</b> Vanilla writes the file at its world saves, before the journal's like every overworld SavedData. It is
 * not journal dependent (SEC-1 U-g's {@code JournalDependent}): it holds no op's effect and no dedup, and a crash loses
 * at most the rows since the last autosave, which matter only after a reset (UR-1). No migrations: a file of another
 * version loads empty, and a row that does not parse is dropped and counted.
 *
 * <p><b>NBT</b> (MZB-1001): {@code v} (1), {@code gaps}, and {@code rows}, one LongArrayTag a row: 0 key; 1 kind (bits 0 to
 * 7), flags (8 to 15: 1 waiting, 2 broken), K (16 to 31), count (32 to 47), away (48 to 63); 2 P's f32 bits (0 to 31),
 * gapSeen (32 to 63); 3 stamp; 4 wait left; 5 n (0 to 15), l (16 to 31), m (32 to 47), the second pressure {@code p2}
 * in thousandths, a saturating u16 (48 to 63, MZB-1231; a row written before it reads 0); then 2 (n + l + m) longs, each
 * UUID's most then least: home, left behind, missing.
 */
public final class SiteLedger extends SavedData {
    public static final String FILE = "mic_rtsim_sites";
    public static final int VERSION = 1;

    /** Site kinds ({@code rtsim_kernel::sites::SiteKind}). */
    public static final int CAMP = 1, LAIR = 2, NEST = 3;

    /** A row's flags. */
    public static final int ROW_WAITING = 1, ROW_BROKEN = 2;

    /** The most UUIDs one record may list ({@code rtsim_kernel::sites::MAX_BODIES}). */
    public static final int MAX_BODIES = (0xFFF8 - 128) / 16;

    /** The found tail (docs/boundary.md §2.5), from its start: the found record's old end. */
    public static final class Tail {
        public static final int FLAGS = 0;      // u32, below
        public static final int K = 4;          // u16
        public static final int COUNT = 6;      // u16
        public static final int P = 8;          // f32, as of the stamp
        public static final int AWAY = 12;      // u16
        public static final int N = 14;         // u16
        public static final int WAIT_LEFT = 16; // u64, as of the stamp, kind units; 0 unless WAITING
        public static final int ELAPSED = 24;   // u64, game ticks since the stamp, 0 when the clock is below it
        public static final int L = 32;         // u16
        public static final int M = 34;         // u16
        public static final int P2 = 36;        // f32, as of the stamp: a camp with a chief of its own keeps the chief's here (MZB-1231)
        public static final int UUIDS = 40;     // (n + l + m) x (u64 most, u64 least)
        public static final int FIXED = UUIDS;

        public static final int FLAG_ROW = 1, FLAG_GAP = 2, FLAG_WAITING = 4, FLAG_EXISTED = 8;

        private Tail() {}
    }

    /**
     * One site's row: what its last SITE_STATE said, the game time it came ({@code stamp}) and the ledger's gap count
     * then. {@code uuids} is (most, least) pairs: n home, then l left behind, then m missing.
     */
    public record Row(int kind, long key, int flags, int k, int count, int away, float p, float p2, int gapSeen, long stamp,
                      long waitLeft, int n, int l, int m, long[] uuids) {
        public boolean waiting() {
            return (flags & ROW_WAITING) != 0;
        }

        public boolean broken() {
            return (flags & ROW_BROKEN) != 0;
        }

        public int bodies() {
            return n + l + m;
        }

        public List<UUID> home() {
            return list(0, n);
        }

        public List<UUID> leftBehind() {
            return list(n, l);
        }

        public List<UUID> missing() {
            return list(n + l, m);
        }

        private List<UUID> list(int from, int len) {
            List<UUID> out = new ArrayList<>(len);
            for (int i = from; i < from + len; i++)
                out.add(new UUID(uuids[2 * i], uuids[2 * i + 1]));
            return Collections.unmodifiableList(out);
        }
    }

    private record Key(int kind, long key) {}

    private final Map<Key, Row> rows = new LinkedHashMap<>();
    private int gaps;
    /** SITE_STATEs applied and refused this server start (for {@code rtsim perf} and tests). */
    private long applied, refused;

    public static final Factory<SiteLedger> FACTORY = new Factory<>(SiteLedger::new, SiteLedger::load);

    /**
     * The world's ledger. The first call after the reinstall guard's reset counts the gap, so every row written before it
     * reads GAP from then on (MZB-1003).
     */
    public static SiteLedger get(MinecraftServer server) {
        SiteLedger l = server.overworld().getDataStorage().computeIfAbsent(FACTORY, FILE);
        if (ReinstallGuard.takeSiteGap(server)) {
            l.gaps++;
            l.setDirty();
            MicRtsim.LOGGER.info("[mic_rtsim] site ledger: the reinstall reset is gap {}; its {} rows read GAP until their sites report again",
                    l.gaps, l.rows.size());
        }
        return l;
    }

    public static boolean knownKind(int kind) {
        return kind == CAMP || kind == LAIR || kind == NEST;
    }

    /** The row for a site, or null. */
    public Row row(int kind, long key) {
        return rows.get(new Key(kind, key));
    }

    public int size() {
        return rows.size();
    }

    public int gaps() {
        return gaps;
    }

    public long applied() {
        return applied;
    }

    public long refused() {
        return refused;
    }

    /** Test only: drops a site's row, as a world that never had one. */
    public void forget(int kind, long key) {
        if (rows.remove(new Key(kind, key)) != null)
            setDirty();
    }

    // ---- SITE_STATE -----------------------------------------------------------------------------

    /**
     * Applies a SITE_STATE record of {@code size} bytes at {@code at} (docs/boundary.md §2.4): replaces the site's row,
     * stamped {@code now} (the overworld's game time). Lists above K are cut to K (missing keeps its newest, the last).
     * Returns false, changing nothing, for a record that does not match its counts or names an unknown kind.
     */
    public boolean apply(ByteBuffer b, int at, int size, long now) {
        if (size < Cmd.SiteState.UUIDS) {
            refused++;
            return false;
        }
        int kind = b.getInt(at + Cmd.SiteState.KIND);
        int k = u16(b, at + Cmd.SiteState.K);
        int n = u16(b, at + Cmd.SiteState.N), l = u16(b, at + Cmd.SiteState.L), m = u16(b, at + Cmd.SiteState.M);
        if (!knownKind(kind) || size != Cmd.SiteState.size(n + l + m)) {
            refused++;
            return false;
        }
        int flags = b.getInt(at + Cmd.SiteState.FLAGS);
        boolean waiting = (flags & Cmd.SiteState.FLAG_WAITING) != 0;
        int rowFlags = (waiting ? ROW_WAITING : 0) | ((flags & Cmd.SiteState.FLAG_BROKEN) != 0 ? ROW_BROKEN : 0);
        int cap = Math.min(k, MAX_BODIES / 3);
        int bn = Math.min(n, cap), bl = Math.min(l, cap), bm = Math.min(m, cap);
        long[] uuids = new long[2 * (bn + bl + bm)];
        int base = at + Cmd.SiteState.UUIDS;
        int w = 0;
        w = copy(b, base, 0, bn, uuids, w);
        w = copy(b, base, n, bl, uuids, w);
        copy(b, base, n + l + (m - bm), bm, uuids, w);
        long key = b.getLong(at + Cmd.SiteState.KEY);
        Row row = new Row(kind, key, rowFlags, k, u16(b, at + Cmd.SiteState.COUNT), u16(b, at + Cmd.SiteState.AWAY),
                sane(b.getFloat(at + Cmd.SiteState.P)), sane(b.getFloat(at + Cmd.SiteState.P2)), gaps, now, waiting ? b.getLong(at + Cmd.SiteState.WAIT_LEFT) : 0, bn, bl, bm, uuids);
        rows.put(new Key(kind, key), row);
        applied++;
        setDirty();
        return true;
    }

    private static int copy(ByteBuffer b, int base, int from, int len, long[] out, int w) {
        for (int i = from; i < from + len; i++) {
            out[w++] = b.getLong(base + 16 * i);
            out[w++] = b.getLong(base + 16 * i + 8);
        }
        return w;
    }

    // ---- the found tail -------------------------------------------------------------------------

    /** The tail's size for a site: {@link Tail#FIXED} plus 16 a UUID of its row. */
    public int tailSize(int kind, long key) {
        Row r = row(kind, key);
        return Tail.FIXED + (r == null ? 0 : 16 * r.bodies());
    }

    /** {@link #writeTail(ByteBuffer, int, int, long, long, boolean)} for a site with no earlier life the plugin knows of. */
    public void writeTail(ByteBuffer b, int at, int kind, long key, long now) {
        writeTail(b, at, kind, key, now, false);
    }

    /**
     * Writes the site's found tail at {@code at}, {@link #tailSize} bytes (docs/boundary.md §2.5). With a row: ROW, GAP when
     * the reinstall guard reset the sim since it was written, WAITING, and the row as of its stamp with the game ticks since
     * ({@code now} below the stamp reads as none). Without one: zeros, and EXISTED when {@code existed} (the plugin knows the
     * site stood before this engine life).
     */
    public void writeTail(ByteBuffer b, int at, int kind, long key, long now, boolean existed) {
        for (int i = 0; i < Tail.FIXED; i += 8)
            b.putLong(at + i, 0);
        Row r = row(kind, key);
        if (r == null) {
            b.putInt(at + Tail.FLAGS, existed ? Tail.FLAG_EXISTED : 0);
            return;
        }
        int flags = Tail.FLAG_ROW | (r.gapSeen != gaps ? Tail.FLAG_GAP : 0) | (r.waiting() ? Tail.FLAG_WAITING : 0);
        b.putInt(at + Tail.FLAGS, flags);
        b.putShort(at + Tail.K, (short) r.k);
        b.putShort(at + Tail.COUNT, (short) r.count);
        b.putFloat(at + Tail.P, r.p);
        b.putFloat(at + Tail.P2, r.p2);
        b.putShort(at + Tail.AWAY, (short) r.away);
        b.putShort(at + Tail.N, (short) r.n);
        b.putLong(at + Tail.WAIT_LEFT, r.waitLeft);
        b.putLong(at + Tail.ELAPSED, Math.max(0, now - r.stamp));
        b.putShort(at + Tail.L, (short) r.l);
        b.putShort(at + Tail.M, (short) r.m);
        for (int i = 0; i < r.uuids.length; i++)
            b.putLong(at + Tail.UUIDS + 8 * i, r.uuids[i]);
    }

    // ---- NBT ------------------------------------------------------------------------------------

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        tag.putInt("v", VERSION);
        tag.putInt("gaps", gaps);
        ListTag list = new ListTag();
        for (Row r : rows.values()) {
            long[] a = new long[6 + r.uuids.length];
            a[0] = r.key;
            a[1] = (r.kind & 0xFFL) | (long) (r.flags & 0xFF) << 8 | (long) (r.k & 0xFFFF) << 16 | (long) (r.count & 0xFFFF) << 32
                    | (long) (r.away & 0xFFFF) << 48;
            a[2] = (Float.floatToRawIntBits(r.p) & 0xFFFF_FFFFL) | (long) r.gapSeen << 32;
            a[3] = r.stamp;
            a[4] = r.waitLeft;
            a[5] = (r.n & 0xFFFFL) | (long) (r.l & 0xFFFF) << 16 | (long) (r.m & 0xFFFF) << 32 | (long) milli(r.p2) << 48;
            System.arraycopy(r.uuids, 0, a, 6, r.uuids.length);
            list.add(new LongArrayTag(a));
        }
        tag.put("rows", list);
        return tag;
    }

    private static SiteLedger load(CompoundTag tag, HolderLookup.Provider registries) {
        SiteLedger ledger = new SiteLedger();
        int v = tag.getInt("v");
        if (v != VERSION) {
            MicRtsim.LOGGER.warn("[mic_rtsim] site ledger: {}.dat is version {}, not {}; it loads empty (no migrations)", FILE, v, VERSION);
            return ledger;
        }
        ledger.gaps = tag.getInt("gaps");
        int dropped = 0;
        for (Tag t : tag.getList("rows", Tag.TAG_LONG_ARRAY)) {
            Row r = parse(((LongArrayTag) t).getAsLongArray());
            if (r == null) {
                dropped++;
                continue;
            }
            ledger.rows.put(new Key(r.kind, r.key), r);
        }
        if (dropped > 0)
            MicRtsim.LOGGER.warn("[mic_rtsim] site ledger: {} rows did not parse and are dropped (UR-1); their sites are found as new", dropped);
        ledger.setDirty(false);
        return ledger;
    }

    /** A row from its longs, or null when it does not parse (a wrong length, an unknown kind, a list above K). */
    static Row parse(long[] a) {
        if (a.length < 6)
            return null;
        int kind = (int) (a[1] & 0xFF);
        int flags = (int) (a[1] >>> 8 & 0xFF);
        int k = (int) (a[1] >>> 16 & 0xFFFF);
        int n = (int) (a[5] & 0xFFFF), l = (int) (a[5] >>> 16 & 0xFFFF), m = (int) (a[5] >>> 32 & 0xFFFF);
        int cap = Math.min(k, MAX_BODIES / 3);
        if (!knownKind(kind) || n > cap || l > cap || m > cap || a.length != 6 + 2 * (n + l + m))
            return null;
        long[] uuids = new long[a.length - 6];
        System.arraycopy(a, 6, uuids, 0, uuids.length);
        return new Row(kind, a[0], flags, k, (int) (a[1] >>> 32 & 0xFFFF), (int) (a[1] >>> 48 & 0xFFFF),
                sane(Float.intBitsToFloat((int) a[2])), (a[5] >>> 48 & 0xFFFF) / 1000f, (int) (a[2] >>> 32), a[3], a[4], n, l, m, uuids);
    }

    private static int u16(ByteBuffer b, int at) {
        return b.getShort(at) & 0xFFFF;
    }

    /** A pressure in thousandths, a saturating u16 (the row's 16 spare bits, MZB-1231). */
    static int milli(float p) {
        return (int) Math.min(0xFFFF, Math.round(sane(p) * 1000f));
    }

    /** A pressure that is not a finite positive number reads as 0 ({@code rtsim_kernel::sites}). */
    private static float sane(float p) {
        return Float.isFinite(p) && p > 0 ? p : 0;
    }
}
