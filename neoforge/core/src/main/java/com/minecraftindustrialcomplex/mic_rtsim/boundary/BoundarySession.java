package com.minecraftindustrialcomplex.mic_rtsim.boundary;

import com.minecraftindustrialcomplex.mic_rtsim.journal.Journal;

import java.nio.ByteBuffer;

/**
 * One session of the boundary: the engine, the hand-over state and the journal, glued by the rules
 * of PLAN.md §4.5 and §4.9. Pure Java; the Minecraft side ({@code RtsimSession}) owns one and feeds it
 * observations and a command sink.
 *
 * <p><b>Order inside a buffer.</b> A session's first buffer starts with SESSION_START. Dictionary
 * entries go out before anything that could use them: a latest-wins record is written only once
 * every dictionary entry has been sent, and journal entries are drained only after the dictionary.
 * So an id is always defined in the same or an earlier hand-over than its first use.
 *
 * <p><b>Nothing journaled is ever dropped.</b> A journaled record is appended to the journal and
 * copied into the open buffer at the next hand-over that has room. Latest-wins records that do not
 * fit are dropped and counted; the bridge sends them again at its next pass.
 */
public final class BoundarySession {

    private final Engine engine;
    private final Journal journal;
    private final Handover handover;
    private final SessionStart start;
    /** The last journal sequence before this session; entries up to it are a replay. */
    private final long replayUpTo;

    private long sentObsSeq;
    private int dictSent;
    private long tickMarked = Long.MIN_VALUE;
    private boolean sessionStartPending = true;
    private int moduleMask;
    private boolean modulesDirty = true;
    private final long gameTimeAtStart;
    private final long journalFirstAtStart;
    private final long worldNextAtStart;
    private long durableReported;
    private volatile long durablePending;

    private long droppedRecords;
    private long skippedStaleCommands;
    private long malformed;
    private String lastError;

    public BoundarySession(Engine engine, Journal journal, int moduleMask, long gameTime) {
        this.engine = engine;
        this.journal = journal;
        this.moduleMask = moduleMask;
        this.gameTimeAtStart = gameTime;
        this.journalFirstAtStart = journal.firstSeq();
        this.worldNextAtStart = journal.nextObsSeq();
        this.start = engine.start(journal.lastAppliedCmdSeq());
        journal.reserveBeyond(start.pagesObsSeq());
        this.replayUpTo = journal.nextObsSeq() - 1;
        this.handover = new Handover(engine);
    }

    public Engine engine() {
        return engine;
    }

    public Journal journal() {
        return journal;
    }

    public Handover handover() {
        return handover;
    }

    public SessionStart start() {
        return start;
    }

    public void setModuleMask(int mask) {
        if (mask != moduleMask) {
            moduleMask = mask;
            modulesDirty = true;
        }
    }

    // ---- observations ---------------------------------------------------------------------------

    /** Starts a journaled observation; fill it from {@link Obs#BODY} and {@link #commit} it. */
    public ByteBuffer beginJournaled(int kind, int size, int aux, long gameTime) {
        return journal.begin(kind, size, aux, gameTime);
    }

    /** Appends a journaled observation. Returns its sequence. */
    public long commit(ByteBuffer record) {
        return journal.commit(record);
    }

    /**
     * Appends a journaled observation that carries a Java-side operation id at {@code opIdOffset}. The
     * op id is the record's own sequence, top bit clear: unique in the world, and the same bytes on
     * every replay. Returns the op id.
     */
    public long commitWithOpId(ByteBuffer record, int opIdOffset) {
        return journal.commitWithOpId(record, opIdOffset);
    }

    /** The id of a dimension, item or body-kind name in Java's dictionary. */
    public int dictId(int dictKind, String name) {
        return journal.dictId(dictKind, name);
    }

    /**
     * Reserves a latest-wins record in the open buffer for this tick. Returns its offset, or -1 when it
     * was dropped for space (counted). Write fields with absolute puts on {@link #openBuffer()}.
     */
    public int direct(int kind, int size, int aux, long gameTime) {
        if (!flushPreamble(gameTime)) {
            drop();
            return -1;
        }
        int at = handover.open().reserve(kind, size, aux);
        if (at < 0)
            drop();
        return at;
    }

    public ByteBuffer openBuffer() {
        return handover.open().buffer();
    }

    private void drop() {
        droppedRecords++;
        handover.flagOpen(Layout.OBS_FLAG_DROPPED);
    }

