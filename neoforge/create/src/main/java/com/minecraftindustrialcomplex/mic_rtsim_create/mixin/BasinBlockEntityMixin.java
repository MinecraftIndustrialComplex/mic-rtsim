package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.content.processing.basin.BasinBlock;
import com.simibubi.create.content.processing.basin.BasinBlockEntity;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.inventory.InvManipulationBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Count point for a basin's side output (SECTIONS.md §4.2, slice S3; PATCHES.md row 71). A basin whose spout
 * faces sideways does not keep its recipe outputs: {@code acceptOutputs} puts them in {@code spoutputBuffer},
 * and {@code tryClearingSpoutputOverflow}, which runs while the buffer is not empty, inserts each into the
 * target at {@code below().relative(FACING)} (a simulated insert to test, then the real one). The real
 * insert is the move: counted once, there. Outputs that stay in the basin ({@code FACING} down) are not a
 * move between blocks and are in another method; a mixer or press over the basin calls {@code acceptOutputs},
 * which moves nothing into the inventory, so it is not a mover of that inventory (the basin's position is the
 * mover's, whichever model the engine gives the block).
 *
 * <p>The target is the item handler the block at that position offers on its side facing the basin, else (a
 * funnel offers none) the inventory behind it, taken from the block's {@code InvManipulationBehaviour}
 * ({@code Optional.ofNullable(getCapability(...)).orElse(inserter.getInventory())}). That second case is a
 * basin pouring through a funnel into a store (an andesite or brass funnel facing up on a vault or chest): the
 * insert skips the funnel's {@code InvManipulationBehaviour.insert}, so the behaviour's own count point never
 * sees it, and the engine's pair for it is (funnel, store), from the funnel's adjacency edge: it is counted
 * here under the funnel's position and the store behind it. Otherwise the pair is (basin, the block at the
 * spout), which the engine names only when that block is a store-like node (none is in Create 6.0.10).
 */
@Mixin(value = BasinBlockEntity.class, remap = false)
public abstract class BasinBlockEntityMixin {
    @WrapOperation(method = "tryClearingSpoutputOverflow", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItemStacked(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countOverflow(IItemHandler target, ItemStack stack, boolean simulate, Operation<ItemStack> original) {
        Counting c = Counting.active();
        ItemStack seen = c != null && !simulate ? stack.copy() : null;
        int before = stack.getCount();
        ItemStack remainder = original.call(target, stack, simulate);
        if (seen != null) {
            try {
                BasinBlockEntity self = (BasinBlockEntity) (Object) this;
                Level level = self.getLevel();
                Direction facing = self.getBlockState().getValue(BasinBlock.FACING);
                BlockPos spout = self.getBlockPos().below().relative(facing);
                int n = before - remainder.getCount();
                InvManipulationBehaviour behind = level.getCapability(Capabilities.ItemHandler.BLOCK, spout, facing.getOpposite()) != null ? null
                        : BlockEntityBehaviour.get(level, spout, InvManipulationBehaviour.TYPE);
                if (behind != null) {
                    if (c.isMover(level, spout))
                        c.moved(level, spout, behind.getTarget().getConnectedPos(), seen, n, true);
                } else if (c.isMover(level, self.getBlockPos())) {
                    c.moved(level, self.getBlockPos(), spout, seen, n, true);
                }
            } catch (RuntimeException e) {
                c.failed("a basin's spout", e); // never break the basin's tick
            }
        }
        return remainder;
    }
}
