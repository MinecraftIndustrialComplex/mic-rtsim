package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators;

import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRing;

/**
 * Diesel's {@code OilChunksSavedData} with the deposit ops applied to it, kept in {@code cdg_oil_chunks} beside the
 * amounts (mixin {@code OilChunksSavedDataRingMixin}, PATCHES.md row 88; docs/choices-journal-barrier.md RP-204).
 */
public interface OilRingHolder {
    /** The ring of this dimension's oil file; empty until an op is applied. */
    DurableOpRing mic_rtsim$ring();

    /** The ring read with the file (its load). */
    void mic_rtsim$setRing(DurableOpRing ring);
}
