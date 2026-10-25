package com.minecraftindustrialcomplex.mic_rtsim.journal;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.BoundarySession;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.Tag;
import net.neoforged.neoforge.attachment.IAttachmentHolder;
import net.neoforged.neoforge.attachment.IAttachmentSerializer;
import org.jetbrains.annotations.Nullable;

import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;

/**
 * A {@link DurableOpRing} in NBT and in the session: {@code ops} and {@code seqs}, two long arrays, oldest first, and
 * {@code gen}; in a store's own tag under {@link DurableOpRing#KEY}, or as a level attachment ({@link #SERIALIZER}).
 * A store without it reads as an empty ring; a store's own loader that does not know the key ignores it, and its next
 * save drops it (UR-1: uninstalling rtsim leaves nothing that matters). An empty ring writes nothing.
 *
 * <p>T2's rule for every ring (docs/choices-mowzie-build.md RP-250, RP-251; docs/choices-journal-barrier.md RP-209):
 * an answer the industry gives an op of a ring ({@link #answered}) waits for the engine's acknowledgement of its
 * sequence, and the op leaves at the end of that tick ({@link #tick}) if its answer is still that one. A ring another
 * session answered is settled once per session ({@link #settle}) against this session's {@link #view} of the engine's
 * pages, taken at its start: at the start for the rings loaded with the world, at the load for one loaded later. The
 * industry's announce (the module back on, or the engine lost observations) makes every answer this session gave
 * unknown ({@link #announce}).
 *
 * <p>Saving and loading never depend on a module switch (docs/replay-gate.md 9.6, "Rings and the module switch"): a
 * save writes the ring whenever it holds an op, a load keeps what it finds, and the prunes and the settle run from the
 * session's hook ({@link SessionHook}), with the industry module on or off.
 */
public final class DurableOpRings {
    private DurableOpRings() {}

    /** Writes {@code ring} into a store's tag at the store's save (server thread). */
    public static void write(DurableOpRing ring, CompoundTag storeTag) {
        CompoundTag t = toTag(ring);
        if (t != null)
            storeTag.put(DurableOpRing.KEY, t);
    }

    /** The ring a store's tag holds, read from disk; empty when it has none. */
    public static DurableOpRing read(CompoundTag storeTag) {
        return storeTag.contains(DurableOpRing.KEY, Tag.TAG_COMPOUND) ? fromTag(storeTag.getCompound(DurableOpRing.KEY)) : new DurableOpRing();
    }

    /**
     * The ring's own tag at its store's save; null when it is empty. Written whatever the industry module's switch
     * (replay-gate 9.6): a save that dropped it after a switch off could die before the journal's write, and the op
     * applied before the switch would be applied again.
     */
    public static @Nullable CompoundTag toTag(DurableOpRing ring) {
        if (ring.size() == 0)
            return null;
        CompoundTag t = new CompoundTag();
        t.putLongArray("ops", ring.ops());
        t.putLongArray("seqs", ring.seqs());
        if (ring.gen() != 0)
            t.putLong("gen", ring.gen());
        return t;
    }

    /** A ring read from disk from its own tag, then {@link #loaded}. */
    public static DurableOpRing fromTag(CompoundTag t) {
        long[] ops = t.getLongArray("ops"), seqs = t.getLongArray("seqs");
        DurableOpRing ring;
        if (ops.length != seqs.length) {
            // Corrupt: keep every op, its answer unknown. A resend is then refused: an effect left out (for a deposit's
            // draw, ore the world keeps), never one applied twice.
            MicRtsim.LOGGER.error("[mic_rtsim] a store's applied ops are corrupt ({} ops, {} sequences): each is kept, its answer unknown",
                    ops.length, seqs.length);
            ring = DurableOpRing.allUnknown(ops, DurableOpRing.DEFAULT_CAP);
        } else {
            ring = DurableOpRing.fromDisk(ops, seqs, t.getLong("gen"), DurableOpRing.DEFAULT_CAP);
        }
        loaded(ring);
        return ring;
    }

