package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.content.logistics.packagePort.frogport.FrogportBlockEntity;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

import java.util.function.Predicate;

/**
 * Count points for a frogport next to an inventory (SECTIONS.md §4.2, slice S3; PATCHES.md rows 72 and 73).
 * Create's frogport moves between its own package buffer and the inventory below it: it pushes the packages
 * it received into that inventory ({@code tryPushingToAdjacentInventories}: one package per slot, the real
 * insert after a simulated extract from its own handler) and pulls a package out of it to send
 * ({@code tryPullingFrom}, called first for the port's own buffer and then for the inventory below). The
 * inventory calls are the counted moves; the port's own buffer is not an inventory of a district and its
 * extract and its arrival insert are skipped. A package counts as its contents (core expands it), as the
 * sampler reads a package in a store.
 */
@Mixin(value = FrogportBlockEntity.class, remap = false)
public abstract class FrogportBlockEntityMixin {
    @WrapOperation(method = "tryPushingToAdjacentInventories", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItemStacked(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countPush(IItemHandler handler, ItemStack stack, boolean simulate, Operation<ItemStack> original) {
        Counting c = Counting.active();
        ItemStack seen = c != null && !simulate ? stack.copy() : null; // the handler may change its argument
        int before = stack.getCount();
        ItemStack remainder = original.call(handler, stack, simulate);
        if (seen != null) {
            FrogportBlockEntity self = (FrogportBlockEntity) (Object) this;
            if (c.isMover(self.getLevel(), self.getBlockPos()))
                c.moved(self.getLevel(), self.getBlockPos(), self.getBlockPos().below(), seen, before - remainder.getCount(), true);
        }
        return remainder;
    }

    @WrapOperation(method = "tryPullingFrom", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/foundation/item/ItemHelper;extract(Lnet/neoforged/neoforge/items/IItemHandler;Ljava/util/function/Predicate;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countPull(IItemHandler handler, Predicate<ItemStack> filter, boolean simulate, Operation<ItemStack> original) {
        ItemStack taken = original.call(handler, filter, simulate);
        Counting c = Counting.active();
        if (c != null && !simulate && !taken.isEmpty()) {
            FrogportBlockEntity self = (FrogportBlockEntity) (Object) this;
            // The port's own buffer is tried first and is not the inventory below it.
            if (handler != self.inventory && c.isMover(self.getLevel(), self.getBlockPos()))
                c.moved(self.getLevel(), self.getBlockPos(), self.getBlockPos().below(), taken, taken.getCount(), false);
        }
        return taken;
    }
}
