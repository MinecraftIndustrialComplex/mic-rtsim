package com.minecraftindustrialcomplex.mic_rtsim.boundary;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.nio.ByteBuffer;

/**
 * The only atomic accesses on the boundary: the two sequence words of a buffer header, read with
 * acquire and written with release through a {@code byteBufferViewVarHandle} (PLAN.md §4.5).
 *
 * <p>A release store of {@link Layout#H_PUBLISHED} makes every plain write before it (the records
 * and the rest of the header) visible to a thread that reads the same word with acquire and sees the
 * new value. The same holds for {@link Layout#H_CONSUMED} in the other direction. The Rust side uses
 * {@code AtomicU64} with {@code Ordering::Release} and {@code Ordering::Acquire} on the same words.
 *
 * <p>The offsets must be 8-aligned in memory, which {@link Buffers#allocate} and the engine's own
 * allocator guarantee (base addresses are 64-aligned); the VarHandle throws
 * {@code IllegalStateException} otherwise.
 */
public final class Seq {
    private Seq() {}

    private static final VarHandle LONGS = MethodHandles.byteBufferViewVarHandle(long[].class, Layout.ORDER);

    public static long getAcquire(ByteBuffer buffer, int offset) {
        return (long) LONGS.getAcquire(buffer, offset);
    }

    public static void setRelease(ByteBuffer buffer, int offset, long value) {
        LONGS.setRelease(buffer, offset, value);
    }
}
