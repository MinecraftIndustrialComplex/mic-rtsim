package com.minecraftindustrialcomplex.mic_rtsim.engine;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import org.junit.jupiter.api.Test;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * The order and received-log encodings NativeEngine shares with rtsim-jni, in pure Java: nothing here
 * loads the native library. The Rust side's tests decode the same layouts.
 */
class NativeCodecTest {

    /** Reads an encoded payload field by field, little-endian, as the Rust side does. */
    private static final class Reader {
        final ByteBuffer b;

        Reader(byte[] bytes) {
            b = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        }

        long u64() {
            return b.getLong();
        }

        int u32() {
            return b.getInt();
        }

        int u16() {
            return Short.toUnsignedInt(b.getShort());
        }

        float f32() {
            return b.getFloat();
        }

        double f64() {
            return b.getDouble();
        }

        String str() {
            byte[] utf8 = new byte[b.getInt()];
            b.get(utf8);
            return new String(utf8, StandardCharsets.UTF_8);
        }

        /** Reads an order header; returns the order's end offset. */
        int header(int opcode) {
            int start = b.position();
            assertEquals(opcode, u16(), "opcode");
            assertEquals(0, u16(), "reserved");
            return start + u32();
        }
    }

    @Test
    void spawnGroupEncodesEveryFieldInOrder() {
        ScriptedEngine.SpawnSpec spec = new ScriptedEngine.SpawnSpec(0x1122334455667788L, 7, "mic_rtsim:human_caravan",
                "minecraft:overworld", 1.5, -64.25, 1e9, List.of(
                ScriptedEngine.MemberSpec.plain(0),
                new ScriptedEngine.MemberSpec(1, 12.5f, 2, 3, List.of(
                        new ScriptedEngine.ItemSpec(0, "minecraft:emerald", 12),
                        new ScriptedEngine.ItemSpec(4, "modid:größe", 30)))));
        byte[] bytes = new NativeCodec.Orders().spawnGroup(spec).toBytes();
        Reader r = new Reader(bytes);
        int end = r.header(NativeCodec.SPAWN_GROUP);
        assertEquals(bytes.length, end, "the size covers the whole order, header included");
        assertEquals(0x1122334455667788L, r.u64());
        assertEquals(7, r.u32());
        assertEquals("mic_rtsim:human_caravan", r.str());
        assertEquals("minecraft:overworld", r.str());
        assertEquals(1.5, r.f64());
        assertEquals(-64.25, r.f64());
        assertEquals(1e9, r.f64());
        assertEquals(2, r.u32(), "member count");
        // member 0: plain
        assertEquals(0, r.u32());
        assertEquals(0f, r.f32());
        assertEquals(-1, r.u32());
        assertEquals(-1, r.u32());
        assertEquals(0, r.u32());
        // member 1: two stacks, each u32 slot, i32 count, str item
        assertEquals(1, r.u32());
        assertEquals(12.5f, r.f32());
        assertEquals(2, r.u32());
        assertEquals(3, r.u32());
        assertEquals(2, r.u32());
        assertEquals(0, r.u32());
        assertEquals(12, r.u32());
        assertEquals("minecraft:emerald", r.str());
        assertEquals(4, r.u32());
        assertEquals(30, r.u32());
        assertEquals("modid:größe", r.str(), "strings are UTF-8 with a byte length");
        assertEquals(bytes.length, r.b.position(), "no padding");
    }

    @Test
    void fixedOrdersHaveTheirExactSizes() {
        byte[] bytes = new NativeCodec.Orders()
                .despawnGroup(42, 3)
                .steerGroup(42, 3, 10.0, 64.0, -10.0, 0.75f)
                .transferToContainer(4242, "minecraft:overworld", 1, -2, 3, "minecraft:diamond", 5)
                .saveAtNextStep()
                .rewindCommandSequence(99)
                .toBytes();
        Reader r = new Reader(bytes);

        int end = r.header(NativeCodec.DESPAWN_GROUP);
        assertEquals(42, r.u64());
        assertEquals(3, r.u32());
        assertEquals(NativeCodec.DESPAWN_REASON_COMMAND, r.u32());
        assertEquals(end, r.b.position());
        assertEquals(24, end);

        int start = r.b.position();
        end = r.header(NativeCodec.STEER_GROUP);
        assertEquals(42, r.u64());
        assertEquals(3, r.u32());
        assertEquals(0.75f, r.f32(), "speed comes before the target");
        assertEquals(10.0, r.f64());
        assertEquals(64.0, r.f64());
        assertEquals(-10.0, r.f64());
        assertEquals(end, r.b.position());
        assertEquals(48, end - start);

        start = r.b.position();
        end = r.header(NativeCodec.TRANSFER_TO_CONTAINER);
        assertEquals(4242, r.u64());
        assertEquals("minecraft:overworld", r.str());
        assertEquals("minecraft:diamond", r.str());
        assertEquals(1, r.u32());
        assertEquals(-2, r.u32());
        assertEquals(3, r.u32());
        assertEquals(5, r.u32());
        assertEquals(end, r.b.position());
        assertEquals(8 + 8 + 4 + 19 + 4 + 17 + 16, end - start);

        start = r.b.position();
        end = r.header(NativeCodec.SAVE_AT_NEXT_STEP);
        assertEquals(8, end - start, "no body");

        start = r.b.position();
        end = r.header(NativeCodec.REWIND_CMD_SEQ);
        assertEquals(99, r.u64());
        assertEquals(16, end - start);
        assertEquals(bytes.length, r.b.position());
    }

