package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.impl.unpacking.DefaultUnpackingHandler;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Count point for a packager unpacking a box into the inventory in front of it (SECTIONS.md §4.2, slice S3;
 * PATCHES.md row 75). {@code PackagerBlockEntity.unwrapBox} asks the registered {@code UnpackingHandler} of the
 * target block, or the default one, to unpack the box's contents into the block at {@code pos}, the packager
 * being at {@code pos.relative(side)}. The default handler's real branch inserts each stack with
 * {@code insertItemStacked}: that insert is the move, counted once. (Its simulated branch tests with
 * {@code insertItem(slot, stack, true)}, which is not wrapped.) The special handlers, for a basin, a
 * mechanical crafter and a block that voids, do not touch a district's store and are not counted; a mod's own
 * handler is not counted either (documented in PATCHES.md).
 */
@Mixin(value = DefaultUnpackingHandler.class, remap = false)
public abstract class DefaultUnpackingHandlerMixin {
    @WrapOperation(method = "unpack", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItemStacked(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countUnpack(IItemHandler inventory, ItemStack stack, boolean simulate, Operation<ItemStack> original,
            @Local(argsOnly = true) Level level, @Local(argsOnly = true) BlockPos pos, @Local(argsOnly = true) Direction side) {
        Counting c = Counting.active();
        ItemStack seen = null; // copied only for a counted mover (the handler may change its argument)
        if (c != null && !simulate) {
            try {
                if (c.isMover(level, pos.relative(side)))
                    seen = stack.copy();
            } catch (RuntimeException e) {
                c.failed("an unpacking", e); // never break the packager's tick
            }
        }
        int before = stack.getCount();
        ItemStack remainder = original.call(inventory, stack, simulate);
        if (seen != null) {
            try {
                c.moved(level, pos.relative(side), pos, seen, before - remainder.getCount(), true);
            } catch (RuntimeException e) {
                c.failed("an unpacking", e);
            }
        }
        return remainder;
    }
}
