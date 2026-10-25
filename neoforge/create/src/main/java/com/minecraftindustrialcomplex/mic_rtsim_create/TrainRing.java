package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.journal.Journal;
import com.simibubi.create.Create;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.graph.DimensionPalette;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.MinecraftServer;

import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

/**
 * A Create train's ring of applied IND_CARRIAGE_DELTA op ids in the train's own NBT (RP-190 to RP-194, RP-250
 * to RP-252): an {@link AppliedOpRing} of {@link #CAPACITY} under {@link #KEY}, the save and load hooks
 * {@code ringmixin.TrainRingMixin} calls, and whether that mixin applied.
 *
 * <p>The ring is the dedup of {@link TrainWatch#carriageDelta} for a train that exists (RP-191). The goods
 * and the ring are written by the same {@code Train.write} into create_tracks.dat, so a crash keeps both or
 * neither: the journal's op index, a second SavedData file, made J3 a double and J4 a dupe (RP-184).
 *
 * <p>An op stays in the ring until the engine can no longer send it (RP-250): each answer (COMMAND_RESULT)
 * the ring records with its sequence, and the engine's acknowledgement of that sequence, its durable save
 * holding the answer, removes the op ({@link #tick}). An answer another session gave is settled at this
 * session's start against the engine's pages ({@link #settle}, RP-251): it leaves only when the pages provably
 * hold it, else it stays with its answer unknown. {@link #CAPACITY} is a valve, not the bound: an op pushed
 * out of a full ring is counted and the first on each train is logged.
 *
 * <p>Saving and loading never depend on the industry module's switch (RP-257, amending RP-194): the switch
 * gates only the applying of new ops ({@code carriageDelta} runs only while the module is on). A save writes
 * the ring whenever it holds an op, a load keeps what it finds, the acknowledgements prune while the module
 * is off ({@link #tick} is the session hook's, not the industry's), and the session's settle runs as usual.
 * A save with the module off that left the ring out, followed by a crash before the journal's file was
 * written, would apply the op again once the module was back on.
 *
 * <p>The mixin's config is optional ({@code mic_rtsim_create_ring.mixins.json}: not required, require 0),
 * so a Create that moved {@code Train.write} or {@code Train.read} loads without it. {@link #available}
 * then finds the round trip broken, logs that once and {@code carriageDelta} falls back to the journal's
 * op index, which is slice T's dedup (RP-193).
 */
public final class TrainRing {
    private TrainRing() {}

    /** The NBT key in a Train's compound: a long array of op ids, oldest first (with {@code _at} and {@code _gen} beside it). */
    public static final String KEY = "mic_rtsim:applied_ops";
    /**
     * The valve (RP-250, correcting RP-192): an op leaves its ring when the engine acknowledges its answer, so
     * a ring holds only the ops whose answers the engine has not saved yet, whatever the window between two
     * saves. A ring that reaches this many drops its oldest op, counted ({@link #evictions}) and logged once
     * per train: that op can be applied twice only if the engine decides it again, so only past this many
     * unacknowledged ops on one train. At most 64 KiB of op ids (and as many answer sequences) a train.
     */
    public static final int CAPACITY = 4096;
    /** The op the probe writes and reads back: an engine op id no engine hands out (top bit set, F4). */
    private static final long PROBE_OP = 0xFFFF_FFFF_FFFF_FFF1L;

    /** Null until the first {@link #available} after start or a test switch; then the answer. */
    private static Boolean available;
    private static BreakForTest broken = BreakForTest.NONE;
    private static boolean probing;
    private static int fallbackLogs;
    private static String lastFallback;

    // ---- the session's view and the deferred prunes ----------------------------------------------

    /** The session generation {@link #view} was taken for (0: none yet). */
    private static long viewGen;
    private static RingView view;
    private static RingView viewOverride;
    /** An acknowledged answer: the op leaves its ring at the next {@link #tick} if its answer is still {@code at}. */
    private record Acked(AppliedOpRing ring, long op, long at, long gen) {}
    private static final ArrayDeque<Acked> ACKED = new ArrayDeque<>();
    private static boolean holdPrunes;
    /** Trains whose eviction was logged this session. */
    private static final Set<UUID> WARNED = new HashSet<>();
    private static long evictions, pruned, settles, settledAway, settledUnknown;

    /**
     * True when the ring mixin applied: {@code Train} implements {@link AppliedOpsHolder} and a probe train's
     * ring survives a {@code Train.write} then {@code Train.read}. Probed once, on the first call.
     */
    public static boolean available(MinecraftServer server) {
        if (available == null) {
            String why = probe(server.registryAccess());
            available = why == null;
            if (why != null) {
                fallbackLogs++;
                lastFallback = why;
                MicRtsim.LOGGER.warn("[mic_rtsim_create] the train op ring is off ({}): IND_CARRIAGE_DELTA's dedup is the "
                        + "journal's op index, which repeats a train op after a crash that kept the goods file and not the journal's "
                        + "(docs/choices-mowzie-build.md RP-193)", why);
            } else {
                MicRtsim.LOGGER.info("[mic_rtsim_create] the train op ring is on: IND_CARRIAGE_DELTA's dedup is each train's {}", KEY);
            }
        }
        return available;
    }

