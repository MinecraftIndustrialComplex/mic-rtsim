package com.minecraftindustrialcomplex.mic_rtsim.boundary;

import java.nio.ByteBuffer;

/** Walks the records of one buffer. A malformed record ends the walk and is reported. */
public final class RecordReader {
    private ByteBuffer buffer;
    private int cursor;
    private int end;
    private int kind;
    private int size;
    private int aux;
    private String error;

    public RecordReader reset(ByteBuffer buffer, int fromOffset, int usedBytes) {
        this.buffer = buffer;
        this.cursor = fromOffset;
        this.end = Layout.HEADER_BYTES + usedBytes;
        this.error = null;
        return this;
    }

    /** Advances to the next record; false at the end or on a malformed record (see {@link #error()}). */
    public boolean next() {
        if (cursor >= end)
            return false;
        if (end - cursor < Layout.RECORD_HEADER) {
            error = "truncated record header at " + cursor;
            cursor = end;
            return false;
        }
        int k = Short.toUnsignedInt(buffer.getShort(cursor + Layout.R_KIND));
        int s = Short.toUnsignedInt(buffer.getShort(cursor + Layout.R_SIZE));
        if (s < Layout.RECORD_HEADER || (s & (Layout.RECORD_ALIGN - 1)) != 0 || cursor + s > end) {
            error = "bad record size " + s + " (kind 0x" + Integer.toHexString(k) + ") at " + cursor;
            cursor = end;
            return false;
        }
        kind = k;
        size = s;
        aux = buffer.getInt(cursor + Layout.R_AUX);
        cursor += s;
        return true;
    }

    /** Offset of the current record. */
    public int at() {
        return cursor - size;
    }

    public int kind() {
        return kind;
    }

    public int size() {
        return size;
    }

    public int aux() {
        return aux;
    }

    public ByteBuffer buffer() {
        return buffer;
    }

    public String error() {
        return error;
    }

    /** Copies the current record's bytes. */
    public byte[] copyRecord() {
        byte[] out = new byte[size];
        buffer.get(at(), out, 0, size);
        return out;
    }
}
