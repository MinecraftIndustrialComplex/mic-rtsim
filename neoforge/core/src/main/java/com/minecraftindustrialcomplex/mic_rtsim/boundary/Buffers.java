package com.minecraftindustrialcomplex.mic_rtsim.boundary;

import java.nio.ByteBuffer;

/** Allocation and views of boundary buffers. */
public final class Buffers {
    private Buffers() {}

    /**
     * A direct buffer of {@code recordBytes} record area plus the header, with a 64-aligned base
     * address and {@link Layout#H_CAPACITY} filled in. This is what an engine does on its side; the
     * stub engine uses it, and the real engine allocates in Rust and hands the memory over as a direct
     * buffer through JNI.
     */
    public static ByteBuffer allocate(int recordBytes) {
        if (recordBytes <= 0 || (recordBytes & (Layout.RECORD_ALIGN - 1)) != 0)
            throw new IllegalArgumentException("record area must be a positive multiple of 8: " + recordBytes);
        int total = Layout.HEADER_BYTES + recordBytes;
        ByteBuffer raw = ByteBuffer.allocateDirect(total + 2 * Layout.BUFFER_ALIGN);
        ByteBuffer aligned = raw.alignedSlice(Layout.BUFFER_ALIGN);
        aligned.limit(total);
        ByteBuffer buffer = aligned.slice().order(Layout.ORDER);
        buffer.putInt(Layout.H_CAPACITY, recordBytes);
        return buffer;
    }

    /**
     * Java's view of a buffer the engine handed over: an independent duplicate in little-endian order,
     * checked for alignment and size. Only absolute gets and puts are used on it.
     */
    public static ByteBuffer view(ByteBuffer engineBuffer) {
        if (!engineBuffer.isDirect())
            throw new IllegalArgumentException("boundary buffers must be direct");
        ByteBuffer view = engineBuffer.duplicate().order(Layout.ORDER);
        view.clear();
        if (view.alignmentOffset(0, Layout.BUFFER_ALIGN) != 0)
            throw new IllegalArgumentException("boundary buffer base is not 64-byte aligned");
        int capacity = view.getInt(Layout.H_CAPACITY);
        if (capacity <= 0 || Layout.HEADER_BYTES + capacity > view.capacity())
            throw new IllegalArgumentException("boundary buffer header capacity " + capacity + " exceeds buffer " + view.capacity());
        return view;
    }

    public static int capacity(ByteBuffer buffer) {
        return buffer.getInt(Layout.H_CAPACITY);
    }
}