    @Test
    void manyOrdersGrowTheBufferAndChainTheirSizes() {
        NativeCodec.Orders orders = new NativeCodec.Orders();
        for (int i = 0; i < 200; i++)
            orders.transferToContainer(i, "minecraft:the_nether", i, i, i, "minecraft:cobblestone", 64);
        byte[] bytes = orders.toBytes();
        ByteBuffer b = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        int at = 0;
        int n = 0;
        while (at < bytes.length) {
            assertEquals(NativeCodec.TRANSFER_TO_CONTAINER, Short.toUnsignedInt(b.getShort(at)));
            assertEquals(n, b.getLong(at + NativeCodec.ORDER_HEADER), "op id of order " + n);
            at += b.getInt(at + 4);
            n++;
        }
        assertEquals(200, n);
        assertEquals(bytes.length, at);
    }

    @Test
    void phase1OpcodesAreTheAgreedNumbers() {
        assertEquals(16, NativeCodec.MARKET);
        assertEquals(17, NativeCodec.MERCHANT_OFFERS);
        assertEquals(18, NativeCodec.WRITE_GOSSIP);
        assertEquals(19, NativeCodec.SENTIMENT);
        assertEquals(20, NativeCodec.PLACE_HOUSE);
        assertEquals(21, NativeCodec.SPAWN_RESIDENT);
        assertEquals(22, NativeCodec.SCRIPT_VILLAGE);
        assertEquals(23, NativeCodec.P1_SPAWN_NEAR);
        assertEquals(24, NativeCodec.P1_ADVANCE);
    }

    @Test
    void marketEncodesEveryFieldInOrder() {
        byte[] bytes = new NativeCodec.Orders().market(0x0102030405060708L, 9_000_000_000L, "minecraft:emerald",
                -5_000_000_000L, 77, -3, 4, List.of(
                        new ScriptedEngine.GoodSpec("minecraft:wheat", 123_456_789_012L, -2, 1_500_000L, 7, 11),
                        new ScriptedEngine.GoodSpec("modid:größe", 0, 0, 0, -1, -2))).toBytes();
        Reader r = new Reader(bytes);
        int end = r.header(NativeCodec.MARKET);
        assertEquals(bytes.length, end);
        assertEquals(0x0102030405060708L, r.u64(), "village");
        assertEquals(9_000_000_000L, r.u64(), "asOfObsSeq");
        assertEquals("minecraft:emerald", r.str(), "currency");
        assertEquals(-5_000_000_000L, r.u64(), "treasury is a signed 64-bit");
        assertEquals(77, r.u64(), "treasuryTarget");
        assertEquals(-3, r.u32(), "worstNum");
        assertEquals(4, r.u32(), "worstDen");
        assertEquals(2, r.u32(), "good count");
        assertEquals("minecraft:wheat", r.str());
        assertEquals(123_456_789_012L, r.u64(), "stock");
        assertEquals(-2, r.u64(), "target");
        assertEquals(1_500_000L, r.u64(), "anchor");
        assertEquals(7, r.u32(), "rNum");
        assertEquals(11, r.u32(), "rDen");
        assertEquals("modid:größe", r.str());
        assertEquals(0, r.u64());
        assertEquals(0, r.u64());
        assertEquals(0, r.u64());
        assertEquals(-1, r.u32());
        assertEquals(-2, r.u32());
        assertEquals(bytes.length, r.b.position(), "no padding");
    }

