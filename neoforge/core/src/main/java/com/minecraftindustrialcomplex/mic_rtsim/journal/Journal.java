package com.minecraftindustrialcomplex.mic_rtsim.journal;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.RecordWriter;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/**
 * The journal (A9, PLAN.md §4.9): what keeps the sim and Minecraft consistent across saves and
 * crashes. It lives in Minecraft's own level save ({@link JournalSavedData}) and holds:
 * <ul>
 *   <li>the <b>last applied command sequence</b>: Java skips any command at or below it;</li>
 *   <li><b>every observation the engine has not yet acknowledged</b>, as the exact record bytes that
 *       go into a buffer. The engine acknowledges an observation once a page that includes it is on
 *       disk; the journal is then trimmed up to it;</li>
 *   <li>the next observation sequence, so sequences are never reused in this world;</li>
 *   <li>the newest JOURNAL_GEN the engine acknowledged, kept past the trim ({@link #pinnedGen()}): the
 *       entries after it (up to the next JOURNAL_GEN among them) are that generation's, and an engine whose
 *       pages are older than the journal must hear which generation they are before it judges a copy of one
 *       (docs/boundary.md §3.2);</li>
 *   <li>Java's dictionary (dimension, item and body-kind names behind the ids observations use),
 *       because a journaled record replayed in a later session must still mean the same thing.</li>
 * </ul>
 *
 * <p><b>Replay is the normal path.</b> Journaled observations always reach the engine from here: at
 * each hand-over the bridge copies the entries it has not sent yet this session. At a session start
 * none have been sent, so everything unacknowledged goes out again. The engine skips what its pages
 * already include (by sequence) and applies an operation id at most once, so a replay is idempotent.
 *
 * <p>Pure Java; appending is O(1) in memory and the server thread never waits on it.
 */
public final class Journal {

    /** One journaled observation: its sequence and its full record bytes. */
    public record Entry(long seq, byte[] bytes) {}

    /** A dictionary entry: kind ({@link Obs#DICT_DIMENSION} and so on) and name. Its id is its index + 1. */
    public record DictEntry(int kind, String name) {}

    private long lastAppliedCmdSeq;
    private long nextObsSeq = 1;
    private final ArrayDeque<Entry> entries = new ArrayDeque<>();
    private long entryBytes;
    /** The newest acknowledged JOURNAL_GEN ({@link #pinnedGen()}), or null. Always below every entry. */
    private Entry pinnedGen;
    /**
     * Op ids of engine commands applied in Minecraft whose COMMAND_RESULT is still unacknowledged, each
     * with how many entries hold it (a COMMAND_RESULT and copies of it, REASSERT, trimmed one by one).
     */
    private final Map<Long, Integer> appliedCmdOps = new HashMap<>();
    private final List<DictEntry> dict = new ArrayList<>();
    private final Map<DictEntry, Integer> dictIds = new HashMap<>();
    private Runnable onChange = () -> {};
    /**
     * Java op ids are random, not the record's sequence: a crash can roll the journal back while a
     * chunk saved on unload keeps a record (and its op id) from the lost stretch, and that stretch's
     * sequences are then handed out again. A random 63-bit number never collides in practice.
     */
    private final java.util.SplittableRandom opIds = new java.util.SplittableRandom(new java.security.SecureRandom().nextLong());

    public void onChange(Runnable onChange) {
        this.onChange = onChange;
    }

    // ---- command side ---------------------------------------------------------------------------

    public long lastAppliedCmdSeq() {
        return lastAppliedCmdSeq;
    }

    public void markCommandApplied(long cmdSeq) {
        if (cmdSeq > lastAppliedCmdSeq) {
            lastAppliedCmdSeq = cmdSeq;
            onChange.run();
        }
    }

    /** True when an engine op id was already applied in Minecraft and the engine may not know yet. */
    public boolean commandOpApplied(long opId) {
        return appliedCmdOps.containsKey(opId);
    }

    /** GameTests: a crash rolled the journal back to before this op id was applied. */
    public void forgetAppliedForTest(long opId) {
        appliedCmdOps.remove(opId);
    }

    // ---- observation side -----------------------------------------------------------------------

    public long nextObsSeq() {
        return nextObsSeq;
    }

    /** Moves the next sequence past what the engine's pages include (SessionStart.pagesObsSeq). */
    public void reserveBeyond(long pagesObsSeq) {
        if (pagesObsSeq + 1 > nextObsSeq) {
            nextObsSeq = pagesObsSeq + 1;
            onChange.run();
        }
    }

    /**
     * Starts a journaled record of {@code size} bytes. Write the body at offsets from
     * {@link Obs#BODY} into the returned buffer, then {@link #commit} it. The sequence is assigned at
     * commit, so a record that is never committed costs no sequence.
     */
    public ByteBuffer begin(int kind, int size, int aux, long gameTime) {
        if (!Obs.isJournaled(kind))
            throw new IllegalArgumentException("not a journaled kind: 0x" + Integer.toHexString(kind));
        int aligned = Layout.align(size);
        if (aligned > Layout.MAX_RECORD)
            throw new IllegalArgumentException("record too large: " + size);
        ByteBuffer b = ByteBuffer.allocate(aligned).order(Layout.ORDER);
        b.putShort(Layout.R_KIND, (short) kind);
        b.putShort(Layout.R_SIZE, (short) aligned);
        b.putInt(Layout.R_AUX, aux);
        b.putLong(Obs.GAME_TIME, gameTime);
        return b;
    }

