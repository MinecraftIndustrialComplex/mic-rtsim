package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.content.processing.basin.BasinBlock;
import com.simibubi.create.content.processing.basin.BasinBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Count point for a basin's side output (SECTIONS.md §4.2, slice S3; PATCHES.md row 71). A basin whose spout
 * faces sideways does not keep its recipe outputs: {@code acceptOutputs} puts them in {@code spoutputBuffer},
 * and {@code tryClearingSpoutputOverflow}, which runs while the buffer is not empty, inserts each into the
 * inventory at {@code below().relative(FACING)} (a simulated insert to test, then the real one). The real
 * insert is the move: counted once, there. Outputs that stay in the basin ({@code FACING} down) are not a
 * move between blocks and are in another method; a mixer or press over the basin calls {@code acceptOutputs},
 * which moves nothing into the inventory, so it is not a mover of that inventory (the basin's position is the
 * mover's, whichever model the engine gives the block).
 */
@Mixin(value = BasinBlockEntity.class, remap = false)
public abstract class BasinBlockEntityMixin {
    @WrapOperation(method = "tryClearingSpoutputOverflow", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItemStacked(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countOverflow(IItemHandler target, ItemStack stack, boolean simulate, Operation<ItemStack> original) {
        int before = stack.getCount();
        ItemStack remainder = original.call(target, stack, simulate);
        Counting c = Counting.active();
        if (c != null && !simulate) {
            BasinBlockEntity self = (BasinBlockEntity) (Object) this;
            if (c.isMover(self.getLevel(), self.getBlockPos())) {
                BlockPos to = self.getBlockPos().below().relative(self.getBlockState().getValue(BasinBlock.FACING));
                c.moved(self.getLevel(), self.getBlockPos(), to, stack, before - remainder.getCount(), true);
            }
        }
        return remainder;
    }
}
