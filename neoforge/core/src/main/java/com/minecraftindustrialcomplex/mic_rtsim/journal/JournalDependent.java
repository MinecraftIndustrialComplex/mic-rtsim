package com.minecraftindustrialcomplex.mic_rtsim.journal;

/**
 * A SavedData whose file holds the effect of an op the journal's file records, or the dedup of one (a ring, a
 * reconcile epoch, a district, a house order; docs/replay-gate.md 9.6): the journal's file must never reach disk ahead
 * of it. A failed write of such a file holds the journal's write ({@link SavedDataWrites}, docs/choices-journal-barrier.md
 * U-g1).
 *
 * <p>Our own SavedData implements it; another mod's gets it from a mixin in the plugin that owns the dependency (D4):
 * Create's {@code create_tracks}, Diesel's {@code cdg_oil_chunks}, NeoForge's level attachments (Destroy's pollution).
 * The marker adds nothing to a save (UR-1).
 */
public interface JournalDependent {
    /**
     * Marks the data dirty again ({@code SavedData.setDirty()}): vanilla cleared its flag before the write that
     * failed, so without this the next save would not write the file at all. Server thread.
     */
    void mic_rtsim$remark();
}