    /** Assigns the next sequence to a record from {@link #begin} and appends it. Returns the sequence. */
    public long commit(ByteBuffer record) {
        long seq = nextObsSeq++;
        record.putLong(Obs.OBS_SEQ, seq);
        byte[] bytes = record.array();
        entries.addLast(new Entry(seq, bytes));
        entryBytes += bytes.length;
        int opAt = appliedOpAt(record);
        if (opAt >= 0)
            appliedCmdOps.merge(record.getLong(opAt), 1, Integer::sum);
        onChange.run();
        return seq;
    }

    /**
     * Like {@link #commit}, and writes a new Java op id at {@code opIdOffset}: 63 random bits with
     * {@link Obs#JAVA_OP_BIT} set. Returns the op id.
     */
    public long commitWithOpId(ByteBuffer record, int opIdOffset) {
        long op;
        do {
            op = Obs.JAVA_OP_BIT | (opIds.nextLong() >>> 1);
        } while (op == Obs.JAVA_OP_BIT);
        record.putLong(opIdOffset, op);
        commit(record);
        return op;
    }

    /**
     * Journals a record again: the same bytes (op id and game time included) under a new sequence.
     * Used when a chunk or entity that carries a record loads. Returns the new sequence.
     */
    public long recommit(byte[] recordBytes) {
        ByteBuffer copy = ByteBuffer.wrap(recordBytes.clone()).order(Layout.ORDER);
        return commit(copy);
    }

    /**
     * Drops every entry the engine acknowledged (its pages on disk include them), except the newest
     * JOURNAL_GEN among them, which becomes {@link #pinnedGen()}.
     */
    public void trim(long ackedObsSeq) {
        boolean changed = false;
        while (!entries.isEmpty() && entries.peekFirst().seq() <= ackedObsSeq) {
            Entry e = entries.removeFirst();
            entryBytes -= e.bytes().length;
            if (kindOf(e.bytes()) == Obs.JOURNAL_GEN)
                pinnedGen = e;
            forgetOp(e);
            changed = true;
        }
        if (changed)
            onChange.run();
    }

    /**
     * The newest JOURNAL_GEN the engine acknowledged, or null.
     * Sent first at each session start like an entry ({@link #drainInto}); an engine whose pages include it
     * skips it. Without it, an engine whose pages are older than the journal (restored, or lost after the
     * trim) would admit the entries under the wrong generation, and a copy of one of them (REASSERT)
     * would apply a second time (docs/boundary.md §3.2). The entries after it, up to the next JOURNAL_GEN
     * among them, are its generation's. Not an entry: not in {@link #size}, {@link #bytes},
     * {@link #entries} or {@link #firstSeq}.
     */
    public Entry pinnedGen() {
        return pinnedGen;
    }

    private static int kindOf(byte[] record) {
        return Short.toUnsignedInt(ByteBuffer.wrap(record).order(Layout.ORDER).getShort(Layout.R_KIND));
    }

    private void forgetOp(Entry e) {
        ByteBuffer b = ByteBuffer.wrap(e.bytes()).order(Layout.ORDER);
        int opAt = appliedOpAt(b);
        if (opAt >= 0)
            appliedCmdOps.computeIfPresent(b.getLong(opAt), (op, n) -> n > 1 ? n - 1 : null);
    }

    /**
     * Where a record holds the op id of an engine command Java applied: a COMMAND_RESULT APPLIED's, or
     * the one inside a copy of one (REASSERT, docs/boundary.md §3.2). -1 for any other record.
     */
    static int appliedOpAt(ByteBuffer b) {
        int base = 0;
        if (Short.toUnsignedInt(b.getShort(Layout.R_KIND)) == Obs.REASSERT)
            base = Obs.Reassert.INNER;
        if (b.limit() < base + Obs.CommandResult.SIZE)
            return -1;
        if (Short.toUnsignedInt(b.getShort(base + Layout.R_KIND)) == Obs.COMMAND_RESULT
                && b.getInt(base + Layout.R_AUX) == Obs.CommandResult.APPLIED)
            return base + Obs.CommandResult.OP_ID;
        return -1;
    }

    /**
     * Copies entries with a sequence above {@code afterSeq}, in order, while they fit, the pinned
     * generation ({@link #pinnedGen()}) first. Returns the last sequence copied, or {@code afterSeq} when
     * none was.
     */
    public long drainInto(RecordWriter writer, long afterSeq) {
        long last = afterSeq;
        if (pinnedGen != null && pinnedGen.seq() > afterSeq) {
            if (!writer.copy(pinnedGen.bytes()))
                return last;
            last = pinnedGen.seq();
        }
        for (Entry e : entries) {
            if (e.seq() <= afterSeq)
                continue;
            if (!writer.copy(e.bytes()))
                break;
            last = e.seq();
        }
        return last;
    }

