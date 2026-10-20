package com.minecraftindustrialcomplex.mic_rtsim.engine;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import org.junit.jupiter.api.Test;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.List;

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
    void anIndustryCommandCarriesItsNamesThenTheRecord() {
        byte[] record = new byte[48];
        ByteBuffer rb = ByteBuffer.wrap(record).order(ByteOrder.LITTLE_ENDIAN);
        rb.putShort(0, (short) 0x8403);
        rb.putShort(2, (short) 48);
        rb.putLong(16, (0x49L << 56) | 7);
        byte[] bytes = new NativeCodec.Orders().industryCommand(List.of(
                new NativeEngine.DictEntry(0x10, 3, "d/0/oreexcavation:vein"),
                new NativeEngine.DictEntry(1, 1, "minecraft:overworld")), record).toBytes();
        Reader r = new Reader(bytes);
        int end = r.header(NativeCodec.INDUSTRY_COMMAND);
        assertEquals(bytes.length, end);
        assertEquals(2, r.u32());
        assertEquals(0x10, r.u32());
        assertEquals(3, r.u32());
        assertEquals("d/0/oreexcavation:vein", r.str());
        assertEquals(1, r.u32());
        assertEquals(1, r.u32());
        assertEquals("minecraft:overworld", r.str());
        byte[] tail = new byte[end - r.b.position()];
        r.b.get(tail);
        assertArrayEquals(record, tail, "the record follows the names unchanged");
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
