package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Puppets are never saved by vanilla (PLAN.md §6.1 item 3).
 *
 * <ul>
 *   <li>{@code shouldBeSaved()}: the filter {@code PersistentEntitySectionManager.storeChunkSections}
 *       applies before it hands a chunk's entities to {@code EntityStorage.storeEntities}.</li>
 *   <li>{@code saveAsPassenger(CompoundTag)}: a vehicle saves its passengers from
 *       {@code saveWithoutId}, calling each one's {@code saveAsPassenger} without asking
 *       {@code shouldBeSaved} (checked in the 1.21.1 source). {@code Entity.save} also goes through
 *       it, so this one covers a puppet riding a boat, a puppet as the root of a stack, and every
 *       other caller of {@code save}.</li>
 * </ul>
 *
 * {@code saveWithoutId} itself is left alone: a dimension change copies an entity through it
 * ({@code Entity.restoreFrom}), and that copy must keep its tag so the join check recognizes it.
 * Neither method is overridden by a vanilla subclass of a body the core drives (only {@code Player}
 * and {@code EnderDragonPart} override {@code shouldBeSaved}).
 */
@Mixin(Entity.class)
public abstract class EntitySaveMixin {

    @Inject(method = "shouldBeSaved", at = @At("HEAD"), cancellable = true)
    private void mic_rtsim$neverSavePuppet(CallbackInfoReturnable<Boolean> cir) {
        if (RtsimAttachments.isTagged((Entity) (Object) this))
            cir.setReturnValue(false);
    }

    @Inject(method = "saveAsPassenger", at = @At("HEAD"), cancellable = true)
    private void mic_rtsim$neverSavePuppetPassenger(CompoundTag compound, CallbackInfoReturnable<Boolean> cir) {
        if (RtsimAttachments.isTagged((Entity) (Object) this))
            cir.setReturnValue(false);
    }
}
