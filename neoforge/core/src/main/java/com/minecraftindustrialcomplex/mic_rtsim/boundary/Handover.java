package com.minecraftindustrialcomplex.mic_rtsim.boundary;

import java.nio.ByteBuffer;

/**
 * Java's half of the hand-over protocol (PLAN.md §4.5): which buffer is open for observations, which
 * command buffer is being applied, and when a new step may be submitted. Runs on the server thread
 * only. Holds no Minecraft types, so it is unit-tested against the stub engine directly.
 *
 * <p>Per tick, the bridge calls {@link #drainCommands} (apply what finished steps produced, as far as
 * quotas allow), writes the tick's observations through {@link #open()}, and calls
 * {@link #trySubmit}. Nothing here ever waits.
 */
public final class Handover {

    /** Applies commands. Return false from {@link #accept} to stop for this tick (a quota is spent). */
    public interface CommandSink {
        /** The header of a newly finished command buffer, read once before its first record. */
        void onHeader(long handover, long ackedObsSeq, long gameTime, int flags);

        /**
         * One command record at {@code at} in {@code buffer}. Return true when it is done with (applied,
         * rejected or skipped), false to leave it, and everything after it, for a later tick.
         */
        boolean accept(ByteBuffer buffer, int at, int kind, int size, int aux);

        /** A malformed record; the rest of that buffer is skipped. */
        void onMalformed(long handover, String error);
    }

    private final Engine engine;
    private final ByteBuffer[] obs = new ByteBuffer[2];
    private final ByteBuffer[] cmd = new ByteBuffer[2];
    private final RecordWriter open = new RecordWriter();
    private final RecordReader reader = new RecordReader();

    /** The last hand-over submitted; 0 before the first. */
    private long submitted;
    /** The next hand-over whose commands are to be applied; every earlier one is fully applied. */
    private long nextCmd = 1;
    /** Where applying stopped in command buffer {@code nextCmd & 1}; -1 when it is not opened yet. */
    private int cmdCursor = -1;
    private int cmdEnd;
    /** Flags for the open buffer's header. */
    private int openFlags;
    private long openHighestObsSeq;

    // Counters for /rtsim perf and the tests.
    private long ticksWaiting;
    private long submits;

    public Handover(Engine engine) {
        this.engine = engine;
        for (int i = 0; i < 2; i++) {
            obs[i] = Buffers.view(engine.observationBuffer(i));
            cmd[i] = Buffers.view(engine.commandBuffer(i));
        }
        open.reset(obs[1]); // hand-over 1 uses observation buffer 1 & 1
    }

    /** The open observation buffer: everything written here goes out with the next hand-over. */
    public RecordWriter open() {
        return open;
    }

    public void flagOpen(int flag) {
        openFlags |= flag;
    }

    public void noteObsSeq(long seq) {
        if (seq > openHighestObsSeq)
            openHighestObsSeq = seq;
    }

    public long submitted() {
        return submitted;
    }

    public boolean stepInFlight() {
        return submitted > 0 && Seq.getAcquire(cmd[(int) (submitted & 1)], Layout.H_PUBLISHED) != submitted;
    }

    public long ticksWaiting() {
        return ticksWaiting;
    }

    public long submits() {
        return submits;
    }

    /** True when a finished command buffer still has records left (a quota stopped it). */
    public boolean commandsPending() {
        return cmdCursor >= 0 && cmdCursor < cmdEnd;
    }

    /** Applies commands from finished steps, oldest first, until the sink defers or none are left. */
    public void drainCommands(CommandSink sink) {
        while (nextCmd <= submitted) {
            ByteBuffer c = cmd[(int) (nextCmd & 1)];
            if (cmdCursor < 0) {
                if (Seq.getAcquire(c, Layout.H_PUBLISHED) != nextCmd)
                    return; // that step is still running
                int used = c.getInt(Layout.H_USED);
                if (used < 0 || used > Buffers.capacity(c)) {
                    sink.onMalformed(nextCmd, "command buffer used=" + used + " beyond capacity");
                    used = 0;
                }
                sink.onHeader(nextCmd, c.getLong(Layout.H_WORD_A), c.getLong(Layout.H_WORD_B), c.getInt(Layout.H_FLAGS));
                cmdCursor = Layout.HEADER_BYTES;
                cmdEnd = Layout.HEADER_BYTES + used;
            }
            reader.reset(c, cmdCursor, cmdEnd - Layout.HEADER_BYTES);
            while (reader.next()) {
                if (!sink.accept(c, reader.at(), reader.kind(), reader.size(), reader.aux())) {
                    cmdCursor = reader.at();
                    return;
                }
                cmdCursor = reader.at() + reader.size();
            }
            if (reader.error() != null)
                sink.onMalformed(nextCmd, reader.error());
            Seq.setRelease(c, Layout.H_CONSUMED, nextCmd);
            nextCmd++;
            cmdCursor = -1;
        }
    }

    /**
     * Hands the open observation buffer to the engine and starts the next step, if the protocol
     * allows it this tick. Returns true when a step was submitted.
     *
     * @param gameTime          the game time now
     * @param lastAppliedCmdSeq the journal's last applied command sequence
     */
    public boolean trySubmit(long gameTime, long lastAppliedCmdSeq) {
        long h = submitted + 1;
        if (submitted > 0 && Seq.getAcquire(cmd[(int) (submitted & 1)], Layout.H_PUBLISHED) != submitted) {
            ticksWaiting++;
            return false; // step h-1 still running: keep collecting observations
        }
        if (nextCmd < h - 1) {
            ticksWaiting++;
            return false; // command buffer h & 1 still holds commands of step h-2 that Java has not applied
        }
        ByteBuffer o = obs[(int) (h & 1)];
        o.putInt(Layout.H_USED, open.used());
        o.putInt(Layout.H_COUNT, open.count());
        o.putInt(Layout.H_FLAGS, openFlags);
        o.putLong(Layout.H_WORD_A, lastAppliedCmdSeq);
        o.putLong(Layout.H_WORD_B, gameTime);
        o.putLong(Layout.H_WORD_C, openHighestObsSeq);
        o.putLong(Layout.H_RESERVED, 0L);
        Seq.setRelease(o, Layout.H_PUBLISHED, h);
        submitted = h;
        submits++;
        engine.submit(h);

        // The next open buffer last held hand-over h-1, which step h-1 consumed before it finished.
        ByteBuffer next = obs[(int) ((h + 1) & 1)];
        long consumed = Seq.getAcquire(next, Layout.H_CONSUMED);
        if (consumed != h - 1)
            throw new IllegalStateException("engine protocol violation: observation buffer " + ((h + 1) & 1)
                    + " consumed=" + consumed + ", expected " + (h - 1) + " before step " + (h - 1) + " finished");
        open.reset(next);
        openFlags = 0;
        openHighestObsSeq = 0;
        return true;
    }
}
