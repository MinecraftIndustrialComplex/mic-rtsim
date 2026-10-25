package com.minecraftindustrialcomplex.mic_rtsim_create;

/**
 * What a session settles the op rings another session answered against (docs/choices-mowzie-build.md RP-251):
 * this session's generation, the generation of the session whose journal file it loaded, the engine's pages
 * at its start, the end of that journal file, and this session's first sequence. Pure, so the unit tests
 * run it without a server; {@link TrainRing} fills it from the session.
 *
 * <p>An op a ring holds with the sequence {@code at} of its answer leaves at the settle only when the engine's
 * pages hold that answer as the answering session sent it, live: the ring was settled last by the session
 * that wrote the loaded journal file ({@code ringGen == writerGen}), and the pages are not ahead of that file
 * ({@code pages <= written}), so no later session made them (a later session's pages include its own first
 * record, past the file's end). Then every sequence up to {@code pages} is the writer's, which answered the op
 * live after its engine decided it, and no later session reused it. Anything else, the op stays and its answer
 * becomes unknown.
 *
 * @param gen       this session's generation
 * @param writerGen the newest JOURNAL_GEN below {@code firstSeq} in the loaded journal (pinned or kept), 0 for none
 * @param pages     the engine's pages at this session's start ({@code SessionStart.pagesObsSeq})
 * @param written   the loaded journal file's last sequence ({@code JournalSavedData.writtenThrough} before this
 *                  session's first write landed)
 * @param firstSeq  this session's first sequence, its JOURNAL_GEN's ({@code replayUpTo + 1})
 */
public record RingView(long gen, long writerGen, long pages, long written, long firstSeq) {
    /** Why this view proves no answer of another session saved, or null when it can. */
    public String unproven() {
        if (writerGen == 0)
            return "the journal names no earlier session (a new journal, or this session's own generation is the newest acknowledged)";
        if (written >= firstSeq)
            return "this session's journal file was written before the view was taken (" + written + " at or past " + firstSeq + ")";
        if (pages > written)
            return "the engine's pages (" + pages + ") are ahead of the loaded journal file (" + written + ", P29): a later session may have made them";
        return null;
    }

    /**
     * The bound a ring settled last by generation {@code ringGen} settles against ({@link AppliedOpRing#settle}):
     * the pages when the view proves them and the ring is the journal writer's, else 0 (every op stays,
     * unknown).
     */
    public long bound(long ringGen) {
        return ringGen != 0 && ringGen == writerGen && unproven() == null ? pages : 0;
    }

    /**
     * The generation of the newest JOURNAL_GEN below {@code firstSeq}: {@code seqs[i]} and {@code gens[i]} are
     * each JOURNAL_GEN the journal holds (its pinned one and its entries), in any order. 0 for none.
     */
    public static long writerGen(long[] seqs, long[] gens, long firstSeq) {
        long best = -1, gen = 0;
        for (int i = 0; i < seqs.length; i++)
            if (seqs[i] < firstSeq && seqs[i] > best) {
                best = seqs[i];
                gen = gens[i];
            }
        return gen;
    }
}
