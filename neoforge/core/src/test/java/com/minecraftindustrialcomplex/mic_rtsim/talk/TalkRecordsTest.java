package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Ask;
import com.minecraftindustrialcomplex.mic_rtsim.talk.TalkRecords.Faction;
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
 * The TALK_ASK, TALK_FACTS and TALKED bytes (docs/boundary.md §13), pinned to the SAME bytes as the Rust
 * side's tests in {@code crates/rtsim-jni/src/talk_wire.rs} ({@code the_ask_bytes_are_pinned},
 * {@code the_facts_bytes_are_pinned} and {@code the_talked_bytes_are_pinned}). A change to a layout fails both.
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

    /** talk_wire.rs PINNED_FACTION_BLOCK: the 48 bytes after the (here, absent) news of {@link #pinnedBlock()}. */
    static final String PINNED_FACTION_BLOCK =
            "0409040000004841201c0000030000000600000004000000bbaa000000000000"
                    + "63000000000000000000000000000000";

    /** talk_wire.rs PINNED_TALKED. */
    static final String PINNED_TALKED =
            "000a400000000000341200000000000040e201000000000011100f0e0d0c0b0a"
                    + "0807060504030201444433332222111188887777666655550300000000000000";

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

    /** Cowed and lean, at the raid rung, over killing, with a chief ref (reserved on the wire for now). */
    private static Faction pinnedBlock() {
        return new Faction(4, TalkRecords.MOOD_COWED | TalkRecords.MOOD_LEAN, 4, 12.5f, 7200, 3,
                new Ref(TalkRecords.C_CHIEF, 0, 4, 0xAABB, 99, 0, 0));
    }

    private static Facts campFacts(List<News> news, Faction block) {
        return new Facts(9, TalkRecords.F_HOLDER, 0, 0, 0, 0, 0, news, block);
    }

    @Test
    void theAskBytesArePinned() {
        byte[] rec = TalkRecords.encodeAsks(List.of(pinnedAsk()));
        assertEquals(PINNED_ASK, hex(rec));
        assertEquals(List.of(pinnedAsk()), TalkRecords.decodeAsks(rec));
    }

    private static TalkRecords.Talked pinnedTalked() {
        return new TalkRecords.Talked(0x0102_0304_0506_0708L, 0x1111_2222_3333_4444L, 0x5555_6666_7777_8888L, 3);
    }

    @Test
    void theTalkedBytesArePinned() {
        byte[] rec = TalkRecords.encodeTalked(0x1234, 123_456, 0x0A0B_0C0D_0E0F_1011L, pinnedTalked());
        assertEquals(64, rec.length);
        assertEquals(PINNED_TALKED, hex(rec));
    }

    @Test
    void theTalkedBytesAreAlsoWhatTheLiteralOffsetsSay() {
        // Built by literal offset (docs/boundary.md §13.7), not through the codec's constants.
        ByteBuffer w = le(new byte[64]);
        w.putShort(0, (short) 0x0A00).putShort(2, (short) 64).putInt(4, 0);
        w.putLong(8, 0x1234).putLong(16, 123_456).putLong(24, 0x0A0B_0C0D_0E0F_1011L);
        w.putLong(32, 0x0102_0304_0506_0708L).putLong(40, 0x1111_2222_3333_4444L).putLong(48, 0x5555_6666_7777_8888L);
        w.putInt(56, 3).putInt(60, 0);
        assertArrayEquals(w.array(), TalkRecords.encodeTalked(0x1234, 123_456, 0x0A0B_0C0D_0E0F_1011L, pinnedTalked()));
        assertEquals(64, TalkRecords.Talked.SIZE);
        assertEquals(TalkRecords.Talked.SIZE, TalkRecords.Talked.RESERVED + 4);
    }

    @Test
    void writingTheFieldsLeavesTheHeaderAndTheOpIdToTheJournal() {
        // What the plugin does: the journal wrote the header, obsSeq and game time; the fields go in around the op id.
        byte[] rec = new byte[64];
        java.util.Arrays.fill(rec, (byte) 0x7F);
        ByteBuffer b = le(rec);
        TalkRecords.writeTalked(b, pinnedTalked());
        for (int i = 0; i < 32; i++)
            assertEquals(0x7F, rec[i], "byte " + i + " is the journal's, not the writer's");
        assertEquals(0x0102_0304_0506_0708L, b.getLong(32));
        assertEquals(3, b.getInt(56));
        assertEquals(0, b.getInt(60));
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
        assertEquals(48, Faction.SIZE);
        assertEquals(96, TalkRecords.factsRecordSize(0, true));
        assertEquals(576, TalkRecords.factsRecordSize(4, true));
        assertEquals(TalkRecords.factsRecordSize(2), TalkRecords.factsRecordSize(2, false));
        assertEquals(Faction.SIZE, Faction.CHIEF + Ref.SIZE);
        assertEquals(News.SIZE, News.NEAR + Ref.SIZE);
        for (int n = 0; n <= 4; n++)
            assertEquals(0, TalkRecords.factsRecordSize(n) % 8);
    }

    @Test
    void theFactionBlockBytesArePinned() {
        byte[] rec = TalkRecords.encodeFacts(campFacts(List.of(), pinnedBlock()), 5);
        assertEquals(96, rec.length);
        assertEquals(PINNED_FACTION_BLOCK, hex(java.util.Arrays.copyOfRange(rec, 48, 96)));
        Facts back = TalkRecords.decodeFacts(rec);
        assertNotNull(back);
        assertEquals(pinnedBlock(), back.faction());
        assertEquals(TalkRecords.F_HOLDER | TalkRecords.F_FACTION, back.flags());
    }

    @Test
    void theFactionBlockDecodesFromTheLiteralOffsets() {
        // The block as docs/boundary.md section 13.3 lays it out, built by hand after two news items.
        byte[] rec = new byte[48 + 2 * 120 + 48];
        ByteBuffer w = le(rec);
        w.putShort(0, (short) TalkRecords.TALK_FACTS).putShort(2, (short) rec.length).putInt(4, 2);
        w.putLong(16, 9).putInt(24, 1 | 8);
        int at = 48 + 2 * 120;
        w.put(at, (byte) 5).put(at + 1, (byte) (4 | 16)).putShort(at + 2, (short) 0xFFFF).putFloat(at + 4, 0.25f);
        w.putInt(at + 8, -1).putInt(at + 12, 70_000);
        w.put(at + 16, (byte) TalkRecords.C_CHIEF);
        Facts f = TalkRecords.decodeFacts(le(rec), 0, rec.length, 2);
        assertNotNull(f);
        assertEquals(2, f.news().size());
        Faction x = f.faction();
        assertNotNull(x);
        assertEquals(5, x.rung());
        assertTrue(x.has(TalkRecords.MOOD_NEW_CHIEF) && x.has(TalkRecords.MOOD_STRONG) && !x.has(TalkRecords.MOOD_COWED));
        assertEquals(TalkRecords.NO_SIGNAL, x.signal(), "the signal is unsigned");
        assertEquals(0.25f, x.grievance());
        assertEquals(0xFFFF_FFFFL, x.cowedTicks(), "the ticks are unsigned");
        assertEquals(70_000, x.plunders());
        assertEquals(TalkRecords.C_CHIEF, x.chief().cls());
    }

    @Test
    void aRoundTripKeepsTheNewsAndTheBlockAndWritesTheFlagItself() {
        List<News> news = pinnedFacts().news();
        Facts with = campFacts(news, pinnedBlock());
        byte[] rec = TalkRecords.encodeFacts(with, 1);
        assertEquals(TalkRecords.factsRecordSize(1, true), rec.length);
        assertEquals(48 + 120 + 48, rec.length);
        assertEquals(1, le(rec).getInt(4), "aux counts the news only");
        Facts back = TalkRecords.decodeFacts(rec);
        assertEquals(news, back.news());
        assertEquals(pinnedBlock(), back.faction());
        // The encoder owns the flag: a block with the flag clear still sets it, no block clears it.
        assertEquals(TalkRecords.F_HOLDER | TalkRecords.F_FACTION, back.flags());
        Facts stale = new Facts(9, TalkRecords.F_HOLDER | TalkRecords.F_FACTION, 0, 0, 0, 0, 0, news, null);
        byte[] plain = TalkRecords.encodeFacts(stale, 1);
        assertEquals(TalkRecords.factsRecordSize(1), plain.length);
        Facts plainBack = TalkRecords.decodeFacts(plain);
        assertNull(plainBack.faction());
        assertEquals(TalkRecords.F_HOLDER, plainBack.flags());
    }

    @Test
    void aFactionFlagWithNoRoomForTheBlockIsRefusedAndTheOtherFlagsNeedNone() {
        byte[] rec = TalkRecords.encodeFacts(campFacts(List.of(), pinnedBlock()), 1);
        // Every length short of the block is refused, whole header or not.
        for (int len : new int[]{0, 47, 48, 60, 95})
            assertNull(TalkRecords.decodeFacts(java.util.Arrays.copyOf(rec, len)), "length " + len);
        // With the flag clear the same short record is a plain answer.
        byte[] plain = java.util.Arrays.copyOf(rec, 48);
        le(plain).putShort(2, (short) 48).putInt(24, TalkRecords.F_HOLDER);
        Facts f = TalkRecords.decodeFacts(plain);
        assertNotNull(f);
        assertNull(f.faction());
        // A block after four news items still fits the 576-byte ceiling and no more.
        News n = pinnedFacts().news().get(0);
        byte[] big = TalkRecords.encodeFacts(campFacts(List.of(n, n, n, n, n, n), pinnedBlock()), 1);
        assertEquals(576, big.length);
        assertEquals(4, TalkRecords.decodeFacts(big).news().size());
        assertEquals(pinnedBlock(), TalkRecords.decodeFacts(big).faction());
        assertNull(TalkRecords.decodeFacts(java.util.Arrays.copyOf(big, 575)));
    }

    @Test
    void aCampIsAskedForItsFactionBlockAndEveryoneElseForNewsOnly() {
        assertEquals(TalkRecords.ASK_NEWS | TalkRecords.ASK_FACTION, TalkRecords.askFlagsFor(TalkRecords.HOLDER_FACTION));
        assertEquals(TalkRecords.ASK_NEWS, TalkRecords.askFlagsFor(TalkRecords.HOLDER_VILLAGE));
        assertEquals(TalkRecords.ASK_NEWS, TalkRecords.askFlagsFor(TalkRecords.HOLDER_CARAVAN));
        assertEquals(TalkRecords.ASK_NEWS, TalkRecords.askFlagsFor(TalkRecords.HOLDER_SHIP));
        assertEquals(3, TalkRecords.askFlagsFor(3));
        assertEquals(2, TalkRecords.ASK_FACTION);
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