    /** The ring of a train, made on first use; null when the mixin did not apply. */
    public static AppliedOpRing of(Train train) {
        if (broken == BreakForTest.HOLDER)
            return null;
        return (Object) train instanceof AppliedOpsHolder h ? h.mic_rtsim$appliedOps() : null;
    }

    /**
     * {@code Train.write}'s hook: the ring under {@link #KEY}, whenever it holds an op, whatever the industry
     * module's switch says (RP-257). An empty ring writes no key, so a ring the acknowledgements emptied leaves
     * the save as Create's own; with the plugin removed the mixin is gone and Create's {@code Train.write}
     * drops the key (UR-1).
     */
    public static void write(Object train, CompoundTag tag) {
        if (!(train instanceof AppliedOpsHolder h) || tag == null || broken == BreakForTest.WRITE)
            return;
        AppliedOpRing ring = h.mic_rtsim$appliedOpsIfAny();
        if (ring == null || ring.isEmpty())
            return;
        ring.writeTo(tag, KEY);
    }

    /**
     * {@code Train.read}'s hook: the saved ring onto the train Create made. Read during this session (a train
     * Create loads late, or a GameTest's restart), a ring this session answered keeps its answers and waits for
     * their acknowledgements again; any other is settled ({@link #settle}). Read before the session (the
     * world's load), the session settles it when it starts.
     */
    public static void read(Object train, CompoundTag tag) {
        if (!(train instanceof AppliedOpsHolder h) || tag == null || broken == BreakForTest.READ || !AppliedOpRing.savedIn(tag, KEY))
            return;
        AppliedOpRing ring = h.mic_rtsim$appliedOps();
        ring.readFrom(tag, KEY);
        if (probing)
            return;
        RtsimSession s = RtsimSession.current();
        if (s == null)
            return;
        long gen = s.boundary().gen();
        if (ring.gen() == gen)
            ring.forEachAnswered((op, at) -> whenAcked(s, ring, op, at, gen));
        else
            settle(ring);
    }

    // ---- the answers, the acknowledgements and the settle (RP-250, RP-251) ------------------------

    /**
     * Settles a ring another session (or none) answered last, once per session: an op whose answer the
     * engine's pages provably hold leaves, every other op stays with its answer unknown ({@link RingView}).
     * Nothing without a session or for a ring this session already settled.
     */
    public static void settle(AppliedOpRing ring) {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            return;
        long gen = s.boundary().gen();
        if (ring.gen() == gen)
            return;
        if (!ring.isEmpty()) {
            long bound = view(s).bound(ring.gen());
            int away = ring.settle(bound);
            settles++;
            settledAway += away;
            settledUnknown += ring.size();
        }
        ring.gen(gen);
    }

    /** {@code carriageDelta} applied {@code op} to {@code train}: it joins the ring; a full ring drops its oldest. */
    static void applied(Train train, AppliedOpRing ring, long op) {
        long before = ring.evictions();
        ring.add(op);
        if (ring.evictions() == before)
            return;
        evictions++;
        if (WARNED.add(train.id))
            MicRtsim.LOGGER.warn("[mic_rtsim_create] train {} ({}) has a full op ring: {} ops whose answers the engine has not "
                    + "acknowledged, so op {} leaves it, and if the engine decides that op again it applies twice "
                    + "(docs/choices-mowzie-build.md RP-250). Further evictions on this train are only counted ({} on all trains so far, "
                    + "/rtsim district perf)",
                    train.name == null ? "?" : train.name.getString(), train.id, ring.capacity(), Long.toHexString(ring.lastEvicted()), evictions);
    }

    /** {@code carriageDelta} answered {@code op} at {@code seq}: the op leaves the ring once the engine acknowledges {@code seq}. */
    static void answered(AppliedOpRing ring, long op, long seq) {
        RtsimSession s = RtsimSession.current();
        if (s == null || seq <= 0 || !ring.answered(op, seq))
            return;
        whenAcked(s, ring, op, seq, s.boundary().gen());
    }

    private static void whenAcked(RtsimSession s, AppliedOpRing ring, long op, long at, long gen) {
        s.afterAck(at, () -> ACKED.add(new Acked(ring, op, at, gen)));
    }