    @Test
    void marketWithNoGoodsIsJustItsHeaderAndCount() {
        byte[] bytes = new NativeCodec.Orders().market(1, 2, "", 3, 4, 5, 6, List.of()).toBytes();
        Reader r = new Reader(bytes);
        r.header(NativeCodec.MARKET);
        assertEquals(1, r.u64());
        assertEquals(2, r.u64());
        assertEquals("", r.str());
        assertEquals(3, r.u64());
        assertEquals(4, r.u64());
        assertEquals(5, r.u32());
        assertEquals(6, r.u32());
        assertEquals(0, r.u32());
        assertEquals(bytes.length, r.b.position());
        assertEquals(8 + 8 + 8 + 4 + 8 + 8 + 4 + 4 + 4, bytes.length);
    }

    @Test
    void merchantOffersEncodeANullSecondCostAsTheEmptyString() {
        byte[] bytes = new NativeCodec.Orders().merchantOffers(0x1122334455667788L, 7, 2, List.of(
                new ScriptedEngine.OfferSpec("minecraft:emerald", 5, null, 0, "minecraft:bread", 3, 12, 1),
                new ScriptedEngine.OfferSpec("minecraft:wheat", 20, "minecraft:paper", 2, "minecraft:emerald", 1, -1, 2))).toBytes();
        Reader r = new Reader(bytes);
        int end = r.header(NativeCodec.MERCHANT_OFFERS);
        assertEquals(bytes.length, end);
        assertEquals(0x1122334455667788L, r.u64(), "agent");
        assertEquals(7, r.u32(), "epoch");
        assertEquals(2, r.u32(), "member");
        assertEquals(2, r.u32(), "offer count");
        assertEquals("minecraft:emerald", r.str(), "costA");
        assertEquals(5, r.u32(), "costACount");
        assertEquals("", r.str(), "a null costB is the empty string");
        assertEquals(0, r.u32(), "costBCount");
        assertEquals("minecraft:bread", r.str(), "result");
        assertEquals(3, r.u32(), "resultCount");
        assertEquals(12, r.u32(), "maxUses");
        assertEquals(1, r.u32(), "flags");
        assertEquals("minecraft:wheat", r.str());
        assertEquals(20, r.u32());
        assertEquals("minecraft:paper", r.str());
        assertEquals(2, r.u32());
        assertEquals("minecraft:emerald", r.str());
        assertEquals(1, r.u32());
        assertEquals(-1, r.u32());
        assertEquals(2, r.u32());
        assertEquals(bytes.length, r.b.position(), "no padding");
    }

    @Test
    void writeGossipEncodesEveryFieldInOrder() {
        UUID player = new UUID(0x0123456789ABCDEFL, 0xFEDCBA9876543210L);
        byte[] bytes = new NativeCodec.Orders().writeGossip(0xAABBCCDDEEFF0011L, 12345, player, 4, -17, 6, 1).toBytes();
        Reader r = new Reader(bytes);
        int end = r.header(NativeCodec.WRITE_GOSSIP);
        assertEquals(bytes.length, end);
        assertEquals(0xAABBCCDDEEFF0011L, r.u64(), "opId");
        assertEquals(12345, r.u64(), "village");
        assertEquals(0x0123456789ABCDEFL, r.u64(), "player most significant bits");
        assertEquals(0xFEDCBA9876543210L, r.u64(), "player least significant bits");
        assertEquals(4, r.u32(), "gossipType");
        assertEquals(-17, r.u32(), "amount");
        assertEquals(6, r.u32(), "report");
        assertEquals(1, r.u32(), "flags");
        assertEquals(bytes.length, r.b.position());
        assertEquals(8 + 8 + 8 + 8 + 8 + 4 + 4 + 4 + 4, bytes.length);
    }

    @Test
    void sentimentEncodesEntriesInTheMapsOrderAndTheReplaceFlag() {
        UUID a = new UUID(1, 2);
        UUID b = new UUID(-3, -4);
        Map<UUID, Integer> entries = new LinkedHashMap<>();
        entries.put(b, -128);
        entries.put(a, 127);

        byte[] bytes = new NativeCodec.Orders().sentiment(99, -60, 60, true, entries).toBytes();
        Reader r = new Reader(bytes);
        int end = r.header(NativeCodec.SENTIMENT);
        assertEquals(bytes.length, end);
        assertEquals(99, r.u64(), "holder");
        assertEquals(-60, r.u32(), "dislike");
        assertEquals(60, r.u32(), "friend");
        assertEquals(1, r.u32(), "flags: replace");
        assertEquals(2, r.u32(), "entry count");
        assertEquals(-3, r.u64());
        assertEquals(-4, r.u64());
        assertEquals(-128, r.u32());
        assertEquals(1, r.u64());
        assertEquals(2, r.u64());
        assertEquals(127, r.u32());
        assertEquals(bytes.length, r.b.position(), "no padding");
        assertEquals(8 + 8 + 4 + 4 + 4 + 4 + 2 * 20, bytes.length);

        byte[] update = new NativeCodec.Orders().sentiment(99, -60, 60, false, Map.of()).toBytes();
        r = new Reader(update);
        r.header(NativeCodec.SENTIMENT);
        r.u64();
        r.u32();
        r.u32();
        assertEquals(0, r.u32(), "flags: update");
        assertEquals(0, r.u32(), "no entries");
        assertEquals(update.length, r.b.position());
    }