    /**
     * A ring read during a session: one this session answered keeps its answers and waits for their acknowledgements
     * again; any other is settled now. Read before the session (the world's load), the session settles it when it
     * starts.
     */
    static void loaded(DurableOpRing ring) {
        RtsimSession s = RtsimSession.current();
        if (s == null || ring.size() == 0)
            return;
        long gen = s.boundary().gen();
        if (ring.gen() == gen)
            ring.forEachAnswered((op, at) -> whenAcked(s, ring, op, at, gen));
        else
            settle(ring);
    }

    /**
     * A ring as a level attachment ({@code mic_rtsim:applied_ops}, {@code RtsimAttachments.LEVEL_APPLIED_OPS}): NeoForge
     * saves a level's attachments together in that level's {@code neoforge_data_attachments}, so the ring is in the
     * same file as another mod's level attachment it guards (Destroy's level pollution). NeoForge always saves that
     * file, so the ring needs no dirty flag. A world without rtsim logs "unknown or non-serializable data attachment
     * mic_rtsim:applied_ops. Skipping." at the level's load and drops it at the next save (UR-1).
     */
    public static final IAttachmentSerializer<CompoundTag, DurableOpRing> SERIALIZER = new IAttachmentSerializer<>() {
        @Override
        public DurableOpRing read(IAttachmentHolder holder, CompoundTag tag, HolderLookup.Provider registries) {
            return fromTag(tag);
        }

        @Override
        public @Nullable CompoundTag write(DurableOpRing ring, HolderLookup.Provider registries) {
            return toTag(ring);
        }
    };

    // ---- the answers, the acknowledgements and the settle (T2's rule) -----------------------------

    /** An acknowledged answer: the op leaves its ring at the next {@link #tick} if its answer is still {@code at}. */
    private record Acked(DurableOpRing ring, long op, long at, long gen) {}

    private static final ArrayDeque<Acked> ACKED = new ArrayDeque<>();
    private static boolean holdPrunes;
    private static long pruned, settles, settledAway, settledUnknown;

    /**
     * {@code op} (or one part's key, {@link DurableOpRing#keyOf}) was answered at {@code seq}: it joins or stays in
     * the ring with that answer, and leaves once the engine acknowledges {@code seq}. A ring not yet settled by this
     * session is settled first, so its generation is this session's. Without a session the op is kept, its answer
     * unknown.
     */
    public static void answered(DurableOpRing ring, long op, long seq) {
        RtsimSession s = RtsimSession.current();
        if (s == null) {
            ring.add(op, DurableOpRing.UNKNOWN);
            return;
        }
        settle(ring);
        ring.add(op, seq);
        if (seq > 0)
            whenAcked(s, ring, op, seq, s.boundary().gen());
    }

    private static void whenAcked(RtsimSession s, DurableOpRing ring, long op, long at, long gen) {
        s.afterAck(at, () -> ACKED.add(new Acked(ring, op, at, gen)));
    }

    /**
     * Settles a ring another session (or none) answered last, once per session: an op whose answer the engine's pages
     * provably hold leaves, every other op stays with its answer unknown ({@link RingView}). Nothing without a session
     * or for a ring this session already settled.
     */
    public static void settle(DurableOpRing ring) {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            return;
        long gen = s.boundary().gen();
        if (ring.gen() == gen)
            return;
        if (ring.size() > 0) {
            long bound = view(s).bound(ring.gen());
            int away = ring.settle(bound);
            settles++;
            settledAway += away;
            settledUnknown += ring.size();
        }
        ring.gen(gen);
        ring.changed();
    }

    /**
     * The session hook's end of tick, after the industry's (and so after its announce): the acknowledged answers' ops
     * leave their rings (an op answered again since, or made unknown by {@link #announce}, stays). Deferred to here
     * from the acknowledgement, which comes earlier in the tick, so that an announce in the same tick forgets the
     * answers first. It runs whatever the industry module's switch says.
     */
    static void tick(RtsimSession session) {
        RtsimSession s = RtsimSession.current();
        if (s == null || s != session || holdPrunes || ACKED.isEmpty())
            return;
        long gen = s.boundary().gen();
        for (Acked a; (a = ACKED.poll()) != null; )
            if (a.gen == gen && a.ring.gen() == gen && a.ring.pruneIfAnsweredAt(a.op, a.at))
                pruned++;
    }

