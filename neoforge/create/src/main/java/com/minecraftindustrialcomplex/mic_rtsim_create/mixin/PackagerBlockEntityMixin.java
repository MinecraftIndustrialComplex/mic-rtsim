package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.content.logistics.packager.PackagerBlockEntity;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Count point for a packager drawing items out of the inventory behind it (SECTIONS.md §4.2, slice S3;
 * PATCHES.md row 74). {@code attemptToSend} (a button, a redstone pulse or a logistics request) extracts a
 * slot's stack from the target inventory, simulated to test and then for real, into a box it is packing. The
 * real extraction is the move. The other direction, a package unpacked into the inventory, is
 * {@code DefaultUnpackingHandler} (its own mixin). The packager's own item handler (what a chute above it
 * inserts a box into) is not an inventory of a district. A repackager overrides these calls and is hooked in
 * {@link RepackagerBlockEntityMixin} (PATCHES.md row 76).
 */
@Mixin(value = PackagerBlockEntity.class, remap = false)
public abstract class PackagerBlockEntityMixin {
    @WrapOperation(method = "attemptToSend", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/IItemHandler;extractItem(IIZ)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countExtract(IItemHandler inventory, int slot, int amount, boolean simulate, Operation<ItemStack> original) {
        ItemStack taken = original.call(inventory, slot, amount, simulate);
        Counting c = Counting.active();
        if (c != null && !simulate && !taken.isEmpty()) {
            try {
                PackagerBlockEntity self = (PackagerBlockEntity) (Object) this;
                if (c.isMover(self.getLevel(), self.getBlockPos()))
                    c.moved(self.getLevel(), self.getBlockPos(), self.targetInventory.getTarget().getConnectedPos(), taken, taken.getCount(), false);
            } catch (RuntimeException e) {
                c.failed("a packager's extract", e); // never break the machine's tick
            }
        }
        return taken;
    }
}
