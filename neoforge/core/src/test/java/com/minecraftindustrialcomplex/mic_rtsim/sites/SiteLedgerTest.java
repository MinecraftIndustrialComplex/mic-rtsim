package com.minecraftindustrialcomplex.mic_rtsim.sites;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.LongArrayTag;
import net.minecraft.nbt.Tag;
import org.junit.jupiter.api.Test;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** The site ledger's second pressure (S9, MZB-1231): in SITE_STATE, the row, the NBT and the found tail. */
class SiteLedgerTest {
    static final long KEY = 0xDEAD_BEEF_0000_0001L;

    /** A SITE_STATE for a camp with one home body, at the layout's offsets. */
    static ByteBuffer state(float p, float p2) {
        int size = Cmd.SiteState.size(1);
        ByteBuffer b = ByteBuffer.allocate(size).order(ByteOrder.LITTLE_ENDIAN);
        b.putLong(Cmd.SiteState.KEY, KEY);
        b.putInt(Cmd.SiteState.KIND, SiteLedger.CAMP);
        b.putInt(Cmd.SiteState.FLAGS, Cmd.SiteState.FLAG_SHORT | Cmd.SiteState.FLAG_WAITING);
        b.putFloat(Cmd.SiteState.P, p);
        b.putFloat(Cmd.SiteState.P2, p2);
        b.putShort(Cmd.SiteState.K, (short) 7);
        b.putShort(Cmd.SiteState.COUNT, (short) 1);
        b.putShort(Cmd.SiteState.AWAY, (short) 0);
        b.putShort(Cmd.SiteState.N, (short) 1);
        b.putLong(Cmd.SiteState.WAIT_LEFT, 480_000);
        b.putLong(Cmd.SiteState.UUIDS, 11L);
        b.putLong(Cmd.SiteState.UUIDS + 8, 22L);
        return b;
    }

    @Test
    void theSiteStateLayoutMovedTheUuidsEightBytes() {
        assertEquals(56, Cmd.SiteState.P2);
        assertEquals(64, Cmd.SiteState.UUIDS);
        assertEquals(64 + 16 * 3, Cmd.SiteState.size(3));
        assertEquals(36, SiteLedger.Tail.P2);
        assertEquals(40, SiteLedger.Tail.FIXED, "the tail keeps its size");
    }

    @Test
    void aSiteStateCarriesTheSecondPressureIntoTheRowAndTheTail() {
        SiteLedger ledger = new SiteLedger();
        ByteBuffer b = state(0.5f, 1.25f);
        assertTrue(ledger.apply(b, 0, b.capacity(), 1_000));
        SiteLedger.Row r = ledger.row(SiteLedger.CAMP, KEY);
        assertNotNull(r);
        assertEquals(0.5f, r.p());
        assertEquals(1.25f, r.p2());

        ByteBuffer t = ByteBuffer.allocate(ledger.tailSize(SiteLedger.CAMP, KEY)).order(ByteOrder.LITTLE_ENDIAN);
        ledger.writeTail(t, 0, SiteLedger.CAMP, KEY, 25_000);
        assertEquals(0.5f, t.getFloat(SiteLedger.Tail.P));
        assertEquals(1.25f, t.getFloat(SiteLedger.Tail.P2));
        assertEquals(24_000L, t.getLong(SiteLedger.Tail.ELAPSED));
        // A site with no row hands over zeros, the second pressure's word included.
        ByteBuffer none = ByteBuffer.allocate(SiteLedger.Tail.FIXED).order(ByteOrder.LITTLE_ENDIAN);
        ledger.writeTail(none, 0, SiteLedger.CAMP, KEY + 1, 25_000);
        assertEquals(0f, none.getFloat(SiteLedger.Tail.P2));
    }

    @Test
    void theSecondPressureSurvivesTheNbtInThousandthsAndAnOldRowReadsZero() {
        SiteLedger ledger = new SiteLedger();
        ByteBuffer b = state(0.5f, 1.2346f);
        assertTrue(ledger.apply(b, 0, b.capacity(), 1_000));
        CompoundTag tag = ledger.save(new CompoundTag(), null);
        ListTag rows = tag.getList("rows", Tag.TAG_LONG_ARRAY);
        assertEquals(1, rows.size());
        long[] a = ((LongArrayTag) rows.get(0)).getAsLongArray();
        assertEquals(6 + 2, a.length, "six longs and a UUID: the row keeps its length");
        assertEquals(1235L, a[5] >>> 48, "thousandths, in the spare bits of the counts' long");
        assertEquals(1L, a[5] & 0xFFFF, "n is where it was");
        SiteLedger.Row back = SiteLedger.parse(a);
        assertNotNull(back);
        assertEquals(1.235f, back.p2(), 1e-6f);
        assertEquals(0.5f, back.p());
        assertEquals(1, back.n());

        // A row written before the second pressure has 0 in those bits.
        long[] old = a.clone();
        old[5] &= 0x0000_FFFF_FFFF_FFFFL;
        assertEquals(0f, SiteLedger.parse(old).p2());

        // Saturates at a u16 of thousandths; a pressure that is not finite and positive is 0.
        assertEquals(0xFFFF, SiteLedger.milli(1_000_000f));
        assertEquals(0, SiteLedger.milli(Float.NaN));
        assertEquals(0, SiteLedger.milli(-1f));
        assertEquals(0, SiteLedger.milli(Float.POSITIVE_INFINITY));
    }

    @Test
    void aStateThatIsNotTheNewSizeIsRefused() {
        SiteLedger ledger = new SiteLedger();
        // The old layout's size (56 + 16 n) no longer matches the counts.
        ByteBuffer b = ByteBuffer.allocate(56 + 16).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(Cmd.SiteState.KIND, SiteLedger.CAMP);
        b.putShort(Cmd.SiteState.K, (short) 7);
        b.putShort(Cmd.SiteState.N, (short) 1);
        assertEquals(false, ledger.apply(b, 0, b.capacity(), 1_000));
        assertEquals(0, ledger.size());
    }
}
