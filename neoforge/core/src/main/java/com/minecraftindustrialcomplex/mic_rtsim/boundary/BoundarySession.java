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
    /** This session's generation (docs/boundary.md §2.6): its JOURNAL_GEN's nonce, random and never 0. */
    private final long gen;
    /** Copies too large to wrap, journaled again plainly ({@link #reassert}). */
    private long plainRecommits;

    private long sentObsSeq;
    private int dictSent;
    private long tickMarked = Long.MIN_VALUE;
    private boolean sessionStartPending = true;
    private int moduleMask;
    private boolean modulesDirty = true;
    private final long gameTimeAtStart;
    private final long journalFirstAtStart;
    private final long worldNextAtStart;
    /** The engine's pages lack observations the journal no longer holds ({@link #observationsLost()}). */
    private final boolean observationsLost;
    private long durableReported;
    private volatile long durablePending;

    /**
     * Sticky latest-wins records written right after the dictionary in the next hand-over, so before
     * anything journaled it carries (a level's TOPOLOGY, docs/boundary.md §11). Keyed by kind and aux:
     * a newer one replaces one not written yet. Each value is the record's bytes after its header.
     */
    private final java.util.LinkedHashMap<Long, byte[]> preamble = new java.util.LinkedHashMap<>();
    private final java.util.Map<Long, Integer> preambleKinds = new java.util.HashMap<>();

    private long pendingAck;
    private java.util.function.LongConsumer ackListener = acked -> {};
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
        this.observationsLost = observationsLost(start.pagesObsSeq(), journalFirstAtStart, worldNextAtStart);
        journal.reserveBeyond(start.pagesObsSeq());
        this.replayUpTo = journal.nextObsSeq() - 1;
        // The session's first record names its generation (the engine's session_first is its sequence).
        this.gen = newGen();
        ByteBuffer g = journal.begin(Obs.JOURNAL_GEN, Obs.JournalGen.SIZE, 0, gameTime);
        g.putLong(Obs.JournalGen.GEN, gen);
        journal.commit(g);
        this.handover = new Handover(engine);
    }

    private static long newGen() {
        long g;
        do {
            g = new java.security.SecureRandom().nextLong();
        } while (g == 0);
        return g;
    }

    /** This session's generation: the nonce a holder keeps with each record it carries from this session. */
    public long gen() {
        return gen;
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

    /**
     * True when the engine's pages lack observations the journal no longer holds: the pages were deleted
     * (SRV2's sim reset, {@code world/rtsim/} removed while the world kept its journal) or are older than
     * the world's journal (restored from another backup). The engine says the same in its log ("the
     * journal starts at observation N but the pages only include M"). What the bridge announced once and
     * remembers in the world as announced (a district's declaration, a village's adoption, a base's mark)
     * is then unknown to the engine, so the modules announce it again at this session's start.
     */
    public boolean observationsLost() {
        return observationsLost;
    }

    /**
     * The kernel's {@code Replay::reconcile} on Java's side: observations are lost when the journal's
     * first sequence ({@link Journal#firstSeq}, 0 for an empty journal, which the engine reads as the
     * world's next sequence) is past the one after the pages, unless the pages are ahead of the world
     * save (P29), which loses nothing.
     */
    public static boolean observationsLost(long pagesObsSeq, long journalFirst, long worldNext) {
        if (worldNext <= pagesObsSeq)
            return false;
        long first = journalFirst == 0 ? worldNext : journalFirst;
        return first > pagesObsSeq + 1;
    }

    /** Called with each acknowledgement (the highest sequence the engine's saved pages include). */
    public void onAck(java.util.function.LongConsumer listener) {
        this.ackListener = listener;
    }

    /** Journals a record again under a new sequence ({@link Journal#recommit}). */
    public long recommit(byte[] recordBytes) {
        return journal.recommit(recordBytes);
    }

    /**
     * Journals a copy of a record a chunk or entity carried (REASSERT, docs/boundary.md §2.6): {@code original}
     * as first journaled, in generation {@code gen}. The engine applies it only when its state lacks the
     * record, so a holder that loads again before the acknowledgement, or after the engine forgot the op
     * id, applies nothing twice, and a record a crash lost applies once. A record too large to wrap is
     * journaled again plainly, as before generations (counted: {@link #plainRecommits}). Returns the copy's
     * sequence.
     */
    public long reassert(long gen, byte[] original) {
        int size = Obs.Reassert.INNER + original.length;
        if (Layout.align(size) > Layout.MAX_RECORD) {
            plainRecommits++;
            return recommit(original);
        }
        long gameTime = ByteBuffer.wrap(original).order(Layout.ORDER).getLong(Obs.GAME_TIME);
        ByteBuffer r = journal.begin(Obs.REASSERT, size, 0, gameTime);
        r.putLong(Obs.Reassert.GEN, gen);
        r.put(Obs.Reassert.INNER, original);
        return journal.commit(r);
    }

    /** Copies journaled plainly because the copy would not fit a record ({@link #reassert}). */
    public long plainRecommits() {
        return plainRecommits;
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
     * op id is 63 random bits with {@link Obs#JAVA_OP_BIT} set, drawn by the journal and written into the
     * record before it is committed: the engine tells a record it has seen again (a resend, a
     * {@link #recommit}) by it. Returns the op id.
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

    /**
     * Queues a sticky latest-wins record for the next hand-over, written right after the dictionary
     * (before any journal entry it replays): {@code body} is the record after its 8-byte header. A newer
     * record of the same kind and aux replaces one not written yet.
     */
    public void preamble(int kind, int aux, byte[] body) {
        long key = ((long) kind << 32) | (aux & 0xFFFFFFFFL);
        preamble.remove(key);
        preamble.put(key, body.clone());
        preambleKinds.put(key, kind);
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
        while (!preamble.isEmpty()) {
            var it = preamble.entrySet().iterator();
            var e = it.next();
            byte[] body = e.getValue();
            int kind = preambleKinds.get(e.getKey());
            int at = w.reserve(kind, Layout.RECORD_HEADER + body.length, (int) (long) e.getKey());
            if (at < 0)
                return false;
            b.put(at + Layout.RECORD_HEADER, body);
            it.remove();
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

    /** The latest world save known to be on disk, not yet or already reported (the GameTests read it). */
    public long durablePending() {
        return durablePending;
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
                if ((flags & Layout.CMD_FLAG_FAILED) != 0)
                    throw new IllegalStateException("the engine failed at hand-over " + h
                            + " and simulates nothing more this session (" + engine.describe() + ")");
                // Trimmed only once the buffer's commands are applied (onConsumed): if a command in this
                // buffer re-issues an op whose COMMAND_RESULT the acknowledgement covers, the op must
                // still be in the journal's index when that command is checked. And never past what
                // this session sent or the pages held at start: a bad acknowledgement must not drop
                // entries the engine never saw.
                pendingAck = Math.min(ackedObsSeq, Math.max(sentObsSeq, start.pagesObsSeq()));
                sink.onHeader(h, ackedObsSeq, stepTime, flags);
            }

            @Override
            public void onConsumed(long h) {
                if (pendingAck > 0) {
                    journal.trim(pendingAck);
                    ackListener.accept(pendingAck);
                    pendingAck = 0;
                }
                sink.onConsumed(h);
            }

            @Override
            public boolean accept(ByteBuffer buffer, int at, int kind, int size, int aux) {
                if (size < Cmd.BODY || kind < 0x8000) {
                    malformed++;
                    return true;
                }
                long cmdSeq = buffer.getLong(at + Cmd.CMD_SEQ);
                // DICT is idempotent and per session: never skipped, so a dictionary the engine resends
                // under old sequences after a restart still defines its ids.
                if (kind != Cmd.DICT && cmdSeq <= journal.lastAppliedCmdSeq()) {
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