    public int size() {
        return entries.size();
    }

    public long bytes() {
        return entryBytes;
    }

    public Iterable<Entry> entries() {
        return entries;
    }

    public long firstSeq() {
        return entries.isEmpty() ? 0 : entries.peekFirst().seq();
    }

    // ---- dictionary -----------------------------------------------------------------------------

    /** The id of a name in Java's dictionary, assigning the next one if it is new. */
    public int dictId(int kind, String name) {
        DictEntry key = new DictEntry(kind, name);
        Integer id = dictIds.get(key);
        if (id != null)
            return id;
        dict.add(key);
        int assigned = dict.size();
        dictIds.put(key, assigned);
        onChange.run();
        return assigned;
    }

    public int dictSize() {
        return dict.size();
    }

    public DictEntry dictEntry(int id) {
        return dict.get(id - 1);
    }

    /** Writes the DICT record for id {@code id}. Returns false when it does not fit. */
    public boolean writeDict(RecordWriter writer, int id) {
        DictEntry e = dictEntry(id);
        byte[] name = e.name().getBytes(StandardCharsets.UTF_8);
        int at = writer.reserve(Obs.DICT, Obs.Dict.size(name.length), e.kind());
        if (at < 0)
            return false;
        ByteBuffer b = writer.buffer();
        b.putInt(at + Obs.Dict.ID, id);
        b.putInt(at + Obs.Dict.NAME_LEN, name.length);
        writer.putBytes(at + Obs.Dict.NAME, name);
        return true;
    }

    // ---- persistence (the SavedData encodes this snapshot as NBT) --------------------------------

    /** A plain snapshot of the journal for encoding. {@code pinnedGen}: {@link #pinnedGen()}'s record, or null. */
    public record Snapshot(long lastAppliedCmdSeq, long nextObsSeq, long[] seqs, byte[][] records,
                           int[] dictKinds, String[] dictNames, byte[] pinnedGen) {}

    public Snapshot snapshot() {
        long[] seqs = new long[entries.size()];
        byte[][] records = new byte[entries.size()][];
        int i = 0;
        for (Entry e : entries) {
            seqs[i] = e.seq();
            records[i] = e.bytes();
            i++;
        }
        int[] kinds = new int[dict.size()];
        String[] names = new String[dict.size()];
        for (int d = 0; d < dict.size(); d++) {
            kinds[d] = dict.get(d).kind();
            names[d] = dict.get(d).name();
        }
        return new Snapshot(lastAppliedCmdSeq, nextObsSeq, seqs, records, kinds, names,
                pinnedGen == null ? null : pinnedGen.bytes());
    }

    public static Journal restore(Snapshot s) {
        Journal j = new Journal();
        j.lastAppliedCmdSeq = s.lastAppliedCmdSeq();
        j.nextObsSeq = Math.max(1, s.nextObsSeq());
        for (int i = 0; i < s.seqs().length; i++) {
            Entry e = new Entry(s.seqs()[i], s.records()[i]);
            j.entries.addLast(e);
            j.entryBytes += e.bytes().length;
            ByteBuffer b = ByteBuffer.wrap(e.bytes()).order(Layout.ORDER);
            int opAt = appliedOpAt(b);
            if (opAt >= 0)
                j.appliedCmdOps.merge(b.getLong(opAt), 1, Integer::sum);
        }
        byte[] pinned = s.pinnedGen();
        if (pinned != null) {
            if (pinned.length < Obs.JournalGen.SIZE || kindOf(pinned) != Obs.JOURNAL_GEN)
                throw new IllegalStateException("mic_rtsim journal is corrupt: its kept generation is not a JOURNAL_GEN");
            j.pinnedGen = new Entry(ByteBuffer.wrap(pinned).order(Layout.ORDER).getLong(Obs.OBS_SEQ), pinned);
            long first = j.entries.isEmpty() ? j.nextObsSeq : j.entries.peekFirst().seq();
            if (j.pinnedGen.seq() <= 0 || j.pinnedGen.seq() >= first)
                throw new IllegalStateException("mic_rtsim journal is corrupt: its kept generation " + j.pinnedGen.seq()
                        + " is not below its first entry " + first);
        }
        for (int d = 0; d < s.dictNames().length; d++) {
            DictEntry key = new DictEntry(s.dictKinds()[d], s.dictNames()[d]);
            j.dict.add(key);
            j.dictIds.put(key, j.dict.size());
        }
        return j;
    }

    /** Sanity: entries are strictly increasing and below the next sequence, the pinned generation below them. */
    public boolean consistent() {
        long prev = 0;
        if (pinnedGen != null) {
            if (pinnedGen.seq() <= 0 || pinnedGen.seq() >= nextObsSeq || kindOf(pinnedGen.bytes()) != Obs.JOURNAL_GEN)
                return false;
            prev = pinnedGen.seq();
        }
        Iterator<Entry> it = entries.iterator();
        while (it.hasNext()) {
            long s = it.next().seq();
            if (s <= prev || s >= nextObsSeq)
                return false;
            prev = s;
        }
        return true;
    }
}
