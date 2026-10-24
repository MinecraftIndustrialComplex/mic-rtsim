package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.content.logistics.chute.ChuteBlockEntity;
import com.simibubi.create.foundation.item.ItemHelper.ExtractionCountMode;
import net.minecraft.core.BlockPos;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

import java.util.function.Predicate;

/**
 * Count points for chutes and smart chutes (SECTIONS.md §4.2, slice S3; PATCHES.md rows 66 to 68). A chute
 * moves items between its one held stack and the inventory above or below it: it draws from the inventory
 * in {@code handleInput} (from above at {@code startLocation} 1, from below at 0), and pushes the held stack
 * into the inventory below in {@code handleDownwardOutput} or above in {@code handleUpwardOutput}. Each is
 * called twice, once to test ({@code simulate} true, or the simulated extraction of a count mode) and once
 * to do it; only the real call is counted, once, at the call that touches the inventory. The smart chute
 * extends this class and overrides none of the three.
 */
@Mixin(value = ChuteBlockEntity.class, remap = false)
public abstract class ChuteBlockEntityMixin {
    @WrapOperation(method = "handleInput", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/foundation/item/ItemHelper;extract(Lnet/neoforged/neoforge/items/IItemHandler;Ljava/util/function/Predicate;Lcom/simibubi/create/foundation/item/ItemHelper$ExtractionCountMode;IZ)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countInput(IItemHandler inv, Predicate<ItemStack> accept, ExtractionCountMode mode, int count, boolean simulate,
            Operation<ItemStack> original, @Local(argsOnly = true) float startLocation) {
        ItemStack taken = original.call(inv, accept, mode, count, simulate);
        Counting c = Counting.active();
        if (c != null && !simulate && !taken.isEmpty()) {
            ChuteBlockEntity self = (ChuteBlockEntity) (Object) this;
            if (c.isMover(self.getLevel(), self.getBlockPos())) {
                BlockPos target = startLocation >= 0.5F ? self.getBlockPos().above() : self.getBlockPos().below();
                c.moved(self.getLevel(), self.getBlockPos(), target, taken, taken.getCount(), false);
            }
        }
        return taken;
    }

    @WrapOperation(method = "handleDownwardOutput", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItemStacked(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countDown(IItemHandler inv, ItemStack stack, boolean simulate, Operation<ItemStack> original) {
        Counting c = Counting.active();
        ItemStack seen = c != null && !simulate ? stack.copy() : null; // the handler may change its argument
        int before = stack.getCount();
        ItemStack remainder = original.call(inv, stack, simulate);
        if (seen != null) {
            ChuteBlockEntity self = (ChuteBlockEntity) (Object) this;
            if (c.isMover(self.getLevel(), self.getBlockPos()))
                c.moved(self.getLevel(), self.getBlockPos(), self.getBlockPos().below(), seen, before - remainder.getCount(), true);
        }
        return remainder;
    }

    @WrapOperation(method = "handleUpwardOutput", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItemStacked(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countUp(IItemHandler inv, ItemStack stack, boolean simulate, Operation<ItemStack> original) {
        Counting c = Counting.active();
        ItemStack seen = c != null && !simulate ? stack.copy() : null; // the handler may change its argument
        int before = stack.getCount();
        ItemStack remainder = original.call(inv, stack, simulate);
        if (seen != null) {
            ChuteBlockEntity self = (ChuteBlockEntity) (Object) this;
            if (c.isMover(self.getLevel(), self.getBlockPos()))
                c.moved(self.getLevel(), self.getBlockPos(), self.getBlockPos().above(), seen, before - remainder.getCount(), true);
        }
        return remainder;
    }
}
