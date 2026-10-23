package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Ask;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Facts;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.News;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Ref;
import org.junit.jupiter.api.Test;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The TALK_ASK and TALK_FACTS bytes (docs/boundary.md §13), pinned to the SAME bytes as the Rust side's
 * tests in {@code crates/rtsim-jni/src/talk_wire.rs} ({@code the_ask_bytes_are_pinned} and
 * {@code the_facts_bytes_are_pinned}). A change to either layout fails both.
 */
class TalkRecordsTest {
    /** talk_wire.rs PINNED_ASK. */
    static final String PINNED_ASK =
            "c000400001000000070000000000000044443333222211118888777766665555"
                    + "010000000300000008070605040302019cffffffd00700000100000000000000";

    /** talk_wire.rs PINNED_FACTS. */
    static final String PINNED_FACTS =
            "008aa800010000003412000000000000070000000000000003000000e2ffffff"
                    + "020000009001000005000000ac0d00000d00960114000000c05d00002c010000"
                    + "d8ffffff00000000050200000400000063000000000000000000000000000000"
                    + "18010000ceffffff020200000400000008070605040302010000000000000000"
                    + "f6ffffff05000000000000000000000000000000000000000000000000000000"
                    + "0000000000000000";

    private static String hex(byte[] b) {
        StringBuilder s = new StringBuilder();
        for (byte v : b)
            s.append(String.format("%02x", v));
        return s.toString();
    }

    private static ByteBuffer le(byte[] b) {
        return ByteBuffer.wrap(b).order(ByteOrder.LITTLE_ENDIAN);
    }

    private static Ask pinnedAsk() {
        return new Ask(7, 0x1111_2222_3333_4444L, 0x5555_6666_7777_8888L, TalkRecords.HOLDER_VILLAGE, 3,
                0x0102_0304_0506_0708L, -100, 2000, TalkRecords.ASK_NEWS);
    }

    private static Facts pinnedFacts() {
        Ref village = new Ref(TalkRecords.C_VILLAGE, TalkRecords.REF_POS, 4, 0x0102_0304_0506_0708L, 0, -10, 5);
        Ref camp = new Ref(TalkRecords.C_CAMP, TalkRecords.REF_POS, 4, 99, 0, 280, -50);
        return new Facts(7, TalkRecords.F_HOLDER | TalkRecords.F_PORT_CLOSED, -30, 2, 400, 5, 3500,
                List.of(new News(13, 150, 1, TalkRecords.TO_US | TalkRecords.SEEN, 24_000, 300, -40, camp, village, Ref.NONE)));
    }

    @Test
    void theAskBytesArePinned() {
        byte[] rec = TalkRecords.encodeAsks(List.of(pinnedAsk()));
        assertEquals(PINNED_ASK, hex(rec));
        assertEquals(List.of(pinnedAsk()), TalkRecords.decodeAsks(rec));
    }

    @Test
    void theAskBytesAreAlsoWhatTheLiteralOffsetsSay() {
        // Built by literal offset (docs/boundary.md §13.2), not through the codec's constants.
        ByteBuffer w = le(new byte[64]);
        w.putShort(0, (short) 0x00C0).putShort(2, (short) 64).putInt(4, 1);
        w.putLong(8, 7).putLong(16, 0x1111_2222_3333_4444L).putLong(24, 0x5555_6666_7777_8888L);
        w.putInt(32, 1).putInt(36, 3).putLong(40, 0x0102_0304_0506_0708L).putInt(48, -100).putInt(52, 2000).putInt(56, 1);
        assertArrayEquals(w.array(), TalkRecords.encodeAsks(List.of(pinnedAsk())));
    }

    @Test
    void severalAsksAreEightPlusFiftySixEach() {
        List<Ask> asks = List.of(pinnedAsk(), new Ask(8, 1, 2, TalkRecords.HOLDER_CARAVAN, 1, 99, 5, -5, 1),
                new Ask(9, 3, 4, TalkRecords.HOLDER_VILLAGE, 1, 100, 0, 0, 1));
        byte[] rec = TalkRecords.encodeAsks(asks);
        assertEquals(8 + 3 * 56, rec.length);
        assertEquals(asks, TalkRecords.decodeAsks(rec));
        assertEquals(3, le(rec).getInt(4));
    }

    @Test
    void anAskRecordThatIsShortForItsNOrNotAnAskIsRefused() {
        byte[] rec = TalkRecords.encodeAsks(List.of(pinnedAsk()));
        le(rec).putInt(4, 2);
        assertNull(TalkRecords.decodeAsks(rec));
        le(rec).putInt(4, -1);
        assertNull(TalkRecords.decodeAsks(rec));
        le(rec).putInt(4, 1).putShort(0, (short) 0x00C1);
        assertNull(TalkRecords.decodeAsks(rec));
        assertNull(TalkRecords.decodeAsks(new byte[4]));
        assertNull(TalkRecords.decodeAsks(null));
    }

    @Test
    void theFactsBytesArePinned() {
        byte[] rec = TalkRecords.encodeFacts(pinnedFacts(), 0x1234);
        assertEquals(48 + 120, rec.length);
        assertEquals(PINNED_FACTS, hex(rec));
        assertEquals(pinnedFacts(), TalkRecords.decodeFacts(rec));
    }