    private boolean flushPreamble(long gameTime) {
        RecordWriter w = handover.open();
        ByteBuffer b = w.buffer();
        if (sessionStartPending) {
            int at = w.reserve(Obs.SESSION_START, Obs.SessionStart.SIZE, 0);
            if (at < 0)
                return false;
            b.putLong(at + Obs.SessionStart.GAME_TIME, gameTimeAtStart);
            b.putLong(at + Obs.SessionStart.LAST_APPLIED_CMD_SEQ, journal.lastAppliedCmdSeq());
            b.putLong(at + Obs.SessionStart.JOURNAL_FIRST_OBS_SEQ, journalFirstAtStart);
            b.putLong(at + Obs.SessionStart.WORLD_NEXT_OBS_SEQ, worldNextAtStart);
            b.putInt(at + Obs.SessionStart.MODULE_MASK, moduleMask);
            handover.flagOpen(Layout.OBS_FLAG_SESSION_START);
            sessionStartPending = false;
        }
        while (dictSent < journal.dictSize()) {
            if (!journal.writeDict(w, dictSent + 1))
                return false;
            dictSent++;
        }
        if (modulesDirty) {
            int at = w.reserve(Obs.MODULES, Obs.Modules.SIZE, 0);
            if (at < 0)
                return false;
            b.putInt(at + Obs.Modules.MASK, moduleMask);
            modulesDirty = false;
        }
        if (tickMarked != gameTime) {
            int at = w.reserve(Obs.TICK, Obs.Tick.SIZE, 0);
            if (at < 0)
                return false;
            b.putLong(at + Obs.Tick.GAME_TIME, gameTime);
            tickMarked = gameTime;
        }
        return true;
    }

    /**
     * The world save that captured the journal at {@code nextObsSeq - 1} is now on disk. May be called
     * from an IO thread; the WORLD_SAVED record goes out at the next hand-over.
     */
    public void worldSaveDurable(long durableObsSeq) {
        if (durableObsSeq > durablePending)
            durablePending = durableObsSeq;
    }

    private void reportDurable(long gameTime) {
        long d = durablePending;
        if (d <= durableReported)
            return;
        int at = direct(Obs.WORLD_SAVED, Obs.WorldSaved.SIZE, 0, gameTime);
        if (at >= 0) {
            openBuffer().putLong(at + Obs.WorldSaved.DURABLE_OBS_SEQ, d);
            durableReported = d;
        }
    }

    // ---- the end of a tick ----------------------------------------------------------------------

    /**
     * The first half of the bridge's per-tick boundary work: apply the commands of finished steps
     * through {@code sink}, oldest first, until the sink defers. Stale sequences are skipped, applied
     * ones are recorded in the journal, and each new command header's acknowledgement trims the
     * journal. Never waits.
     */
    public void applyCommands(Handover.CommandSink sink) {
        handover.drainCommands(new Handover.CommandSink() {
            @Override
            public void onHeader(long h, long ackedObsSeq, long stepTime, int flags) {
                journal.trim(ackedObsSeq);
                sink.onHeader(h, ackedObsSeq, stepTime, flags);
            }

            @Override
            public boolean accept(ByteBuffer buffer, int at, int kind, int size, int aux) {
                if (size < Cmd.BODY || kind < 0x8000) {
                    malformed++;
                    return true;
                }
                long cmdSeq = buffer.getLong(at + Cmd.CMD_SEQ);
                if (cmdSeq <= journal.lastAppliedCmdSeq()) {
                    skippedStaleCommands++;
                    return true;
                }
                if (!sink.accept(buffer, at, kind, size, aux))
                    return false;
                journal.markCommandApplied(cmdSeq);
                return true;
            }

            @Override
            public void onMalformed(long h, String error) {
                malformed++;
                lastError = "hand-over " + h + ": " + error;
                sink.onMalformed(h, error);
            }
        });
    }

    /**
     * The second half: copy unsent journal entries into the open buffer (after the dictionary), and
     * submit the next step if the protocol allows it this tick. Never waits. Returns true when a step
     * was submitted.
     */
    public boolean handOver(long gameTime) {
        reportDurable(gameTime);
        if (flushPreamble(gameTime)) {
            long last = journal.drainInto(handover.open(), sentObsSeq);
            if (last > sentObsSeq) {
                if (sentObsSeq < replayUpTo)
                    handover.flagOpen(Layout.OBS_FLAG_REPLAY);
                handover.noteObsSeq(last);
                sentObsSeq = last;
            }
        }
        return handover.trySubmit(gameTime, journal.lastAppliedCmdSeq());
    }

    // ---- counters -------------------------------------------------------------------------------

    public long droppedRecords() {
        return droppedRecords;
    }

    public long skippedStaleCommands() {
        return skippedStaleCommands;
    }

    public long malformed() {
        return malformed;
    }

    public String lastError() {
        return lastError;
    }

    public long sentObsSeq() {
        return sentObsSeq;
    }

    public long replayUpTo() {
        return replayUpTo;
    }
}
