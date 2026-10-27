package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalDependent;
import com.simibubi.create.content.trains.RailwaySavedData;
import net.minecraft.world.level.saveddata.SavedData;
import org.spongepowered.asm.mixin.Mixin;

/**
 * Marks Create's {@code create_tracks} data as a file the journal's file counts on (PATCHES.md row 96,
 * docs/choices-journal-barrier.md U-g1): a train's position and the train ops decided on it live there, the dedup of
 * each station exchange next to them, and the journal's file must not reach disk ahead of it. The marker is what the
 * {@code SavedData} write wrap in core looks for; a failed write of this file holds the journal's write, and the next
 * world save marks this data dirty again through {@link #mic_rtsim$remark}. It adds no field and nothing to a save
 * (UR-1): uninstalling leaves {@code create_tracks.dat} as Create writes it.
 */
@Mixin(value = RailwaySavedData.class, remap = false)
public abstract class RailwaySavedDataMixin extends SavedData implements JournalDependent {
    @Override
    public void mic_rtsim$remark() {
        setDirty();
    }
}