    @Test
    void theFactsBytesDecodeFromTheLiteralOffsets() {
        byte[] rec = new byte[168];
        for (int i = 0; i < rec.length; i++)
            rec[i] = (byte) Integer.parseInt(PINNED_FACTS.substring(2 * i, 2 * i + 2), 16);
        ByteBuffer b = le(rec);
        Facts f = TalkRecords.decodeFacts(b, 0, rec.length, b.getInt(4));
        assertNotNull(f);
        assertEquals(7, f.askId());
        assertEquals(3, f.flags());
        assertTrue(f.holder() && f.portClosed() && !f.blockaded());
        assertEquals(-30, f.sentiment());
        assertEquals(2, f.scarcest());
        assertEquals(400, f.scarcestRatio());
        assertEquals(5, f.glutted());
        assertEquals(3500, f.gluttedRatio());
        assertEquals(1, f.news().size());
        News n = f.news().get(0);
        assertEquals(13, n.kind());
        assertEquals(150, n.severity());
        assertEquals(1, n.reportFlags());
        assertTrue(n.has(TalkRecords.TO_US) && n.has(TalkRecords.SEEN) && !n.has(TalkRecords.YOU_DID));
        assertEquals(24_000, n.ago());
        assertEquals(300, n.dx());
        assertEquals(-40, n.dz());
        assertEquals(TalkRecords.C_CAMP, n.who().cls());
        assertEquals(99, n.who().a());
        assertEquals(4, n.who().what());
        assertEquals(280, n.who().dx());
        assertEquals(-50, n.who().dz());
        assertTrue(n.who().hasPos());
        assertEquals(TalkRecords.C_VILLAGE, n.whom().cls());
        assertEquals(0x0102_0304_0506_0708L, n.whom().a());
        assertEquals(-10, n.whom().dx());
        assertEquals(5, n.whom().dz());
        assertEquals(Ref.NONE, n.near());
        assertFalse(n.near().hasPos());
    }

    @Test
    void theSizesAreWhatTheSpecSays() {
        assertEquals(56, Ask.SIZE);
        assertEquals(64, TalkRecords.askRecordSize(1));
        assertEquals(120, News.SIZE);
        assertEquals(32, Ref.SIZE);
        assertEquals(48, TalkRecords.factsRecordSize(0));
        assertEquals(528, TalkRecords.factsRecordSize(4));
        assertEquals(News.SIZE, News.NEAR + Ref.SIZE);
        for (int n = 0; n <= 4; n++)
            assertEquals(0, TalkRecords.factsRecordSize(n) % 8);
    }

    @Test
    void aFactsRecordShortForItsNIsRefusedNotThrown() {
        byte[] rec = TalkRecords.encodeFacts(pinnedFacts(), 1);
        // Cut in the middle of the news item, and in the middle of the header.
        for (int len : new int[]{0, 8, 47, 48, 100, 167})
            assertNull(TalkRecords.decodeFacts(java.util.Arrays.copyOf(rec, len)), "length " + len);
        // n claims two items in a record that holds one.
        le(rec).putInt(4, 2);
        assertNull(TalkRecords.decodeFacts(rec));
        // The size handed to the buffer decoder is checked against the buffer, not trusted.
        assertNull(TalkRecords.decodeFacts(le(rec), 0, 10_000, 1));
        assertNull(TalkRecords.decodeFacts(le(rec), 8, 168, 1));
        assertNull(TalkRecords.decodeFacts(le(rec), -8, 168, 1));
        assertNull(TalkRecords.decodeFacts(null, 0, 168, 1));
        // Not a TALK_FACTS.
        le(rec).putInt(4, 1).putShort(0, (short) 0x8A01);
        assertNull(TalkRecords.decodeFacts(rec));
    }

    @Test
    void moreThanFourNewsItemsIsRefused() {
        byte[] big = new byte[48 + 120 * 6];
        ByteBuffer b = le(big);
        b.putShort(0, (short) TalkRecords.TALK_FACTS).putShort(2, (short) big.length).putInt(4, 5);
        assertNull(TalkRecords.decodeFacts(big));
        b.putInt(4, 4);
        assertNotNull(TalkRecords.decodeFacts(big));
        // A huge n, or a negative one read as unsigned, cannot overflow the size arithmetic.
        b.putInt(4, Integer.MAX_VALUE);
        assertNull(TalkRecords.decodeFacts(big));
        b.putInt(4, -1);
        assertNull(TalkRecords.decodeFacts(big));
    }

    @Test
    void atMostFourNewsItemsAreWritten() {
        News n = pinnedFacts().news().get(0);
        Facts six = new Facts(1, 0, 0, 0, 0, 0, 0, List.of(n, n, n, n, n, n));
        byte[] rec = TalkRecords.encodeFacts(six, 1);
        assertEquals(TalkRecords.factsRecordSize(4), rec.length);
        assertEquals(4, TalkRecords.decodeFacts(rec).news().size());
    }

    @Test
    void aRecordWithNoHolderIsAllZeroAfterTheFlags() {
        byte[] rec = TalkRecords.encodeFacts(new Facts(3, 0, 0, 0, 0, 0, 0, List.of()), 9);
        Facts f = TalkRecords.decodeFacts(rec);
        assertNotNull(f);
        assertFalse(f.holder());
        assertEquals(0, f.news().size());
        assertEquals(3, f.askId());
    }

    @Test
    void theTalkCommandRangeIs8A00To8AFF() {
        assertFalse(TalkRecords.isTalkCommand(0x89FF));
        assertTrue(TalkRecords.isTalkCommand(0x8A00));
        assertTrue(TalkRecords.isTalkCommand(0x8AFF));
        assertFalse(TalkRecords.isTalkCommand(0x8B00));
    }
}