    /**
     * The session hook's end of tick, after the industry's (and so after its announce): the acknowledged
     * answers' ops leave their rings (an op answered again since, or forgotten by {@link #announce}, stays),
     * and Create's tracks are marked for the next save. Deferred to here from the acknowledgement, which comes
     * earlier in the tick, so that an announce in the same tick forgets the answers first. It runs whatever
     * the industry module's switch says (RP-257): the industry's own tick does not run while it is off.
     */
    static void tick(RtsimSession session) {
        RtsimSession s = RtsimSession.current();
        if (s == null || s != session || holdPrunes || ACKED.isEmpty())
            return;
        long gen = s.boundary().gen();
        boolean changed = false;
        for (Acked a; (a = ACKED.poll()) != null; )
            if (a.gen == gen && a.ring.gen() == gen && a.ring.pruneIfAnsweredAt(a.op, a.at)) {
                pruned++;
                changed = true;
            }
        if (changed)
            Create.RAILWAYS.markTracksDirty();
    }

    /**
     * The industry comes back on (or the engine lost observations): what the engine was told while the module
     * was off is not known to have reached its pages as an answer, so every op's answer becomes unknown.
     */
    static void announce() {
        RtsimSession s = RtsimSession.current();
        long gen = s == null ? 0 : s.boundary().gen();
        TrainWatch.eachTrain(t -> {
            AppliedOpRing ring = peekForTest(t);
            if (ring != null && ring.gen() == gen)
                ring.forgetAnswers();
        });
    }

    /** The session started: take its view and settle every train Create loaded with the world. */
    static void sessionStarted(RtsimSession s) {
        ACKED.clear();
        WARNED.clear();
        view(s);
        boolean[] any = {false};
        TrainWatch.eachTrain(t -> {
            AppliedOpRing ring = peekForTest(t);
            if (ring != null && !ring.isEmpty()) {
                settle(ring);
                any[0] = true;
            }
        });
        if (any[0])
            Create.RAILWAYS.markTracksDirty();
    }

    /**
     * This session's view of the engine's pages and the journal file it loaded, taken once per session at its
     * start (or at the first ring access if that came first). {@code written} is the loaded file's end only
     * while no write of this session has landed; once one has, {@code written >= firstSeq} and the view proves
     * nothing (every op stays, unknown).
     */
    private static RingView view(RtsimSession s) {
        BoundarySession b = s.boundary();
        if (viewGen == b.gen() && view != null)
            return viewOverride != null ? viewOverride : view;
        long firstSeq = b.replayUpTo() + 1;
        Journal j = b.journal();
        List<long[]> gens = new ArrayList<>();
        if (j.pinnedGen() != null)
            gens.add(new long[] {j.pinnedGen().seq(), genOf(j.pinnedGen().bytes())});
        for (Journal.Entry e : j.entries())
            if (e.seq() < firstSeq && kindOf(e.bytes()) == Obs.JOURNAL_GEN)
                gens.add(new long[] {e.seq(), genOf(e.bytes())});
        long[] seqs = new long[gens.size()], nonces = new long[gens.size()];
        for (int i = 0; i < gens.size(); i++) {
            seqs[i] = gens.get(i)[0];
            nonces[i] = gens.get(i)[1];
        }
        view = new RingView(b.gen(), RingView.writerGen(seqs, nonces, firstSeq), b.start().pagesObsSeq(), s.journal().writtenThrough(), firstSeq);
        viewGen = b.gen();
        String why = view.unproven();
        MicRtsim.LOGGER.info("[mic_rtsim_create] train rings settle against {} (RP-251)", why == null
                ? "the engine's pages through " + view.pages() + ", written by generation " + Long.toHexString(view.writerGen())
                : "nothing: " + why + "; every op another session answered stays, its answer unknown");
        return viewOverride != null ? viewOverride : view;
    }

    private static int kindOf(byte[] record) {
        return Short.toUnsignedInt(ByteBuffer.wrap(record).order(Layout.ORDER).getShort(Layout.R_KIND));
    }

    private static long genOf(byte[] record) {
        return ByteBuffer.wrap(record).order(Layout.ORDER).getLong(Obs.JournalGen.GEN);
    }

    /** The session hook: {@link #sessionStarted} and the prunes ({@link #tick}). Registered by {@link MicRtsimCreate}. */
    static final class SessionHook implements SessionPlugin {
        @Override
        public String name() {
            return "mic_rtsim_create train rings";
        }

        @Override
        public void sessionStarted(RtsimSession session) {
            TrainRing.sessionStarted(session);
        }

        @Override
        public void endOfTick(RtsimSession session, long now) {
            TrainRing.tick(session);
        }
    }

    // ---- counters ---------------------------------------------------------------------------------

    /** Ops pushed out of full rings this server run (each a possible double, RP-250). */
    public static long evictions() {
        return evictions;
    }

