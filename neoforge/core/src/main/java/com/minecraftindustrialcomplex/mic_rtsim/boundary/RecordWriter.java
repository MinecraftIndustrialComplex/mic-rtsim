package com.minecraftindustrialcomplex.mic_rtsim.boundary;

import java.nio.ByteBuffer;

/**
 * Appends records to one buffer's record area. Absolute puts only, so the buffer's position and
 * limit are never touched. Reserved records are zero-filled first: buffers are reused forever, and
 * padding and reserved fields must not leak bytes from an earlier hand-over.
 */
public final class RecordWriter {
    private ByteBuffer buffer;
    private int cursor;
    private int end;
    private int count;

    public void reset(ByteBuffer buffer) {
        this.buffer = buffer;
        this.cursor = Layout.HEADER_BYTES;
        this.end = Layout.HEADER_BYTES + Buffers.capacity(buffer);
        this.count = 0;
    }

    public ByteBuffer buffer() {
        return buffer;
    }

    /** Bytes of records written so far. */
    public int used() {
        return cursor - Layout.HEADER_BYTES;
    }

    public int count() {
        return count;
    }

    public int remaining() {
        return end - cursor;
    }

    /**
     * Reserves a record and writes its header. Returns its offset in the buffer, or -1 when it does not
     * fit, in which case nothing is written.
     */
    public int reserve(int kind, int size, int aux) {
        int aligned = Layout.align(size);
        if (aligned < Layout.RECORD_HEADER || aligned > Layout.MAX_RECORD)
            throw new IllegalArgumentException("bad record size " + size + " for kind 0x" + Integer.toHexString(kind));
        if (cursor + aligned > end)
            return -1;
        int at = cursor;
        for (int i = 0; i < aligned; i += 8)
            buffer.putLong(at + i, 0L);
        buffer.putShort(at + Layout.R_KIND, (short) kind);
        buffer.putShort(at + Layout.R_SIZE, (short) aligned);
        buffer.putInt(at + Layout.R_AUX, aux);
        cursor += aligned;
        count++;
        return at;
    }

    /** A position to {@link #rollback} to: the cursor and count packed in one long. */
    public long mark() {
        return ((long) cursor << 32) | (count & 0xFFFF_FFFFL);
    }

    /** Forgets every record written since {@code mark}. */
    public void rollback(long mark) {
        cursor = (int) (mark >>> 32);
        count = (int) mark;
    }

    /** Copies a whole, already encoded record (a journal entry). Returns false when it does not fit. */
    public boolean copy(byte[] record) {
        if (cursor + record.length > end)
            return false;
        buffer.put(cursor, record, 0, record.length);
        cursor += record.length;
        count++;
        return true;
    }

    /** Writes UTF-8 bytes at an absolute offset (padding was zeroed by {@link #reserve}). */
    public void putBytes(int at, byte[] bytes) {
        buffer.put(at, bytes, 0, bytes.length);
    }
}