    @Test
    void placeHouseEncodesEveryFieldInOrderAndNullProcessorsAsTheEmptyString() {
        byte[] bytes = new NativeCodec.Orders().placeHouse(0x1122334455667788L, 31, "minecraft:overworld",
                "minecraft:village/plains/houses/plains_small_house_1", null, -100, 64, 2_000_000_000, 3, 2, 3, 24,
                0x7EDCBA9876543210L, -2).toBytes();
        Reader r = new Reader(bytes);
        int end = r.header(NativeCodec.PLACE_HOUSE);
        assertEquals(bytes.length, end);
        assertEquals(0x1122334455667788L, r.u64(), "opId");
        assertEquals(31, r.u64(), "village");
        assertEquals("minecraft:overworld", r.str(), "dimension");
        assertEquals("minecraft:village/plains/houses/plains_small_house_1", r.str(), "template");
        assertEquals("", r.str(), "null processors are the empty string");
        assertEquals(-100, r.u32(), "x");
        assertEquals(64, r.u32(), "y");
        assertEquals(2_000_000_000, r.u32(), "z");
        assertEquals(3, r.u32(), "rotation");
        assertEquals(2, r.u32(), "mirror");
        assertEquals(3, r.u32(), "flags");
        assertEquals(24, r.u32(), "radius");
        assertEquals(0x7EDCBA9876543210L, r.u64(), "seed");
        assertEquals(-2, r.u32(), "yOffset");
        assertEquals(bytes.length, r.b.position(), "no padding");

        byte[] named = new NativeCodec.Orders().placeHouse(1, 2, "d", "t", "minecraft:village/plains/zombie", 0, 0, 0, 0, 0, 0, 0, 0, 0).toBytes();
        r = new Reader(named);
        r.header(NativeCodec.PLACE_HOUSE);
        r.u64();
        r.u64();
        assertEquals("d", r.str());
        assertEquals("t", r.str());
        assertEquals("minecraft:village/plains/zombie", r.str());
    }

    @Test
    void spawnResidentEncodesEveryFieldInOrderAndNullProfessionAsTheEmptyString() {
        byte[] bytes = new NativeCodec.Orders().spawnResident(0x1122334455667788L, 31, "minecraft:overworld",
                "ribbits:ribbit", null, 1, 10.5, -64.25, 1e9, 0x0FEDCBA987654321L).toBytes();
        Reader r = new Reader(bytes);
        int end = r.header(NativeCodec.SPAWN_RESIDENT);
        assertEquals(bytes.length, end);
        assertEquals(0x1122334455667788L, r.u64(), "opId");
        assertEquals(31, r.u64(), "village");
        assertEquals("minecraft:overworld", r.str(), "dimension");
        assertEquals("ribbits:ribbit", r.str(), "body");
        assertEquals("", r.str(), "null profession is the empty string");
        assertEquals(1, r.u32(), "flags");
        assertEquals(10.5, r.f64(), "x");
        assertEquals(-64.25, r.f64(), "y");
        assertEquals(1e9, r.f64(), "z");
        assertEquals(0x0FEDCBA987654321L, r.u64(), "house");
        assertEquals(bytes.length, r.b.position(), "no padding");

        byte[] named = new NativeCodec.Orders().spawnResident(1, 2, "d", "b", "minecraft:farmer", 0, 0, 0, 0, 0).toBytes();
        r = new Reader(named);
        r.header(NativeCodec.SPAWN_RESIDENT);
        r.u64();
        r.u64();
        assertEquals("d", r.str());
        assertEquals("b", r.str());
        assertEquals("minecraft:farmer", r.str());
    }