    /** Ops that left their rings on the engine's acknowledgement this server run. */
    public static long pruned() {
        return pruned;
    }

    /** Ops held now whose answers are unknown, on every train Create or the hooks know. */
    public static long unknownCount() {
        long[] n = {0};
        TrainWatch.eachTrain(t -> {
            AppliedOpRing ring = peekForTest(t);
            if (ring != null)
                n[0] += ring.unknownCount();
        });
        return n[0];
    }

    /** For {@code /rtsim district perf}. */
    static String perfLine() {
        long[] held = {0, 0, 0};
        TrainWatch.eachTrain(t -> {
            AppliedOpRing ring = peekForTest(t);
            if (ring != null && !ring.isEmpty()) {
                held[0]++;
                held[1] += ring.size();
                held[2] += ring.unknownCount();
            }
        });
        String why = view == null ? "no view yet" : view.unproven();
        return "train rings: " + held[1] + " ops on " + held[0] + " trains (" + held[2] + " answers unknown), " + pruned + " acknowledged away, "
                + ACKED.size() + " acknowledgements pending; " + settles + " settled (" + settledAway + " away, " + settledUnknown + " kept unknown, "
                + (why == null ? "against pages " + view.pages() : why) + "); " + evictions + " evicted on " + WARNED.size() + " trains (valve "
                + CAPACITY + ")";
    }

    /** Null when the round trip works, else what broke. */
    private static String probe(HolderLookup.Provider registries) {
        try {
            Train train = new Train(UUID.randomUUID(), null, null, new ArrayList<>(), new ArrayList<>(), false, 0);
            AppliedOpRing ring = of(train);
            if (ring == null)
                return "Train does not implement AppliedOpsHolder: the mixin config did not apply";
            ring.add(PROBE_OP);
            CompoundTag tag;
            Train back;
            probing = true;
            try {
                tag = train.write(new DimensionPalette(), registries);
                long[] saved = tag.getLongArray(KEY);
                if (saved.length != 1 || saved[0] != PROBE_OP)
                    return "Train.write wrote no " + KEY + ": its injection did not apply";
                back = Train.read(tag, registries, Map.of(), new DimensionPalette());
            } finally {
                probing = false;
            }
            AppliedOpRing read = of(back);
            if (read == null || !read.contains(PROBE_OP))
                return "Train.read did not read " + KEY + ": its injection did not apply";
            return null;
        } catch (RuntimeException | LinkageError e) {
            return "the probe threw " + e;
        }
    }

    // ---- GameTests ----------------------------------------------------------------------------

    /**
     * What a GameTest breaks so that the real probe fails as on a Create the mixin missed in part (review2
     * P3-3): {@link #HOLDER}, as if the class mixin had not applied ({@link #of} answers null); {@link #WRITE}
     * or {@link #READ}, as if that injection had not ({@link #write} or {@link #read} does nothing).
     */
    public enum BreakForTest { NONE, HOLDER, WRITE, READ }

    /**
     * Breaks one of the ring's hooks ({@link BreakForTest}), or none. A change probes again at the next
     * {@link #available}: the probe runs as at start, fails through its own branch and logs its WARN.
     */
    public static void breakForTest(BreakForTest what) {
        if (broken == what)
            return;
        broken = what;
        available = null;
    }

    /**
     * Holds the acknowledged answers' prunes (they wait, and run once released): a GameTest that sends an op
     * again after the engine acknowledged its answer, which a real engine never does, keeps the op in the ring.
     */
    public static void holdPrunesForTest(boolean hold) {
        holdPrunes = hold;
    }

    /** Settles against this view instead of the session's ({@code null}: the session's again). */
    public static void viewForTest(RingView v) {
        viewOverride = v;
    }

    /** The session's own view (taken if not yet). */
    public static RingView sessionViewForTest() {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            return null;
        RingView keep = viewOverride;
        viewOverride = null;
        try {
            return view(s);
        } finally {
            viewOverride = keep;
        }
    }

    /** Trains whose eviction this session logged. */
    public static int warnedForTest() {
        return WARNED.size();
    }

    /** {@link #applied} as {@code carriageDelta} calls it (the valve's GameTest fills a ring without the engine). */
    public static void appliedForTest(Train train, AppliedOpRing ring, long op) {
        applied(train, ring, op);
    }

    /** A train's ring without making one: null when it never had one or the mixin did not apply. */
    public static AppliedOpRing peekForTest(Train train) {
        return (Object) train instanceof AppliedOpsHolder h ? h.mic_rtsim$appliedOpsIfAny() : null;
    }

    /** How many times the fallback was logged: once per probe that failed. */
    public static int fallbackLogsForTest() {
        return fallbackLogs;
    }

    /** What the last failed probe found broken, as its WARN said; null before any. */
    public static String lastFallbackForTest() {
        return lastFallback;
    }
}
