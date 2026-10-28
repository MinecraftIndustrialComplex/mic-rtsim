package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.journal.JournalDependent;
import com.minecraftindustrialcomplex.mic_rtsim.journal.SavedDataWrites;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.level.saveddata.SavedData;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

import java.nio.file.Path;

/**
 * Sees a failed SavedData write (PATCHES.md row 95, docs/choices-journal-barrier.md U-g1). NeoForge 21.1.249's
 * {@code SavedData.save(File, HolderLookup.Provider)} builds the tag on the server thread, queues
 * {@code lambda$save$0(CompoundTag, File)} on its IO chain and clears the dirty flag; the lambda calls
 * {@code IOUtilities.writeNbtCompressed} inside a {@code try} that logs an {@code IOException} and goes on, so nothing
 * else knows the file was not written (javap of the 21.1.249 patched class; the same patch as 21.1.230's).
 *
 * <p>The wrap runs that one call and, for a {@link JournalDependent} data only, records the outcome in
 * {@link SavedDataWrites}: a throw is recorded and rethrown unchanged (vanilla logs it as before), a success ends the
 * file's streak. Any other SavedData, and any other caller of {@code writeNbtCompressed}, is untouched: one
 * {@code instanceof} per write.
 */
@Mixin(SavedData.class)
public abstract class SavedDataWriteMixin {
    @WrapOperation(method = "lambda$save$0(Lnet/minecraft/nbt/CompoundTag;Ljava/io/File;)V", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/common/IOUtilities;writeNbtCompressed(Lnet/minecraft/nbt/CompoundTag;Ljava/nio/file/Path;)V"))
    private void mic_rtsim$watchWrite(CompoundTag tag, Path path, Operation<Void> original) {
        Object self = this;
        if (!(self instanceof JournalDependent data)) {
            original.call(tag, path);
            return;
        }
        try {
            original.call(tag, path); // throws the IOException it declares, which the Operation cannot name
        } catch (Throwable t) {
            SavedDataWrites.INSTANCE.failed(data, path, t);
            throw t;
        }
        SavedDataWrites.INSTANCE.written(path);
    }
}
