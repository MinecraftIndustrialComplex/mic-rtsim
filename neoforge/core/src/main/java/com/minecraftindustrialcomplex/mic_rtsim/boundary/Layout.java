package com.minecraftindustrialcomplex.mic_rtsim.boundary;

import java.nio.ByteOrder;

/**
 * The binary layout of the four boundary buffers and of every record in them.
 *
 * <p>This class and {@link Obs} and {@link Cmd} are the Java half of the contract; the Rust half
 * (rtsim-jni) mirrors them. {@code docs/boundary.md} describes the same layout in prose and tables,
 * and the two must change together. There is no format version before release: a change here is a
 * change on both sides at once (PLAN.md §4.5).
 *
 * <p><b>Byte order</b> is little-endian everywhere. Every engine target (x86-64, AArch64) is
 * little-endian, so neither side swaps bytes.
 *
 * <p><b>Alignment.</b> A buffer's base address is 64-byte aligned. Every record starts on an
 * 8-byte boundary and its size is a multiple of 8, so every u64 field at an 8-aligned offset in a
 * record is 8-aligned in memory. Only the two sequence words in the header are accessed atomically.
 */
public final class Layout {
    private Layout() {}

    public static final ByteOrder ORDER = ByteOrder.LITTLE_ENDIAN;

    /** Alignment of every buffer's base address. */
    public static final int BUFFER_ALIGN = 64;

    // ---------------------------------------------------------------------------------------------
    // Buffer header: 64 bytes, then the record area.
    // ---------------------------------------------------------------------------------------------

    public static final int HEADER_BYTES = 64;

    /** u64. The hand-over number whose records the buffer now holds; written last, with release. */
    public static final int H_PUBLISHED = 0;
    /** u64. The last hand-over number the consumer has finished with; written with release. */
    public static final int H_CONSUMED = 8;
    /** u32. Bytes in the record area. Written once by whoever allocates the buffer (the engine). */
    public static final int H_CAPACITY = 16;
    /** u32. Bytes of records written, from {@link #HEADER_BYTES}. Written before publishing. */
    public static final int H_USED = 20;
    /** u32. Number of records. Written before publishing. */
    public static final int H_COUNT = 24;
    /** u32. Direction-specific flags (below). Written before publishing. */
    public static final int H_FLAGS = 28;
    /**
     * u64. Observations: the last command sequence Java has applied ({@code lastAppliedCmdSeq}).
     * Commands: the highest observation sequence the engine's saved pages include
     * ({@code ackedObsSeq}); Java trims its journal up to it.
     */
    public static final int H_WORD_A = 32;
    /** i64. Observations: the game time at the hand-over. Commands: the game time the step reached. */
    public static final int H_WORD_B = 40;
    /** u64. Observations: the highest obsSeq in the buffer (0 if none). Commands: the highest cmdSeq. */
    public static final int H_WORD_C = 48;
    /** u64. Reserved, zero. */
    public static final int H_RESERVED = 56;

    /** Observation flag: the first hand-over of a session. Every embodied agent is now abstract. */
    public static final int OBS_FLAG_SESSION_START = 1;
    /** Observation flag: the buffer holds journal entries replayed from an earlier session. */
    public static final int OBS_FLAG_REPLAY = 2;
    /** Observation flag: latest-wins records were dropped for space since the last hand-over. */
    public static final int OBS_FLAG_DROPPED = 4;

    /** Command flag: the engine holds more commands in its overflow ring; they follow in later steps. */
    public static final int CMD_FLAG_MORE = 1;
    /**
     * Command flag: the engine failed (a panic or a protocol error) and simulates nothing more this
     * session; its buffers come back empty. The bridge stops for the session (B30) when it reads it.
     */
    public static final int CMD_FLAG_FAILED = 2;

    // ---------------------------------------------------------------------------------------------
    // Record header: 8 bytes.
    // ---------------------------------------------------------------------------------------------

    /** u16. Record kind: {@link Obs} kinds are below 0x8000, {@link Cmd} kinds at or above it. */
    public static final int R_KIND = 0;
    /** u16. Record size in bytes, header included; a multiple of 8. */
    public static final int R_SIZE = 2;
    /** u32. A small kind-specific field (a reason, a member index, a dictionary kind). */
    public static final int R_AUX = 4;
    public static final int RECORD_HEADER = 8;
    public static final int RECORD_ALIGN = 8;
    /** The largest record size a u16 can hold that is a multiple of 8. */
    public static final int MAX_RECORD = 0xFFF8;

    /**
     * The smallest record area an engine should allocate: the largest record, plus room for the
     * SESSION_START and TICK records that may precede it in an empty buffer. A journaled record that
     * cannot fit in an empty buffer would hold the journal's stream up forever.
     */
    public static final int MIN_CAPACITY = MAX_RECORD + 128;

    /** Rounds a byte count up to the record alignment. */
    public static int align(int bytes) {
        return (bytes + (RECORD_ALIGN - 1)) & -RECORD_ALIGN;
    }
}
