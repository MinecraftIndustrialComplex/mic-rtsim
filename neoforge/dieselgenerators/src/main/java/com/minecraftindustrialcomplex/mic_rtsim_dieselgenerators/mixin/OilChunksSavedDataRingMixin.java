package com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.mixin;

import com.jesz.createdieselgenerators.world.OilChunksSavedData;
import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRing;
import com.minecraftindustrialcomplex.mic_rtsim.journal.DurableOpRings;
import com.minecraftindustrialcomplex.mic_rtsim_dieselgenerators.OilRingHolder;
import net.minecraft.core.HolderLookup;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.saveddata.SavedData;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * The deposit ops applied to a dimension's oil, in its own file (PATCHES.md row 88, docs/choices-journal-barrier.md
 * RP-204): {@code cdg_oil_chunks} gets the ring under {@code mic_rtsim:applied_ops} at its save and reads it back at
 * its load, so the file that holds a draw also holds the op that made it. Diesel's own load reads only its
 * {@code OilChunks} list and its save builds a new tag, so a file left with the key after rtsim is removed loads as
 * before and loses the key at its next save (UR-1; checked against the 1.3.15 bytecode).
 */
@Mixin(value = OilChunksSavedData.class, remap = false)
public abstract class OilChunksSavedDataRingMixin implements OilRingHolder {
    @Unique
    private DurableOpRing mic_rtsim$ring;

    @Override
    public DurableOpRing mic_rtsim$ring() {
        if (mic_rtsim$ring == null)
            mic_rtsim$setRing(new DurableOpRing());
        return mic_rtsim$ring;
    }

    @Override
    public void mic_rtsim$setRing(DurableOpRing ring) {
        SavedData self = (SavedData) (Object) this;
        ring.onChange(self::setDirty);
        mic_rtsim$ring = ring;
    }

    @Inject(method = "save(Lnet/minecraft/nbt/CompoundTag;Lnet/minecraft/core/HolderLookup$Provider;)Lnet/minecraft/nbt/CompoundTag;", at = @At("RETURN"))
    private void mic_rtsim$saveRing(CompoundTag tag, HolderLookup.Provider registries, CallbackInfoReturnable<CompoundTag> cir) {
        if (mic_rtsim$ring != null)
            DurableOpRings.write(mic_rtsim$ring, cir.getReturnValue());
    }

    @Inject(method = "load(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/nbt/CompoundTag;Lnet/minecraft/core/HolderLookup$Provider;)Lcom/jesz/createdieselgenerators/world/OilChunksSavedData;",
            at = @At("RETURN"))
    private static void mic_rtsim$loadRing(ServerLevel level, CompoundTag tag, HolderLookup.Provider registries,
            CallbackInfoReturnable<OilChunksSavedData> cir) {
        ((OilRingHolder) cir.getReturnValue()).mic_rtsim$setRing(DurableOpRings.read(tag));
    }
}