    /**
     * The industry comes back on (or the engine lost observations): what the engine was told while the module was
     * off is not known to have reached its pages as an answer, so every answer this session gave becomes unknown.
     */
    public static void announce() {
        RtsimSession s = RtsimSession.current();
        long gen = s == null ? 0 : s.boundary().gen();
        DurableOpRing.forEachLive(r -> {
            if (r.gen() == gen && r.forgetAnswers() > 0)
                r.changed();
        });
    }

    /** The session started: take its view and settle every ring loaded with the world. */
    static void sessionStarted(RtsimSession s) {
        ACKED.clear();
        view(s);
        DurableOpRing.forEachLive(r -> {
            if (r.size() > 0)
                settle(r);
        });
    }

    // ---- the session's view of the engine's pages (RP-251) ----------------------------------------

    /** The session generation {@link #view} was taken for (0: none yet). */
    private static long viewGen;
    private static RingView view;
    private static RingView viewOverride;

    /**
     * This session's view of the engine's pages and the journal file it loaded, taken once per session at its start
     * (or at the first ring access if that came first), for every ring: the stores', and the train's and the station
     * port's in create_tracks.dat. {@code written} is the loaded file's end only while no write of this session has
     * landed; once one has, {@code written >= firstSeq} and the view proves nothing (every op stays, unknown).
     */
    public static RingView view(RtsimSession s) {
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
        MicRtsim.LOGGER.info("[mic_rtsim] op rings settle against {} (RP-251)", why == null
                ? "the engine's pages through " + view.pages() + ", written by generation " + Long.toHexString(view.writerGen())
                : "nothing: " + why + "; every op another session answered stays, its answer unknown");
        return viewOverride != null ? viewOverride : view;
    }

    /** The last view taken, or null before any (for perf lines). */
    public static @Nullable RingView lastView() {
        return view;
    }

    private static int kindOf(byte[] record) {
        return Short.toUnsignedInt(ByteBuffer.wrap(record).order(Layout.ORDER).getShort(Layout.R_KIND));
    }

    private static long genOf(byte[] record) {
        return ByteBuffer.wrap(record).order(Layout.ORDER).getLong(Obs.JournalGen.GEN);
    }

    /** The session hook: {@link #sessionStarted} and the prunes ({@link #tick}). Registered by {@code MicRtsim}. */
    public static final class SessionHook implements SessionPlugin {
        @Override
        public String name() {
            return "mic_rtsim store op rings";
        }

        @Override
        public void sessionStarted(RtsimSession session) {
            DurableOpRings.sessionStarted(session);
        }

        @Override
        public void endOfTick(RtsimSession session, long now) {
            DurableOpRings.tick(session);
        }
    }

    /** For {@code /rtsim perf}. */
    public static String perfLine() {
        long[] held = {0, 0, 0};
        DurableOpRing.forEachLive(r -> {
            if (r.size() > 0) {
                held[0]++;
                held[1] += r.size();
                held[2] += r.unknownCount();
            }
        });
        String why = view == null ? "no view yet" : view.unproven();
        return "store op rings: " + held[1] + " ops in " + held[0] + " rings (" + held[2] + " answers unknown), " + pruned + " acknowledged away, "
                + ACKED.size() + " acknowledgements pending; " + settles + " settled (" + settledAway + " away, " + settledUnknown
                + " kept unknown, " + (why == null ? "against pages " + view.pages() : why) + "); " + DurableOpRing.overflowedTotal()
                + " overflowed (valve " + DurableOpRing.DEFAULT_CAP + ")";
    }

    // ---- GameTests ----------------------------------------------------------------------------

    /**
     * Holds the acknowledged answers' prunes (they wait, and run once released): a GameTest that sends an op again
     * after the engine acknowledged its answer, which a real engine never does, keeps the op in the ring.
     */
    public static void holdPrunesForTest(boolean hold) {
        holdPrunes = hold;
    }

    /** Settles against this view instead of the session's ({@code null}: the session's again). */
    public static void viewForTest(@Nullable RingView v) {
        viewOverride = v;
    }

    /** The session's own view (taken if not yet). */
    public static @Nullable RingView sessionViewForTest() {
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

    /** Ops that left their rings on the engine's acknowledgement this server run. */
    public static long pruned() {
        return pruned;
    }
}