    @Test
    void scriptVillageAndTheTestModeOrdersEncodeTheirFields() {
        byte[] bytes = new NativeCodec.Orders()
                .scriptVillage(0x1122334455667788L)
                .phase1SpawnNear(0x0102030405060708L, 5, "minecraft:overworld", 1.5, -64.25, 1e9, 48)
                .phase1Advance(24_000L)
                .toBytes();
        Reader r = new Reader(bytes);

        int start = r.b.position();
        int end = r.header(NativeCodec.SCRIPT_VILLAGE);
        assertEquals(0x1122334455667788L, r.u64(), "village");
        assertEquals(end, r.b.position());
        assertEquals(16, end - start);

        start = r.b.position();
        end = r.header(NativeCodec.P1_SPAWN_NEAR);
        assertEquals(0x0102030405060708L, r.u64(), "agent");
        assertEquals(5, r.u32(), "epoch");
        assertEquals("minecraft:overworld", r.str(), "dimension");
        assertEquals(1.5, r.f64(), "x");
        assertEquals(-64.25, r.f64(), "y");
        assertEquals(1e9, r.f64(), "z");
        assertEquals(48, r.u32(), "radius");
        assertEquals(end, r.b.position());
        assertEquals(8 + 8 + 4 + 4 + 19 + 24 + 4, end - start);

        start = r.b.position();
        end = r.header(NativeCodec.P1_ADVANCE);
        assertEquals(24_000L, r.u64(), "ticks");
        assertEquals(16, end - start);
        assertEquals(bytes.length, r.b.position());
    }

    /** One received-log entry as rtsim-jni writes it. */
    private static byte[] entry(long handover, long obsSeq, int flags, byte[] record) {
        ByteBuffer b = ByteBuffer.allocate(NativeCodec.RECEIVED_HEADER + record.length).order(ByteOrder.LITTLE_ENDIAN);
        b.putLong(handover).putLong(obsSeq).putInt(flags).putInt(record.length).put(record);
        return b.array();
    }

    private static byte[] record(int kind, int size, int aux, long word8) {
        ByteBuffer b = ByteBuffer.allocate(size).order(ByteOrder.LITTLE_ENDIAN);
        b.putShort((short) kind).putShort((short) size).putInt(aux).putLong(word8);
        return b.array();
    }

    private static byte[] concat(byte[]... parts) {
        int n = 0;
        for (byte[] p : parts)
            n += p.length;
        ByteBuffer b = ByteBuffer.allocate(n);
        for (byte[] p : parts)
            b.put(p);
        return b.array();
    }

    @Test
    void receivedLogDecodesKindAuxSequenceAndSkip() {
        byte[] transfer = record(Obs.TRANSFER, Obs.Transfer.SIZE, Obs.Transfer.GENERIC, 17);
        byte[] modules = record(Obs.MODULES, Obs.Modules.SIZE, 0, 0b101);
        byte[] log = concat(entry(3, 17, NativeCodec.RECEIVED_SKIPPED, transfer), entry(4, 0, 0, modules));

        List<ScriptedEngine.Received> got = NativeCodec.decodeReceived(log);
        assertEquals(2, got.size());
        ScriptedEngine.Received t = got.get(0);
        assertEquals(3, t.handover());
        assertEquals(Obs.TRANSFER, t.kind());
        assertEquals(Obs.Transfer.GENERIC, t.aux());
        assertEquals(17, t.obsSeq());
        assertTrue(t.skipped());
        assertArrayEquals(transfer, t.bytes());
        assertEquals(17, t.view().getLong(Obs.OBS_SEQ), "the view reads the record little-endian");
        ScriptedEngine.Received m = got.get(1);
        assertEquals(4, m.handover());
        assertEquals(Obs.MODULES, m.kind());
        assertEquals(0, m.obsSeq(), "latest-wins records carry no sequence");
        assertFalse(m.skipped());
        assertEquals(0b101, m.view().getInt(Obs.Modules.MASK));

        assertTrue(NativeCodec.decodeReceived(new byte[0]).isEmpty());
    }

    @Test
    void aMalformedReceivedLogFailsLoudly() {
        byte[] good = entry(1, 0, 0, record(Obs.MODULES, Obs.Modules.SIZE, 0, 1));
        byte[] truncated = new byte[good.length - 1];
        System.arraycopy(good, 0, truncated, 0, truncated.length);
        assertThrows(IllegalStateException.class, () -> NativeCodec.decodeReceived(truncated));
        assertThrows(IllegalStateException.class, () -> NativeCodec.decodeReceived(new byte[NativeCodec.RECEIVED_HEADER - 1]));
        assertThrows(IllegalStateException.class, () -> NativeCodec.decodeReceived(entry(1, 0, 0, new byte[4])),
                "a record shorter than its own header");
    }
}
