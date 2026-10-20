package com.minecraftindustrialcomplex.mic_rtsim.journal;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.RecordWriter;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * The journal (A9, PLAN.md §4.9): what keeps the sim and Minecraft consistent across saves and
 * crashes. It lives in Minecraft's own level save ({@link JournalSavedData}) and holds:
 * <ul>
 *   <li>the <b>last applied command sequence</b>: Java skips any command at or below it;</li>
 *   <li><b>every observation the engine has not yet acknowledged</b>, as the exact record bytes that
 *       go into a buffer. The engine acknowledges an observation once a page that includes it is on
 *       disk; the journal is then trimmed up to it;</li>
 *   <li>the next observation sequence, so sequences are never reused in this world;</li>
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
    /** Op ids of engine commands applied in Minecraft whose COMMAND_RESULT is still unacknowledged. */
    private final Set<Long> appliedCmdOps = new HashSet<>();
    private final List<DictEntry> dict = new ArrayList<>();
    private final Map<DictEntry, Integer> dictIds = new HashMap<>();
    private Runnable onChange = () -> {};

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
        return appliedCmdOps.contains(opId);
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
        int kind = Short.toUnsignedInt(record.getShort(Layout.R_KIND));
        if (kind == Obs.COMMAND_RESULT && record.getInt(Layout.R_AUX) == Obs.CommandResult.APPLIED)
            appliedCmdOps.add(record.getLong(Obs.CommandResult.OP_ID));
        onChange.run();
        return seq;
    }

    /**
     * Like {@link #commit}, and writes the record's Java op id at {@code opIdOffset}: its own sequence
     * with {@link Obs#JAVA_OP_BIT} set. Returns the op id.
     */
    public long commitWithOpId(ByteBuffer record, int opIdOffset) {
        long op = Obs.JAVA_OP_BIT | nextObsSeq;
        record.putLong(opIdOffset, op);
        commit(record);
        return op;
    }

    /** Drops every entry the engine acknowledged (its pages on disk include them). */
    public void trim(long ackedObsSeq) {
        boolean changed = false;
        while (!entries.isEmpty() && entries.peekFirst().seq() <= ackedObsSeq) {
            Entry e = entries.removeFirst();
            entryBytes -= e.bytes().length;
            forgetOp(e);
            changed = true;
        }
        if (changed)
            onChange.run();
    }

    private void forgetOp(Entry e) {
        ByteBuffer b = ByteBuffer.wrap(e.bytes()).order(Layout.ORDER);
        if (Short.toUnsignedInt(b.getShort(Layout.R_KIND)) == Obs.COMMAND_RESULT
                && b.getInt(Layout.R_AUX) == Obs.CommandResult.APPLIED)
            appliedCmdOps.remove(b.getLong(Obs.CommandResult.OP_ID));
    }

    /**
     * Copies entries with a sequence above {@code afterSeq}, in order, while they fit. Returns the last
     * sequence copied, or {@code afterSeq} when none was.
     */
    public long drainInto(RecordWriter writer, long afterSeq) {
        long last = afterSeq;
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

    /** A plain snapshot of the journal for encoding. */
    public record Snapshot(long lastAppliedCmdSeq, long nextObsSeq, long[] seqs, byte[][] records,
                           int[] dictKinds, String[] dictNames) {}

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
        return new Snapshot(lastAppliedCmdSeq, nextObsSeq, seqs, records, kinds, names);
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
            if (Short.toUnsignedInt(b.getShort(Layout.R_KIND)) == Obs.COMMAND_RESULT
                    && b.getInt(Layout.R_AUX) == Obs.CommandResult.APPLIED)
                j.appliedCmdOps.add(b.getLong(Obs.CommandResult.OP_ID));
        }
        for (int d = 0; d < s.dictNames().length; d++) {
            DictEntry key = new DictEntry(s.dictKinds()[d], s.dictNames()[d]);
            j.dict.add(key);
            j.dictIds.put(key, j.dict.size());
        }
        return j;
    }

    /** Sanity: entries are strictly increasing and below the next sequence. */
    public boolean consistent() {
        long prev = 0;
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
