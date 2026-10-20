package com.minecraftindustrialcomplex.mic_rtsim.engine;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * The two byte formats {@link NativeEngine} exchanges with rtsim-jni besides the boundary buffers:
 * test and debug <b>orders</b> going in ({@code nOrder}) and the <b>received log</b> coming out
 * ({@code nDrainReceived}). Pure Java, so it is unit-tested without loading the library. The Rust
 * side (crates/rtsim-jni) mirrors it.
 *
 * <p><b>Orders</b>, little-endian, no padding or alignment. One {@code nOrder} call carries one or
 * more orders and they are one unit (all land in the same step's command buffer, or all wait). Each:
 * u16 opcode, u16 reserved 0, u32 size (the whole order, header included), then the body. A string
 * is u32 byte length, then UTF-8.
 * <ul>
 *   <li>1 SPAWN_GROUP: u64 agent, u32 epoch, str groupKind, str dimension, f64 x, f64 y, f64 z,
 *       u32 memberCount, then per member: u32 index, f32 health, i32 bodyA, i32 bodyB, u32 itemCount,
 *       then per item: u32 slot, i32 count, str item.</li>
 *   <li>2 DESPAWN_GROUP: u64 agent, u32 epoch, u32 reason.</li>
 *   <li>3 STEER_GROUP: u64 agent, u32 epoch, f32 speed, f64 x, f64 y, f64 z.</li>
 *   <li>4 TRANSFER_TO_CONTAINER: u64 opId (0: the engine allocates one), str dimension, str item,
 *       i32 x, i32 y, i32 z, i32 count.</li>
 *   <li>5 SAVE_AT_NEXT_STEP: no body.</li>
 *   <li>6 REWIND_CMD_SEQ: u64 next.</li>
 * </ul>
 *
 * <p><b>The received log</b>: entries of u64 handover, u64 obsSeq (0 if not journaled), u32 flags
 * (bit 0: skipped, its sequence was already applied), u32 length, then {@code length} bytes: the
 * observation record exactly as Java wrote it.
 */
final class NativeCodec {
    private NativeCodec() {}

    static final ByteOrder ORDER = ByteOrder.LITTLE_ENDIAN;

    static final int SPAWN_GROUP = 1;
    static final int DESPAWN_GROUP = 2;
    static final int STEER_GROUP = 3;
    static final int TRANSFER_TO_CONTAINER = 4;
    static final int SAVE_AT_NEXT_STEP = 5;
    static final int REWIND_CMD_SEQ = 6;

    /** Bytes before an order's body: u16 opcode, u16 reserved, u32 size. */
    static final int ORDER_HEADER = 8;
    /** DESPAWN_GROUP reason the orders send: 1, a command. */
    static final int DESPAWN_REASON_COMMAND = 1;

    /** Bytes before a received entry's record: u64 handover, u64 obsSeq, u32 flags, u32 length. */
    static final int RECEIVED_HEADER = 24;
    /** Received entry flag: the record's sequence was already applied, so the engine skipped it. */
    static final int RECEIVED_SKIPPED = 1;

    /** Builds the payload of one {@code nOrder} call. */
    static final class Orders {
        private ByteBuffer b = ByteBuffer.allocate(128).order(ORDER);
        private int start = -1;

        Orders spawnGroup(ScriptedEngine.SpawnSpec s) {
            begin(SPAWN_GROUP);
            u64(s.agent());
            u32(s.epoch());
            str(s.groupKind());
            str(s.dimension());
            f64(s.x());
            f64(s.y());
            f64(s.z());
            u32(s.members().size());
            for (ScriptedEngine.MemberSpec m : s.members()) {
                u32(m.index());
                f32(m.health());
                u32(m.bodyA());
                u32(m.bodyB());
                u32(m.items().size());
                for (ScriptedEngine.ItemSpec i : m.items()) {
                    u32(i.slot());
                    u32(i.count());
                    str(i.item());
                }
            }
            return end();
        }

        Orders despawnGroup(long agent, int epoch) {
            begin(DESPAWN_GROUP);
            u64(agent);
            u32(epoch);
            u32(DESPAWN_REASON_COMMAND);
            return end();
        }

        Orders steerGroup(long agent, int epoch, double x, double y, double z, float speed) {
            begin(STEER_GROUP);
            u64(agent);
            u32(epoch);
            f32(speed);
            f64(x);
            f64(y);
            f64(z);
            return end();
        }

        Orders transferToContainer(long opId, String dimension, int x, int y, int z, String item, int count) {
            begin(TRANSFER_TO_CONTAINER);
            u64(opId);
            str(dimension);
            str(item);
            u32(x);
            u32(y);
            u32(z);
            u32(count);
            return end();
        }

        Orders saveAtNextStep() {
            begin(SAVE_AT_NEXT_STEP);
            return end();
        }

        Orders rewindCommandSequence(long next) {
            begin(REWIND_CMD_SEQ);
            u64(next);
            return end();
        }

        byte[] toBytes() {
            if (start >= 0)
                throw new IllegalStateException("an order is still open");
            byte[] out = new byte[b.position()];
            b.get(0, out);
            return out;
        }

        private void begin(int opcode) {
            if (start >= 0)
                throw new IllegalStateException("an order is still open");
            start = b.position();
            room(ORDER_HEADER);
            b.putShort((short) opcode);
            b.putShort((short) 0);
            b.putInt(0); // the size, once the body is written
        }

        private Orders end() {
            b.putInt(start + 4, b.position() - start);
            start = -1;
            return this;
        }

        private void u64(long v) {
            room(8);
            b.putLong(v);
        }

        private void u32(int v) {
            room(4);
            b.putInt(v);
        }

        private void f32(float v) {
            room(4);
            b.putFloat(v);
        }

        private void f64(double v) {
            room(8);
            b.putDouble(v);
        }

        private void str(String s) {
            byte[] utf8 = s.getBytes(StandardCharsets.UTF_8);
            room(4 + utf8.length);
            b.putInt(utf8.length);
            b.put(utf8);
        }

        private void room(int n) {
            if (b.remaining() >= n)
                return;
            ByteBuffer bigger = ByteBuffer.allocate(Math.max(b.capacity() * 2, b.position() + n)).order(ORDER);
            b.flip();
            bigger.put(b);
            b = bigger;
        }
    }

    /**
     * Decodes a received log into {@code out}.
     *
     * @throws IllegalStateException if the log is malformed (an entry runs past the end, or a record
     *                               is shorter than its own 8-byte header)
     */
    static void decodeReceived(byte[] log, List<ScriptedEngine.Received> out) {
        ByteBuffer b = ByteBuffer.wrap(log).order(ORDER);
        int at = 0;
        while (at < log.length) {
            if (log.length - at < RECEIVED_HEADER)
                throw new IllegalStateException("received log: " + (log.length - at) + " trailing bytes at " + at + ", less than an entry header");
            long handover = b.getLong(at);
            long obsSeq = b.getLong(at + 8);
            int flags = b.getInt(at + 16);
            int length = b.getInt(at + 20);
            int body = at + RECEIVED_HEADER;
            if (length < Layout.RECORD_HEADER || length > log.length - body)
                throw new IllegalStateException("received log: entry at " + at + " has length " + length
                        + " (" + (log.length - body) + " bytes left)");
            byte[] record = new byte[length];
            b.get(body, record);
            int kind = Short.toUnsignedInt(b.getShort(body + Layout.R_KIND));
            int aux = b.getInt(body + Layout.R_AUX);
            out.add(new ScriptedEngine.Received(handover, kind, aux, obsSeq, (flags & RECEIVED_SKIPPED) != 0, record));
            at = body + length;
        }
    }

    static List<ScriptedEngine.Received> decodeReceived(byte[] log) {
        List<ScriptedEngine.Received> out = new ArrayList<>();
        decodeReceived(log, out);
        return out;
    }
}
