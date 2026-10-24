package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.content.logistics.packager.repackager.RepackagerBlockEntity;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Count points for a repackager at an inventory (SECTIONS.md §4.2, slice S3; PATCHES.md row 76). The engine
 * compiles a repackager as a packager (its pairs are (repackager, store) like a packager's), but it extends
 * {@code PackagerBlockEntity} and overrides both calls that {@link PackagerBlockEntityMixin} hooks, without
 * calling {@code super}, so those hooks never see it:
 * <ul>
 *   <li>{@code attemptToRepackage}, which {@code attemptToSend} calls, draws packages out of the inventory
 *       behind it with {@code extractItem}: four calls, two simulated probes and two real removals (the first
 *       takes a whole box to send on, the second the fragments of an order it repacked). The real removals
 *       are the moves, counted out; a package counts as its contents (core expands it);</li>
 *   <li>{@code unwrapBox} puts a package it received into the inventory with {@code insertItem}, slot by
 *       slot until one takes it; the real call that takes it is counted in.</li>
 * </ul>
 * The repackaged boxes it makes leave through the repackager's own exit, not into the inventory.
 */
@Mixin(value = RepackagerBlockEntity.class, remap = false)
public abstract class RepackagerBlockEntityMixin {
    @WrapOperation(method = "attemptToRepackage", require = 4, at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/IItemHandler;extractItem(IIZ)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countExtract(IItemHandler inventory, int slot, int amount, boolean simulate, Operation<ItemStack> original) {
        ItemStack taken = original.call(inventory, slot, amount, simulate);
        Counting c = Counting.active();
        if (c != null && !simulate && !taken.isEmpty()) {
            try {
                RepackagerBlockEntity self = (RepackagerBlockEntity) (Object) this;
                if (c.isMover(self.getLevel(), self.getBlockPos()))
                    c.moved(self.getLevel(), self.getBlockPos(), self.targetInventory.getTarget().getConnectedPos(), taken, taken.getCount(), false);
            } catch (RuntimeException e) {
                c.failed("a repackager's extract", e); // never break the machine's tick
            }
        }
        return taken;
    }

    @WrapOperation(method = "unwrapBox", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/IItemHandler;insertItem(ILnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countUnwrap(IItemHandler inventory, int slot, ItemStack stack, boolean simulate, Operation<ItemStack> original) {
        Counting c = Counting.active();
        ItemStack seen = null; // copied only for a counted mover (the handler may change its argument)
        if (c != null && !simulate) {
            try {
                RepackagerBlockEntity self = (RepackagerBlockEntity) (Object) this;
                if (c.isMover(self.getLevel(), self.getBlockPos()))
                    seen = stack.copy();
            } catch (RuntimeException e) {
                c.failed("a repackager's unwrap", e); // never break the machine's tick
            }
        }
        int before = stack.getCount();
        ItemStack remainder = original.call(inventory, slot, stack, simulate);
        if (seen != null) {
            try {
                RepackagerBlockEntity self = (RepackagerBlockEntity) (Object) this;
                c.moved(self.getLevel(), self.getBlockPos(), self.targetInventory.getTarget().getConnectedPos(), seen, before - remainder.getCount(), true);
            } catch (RuntimeException e) {
                c.failed("a repackager's unwrap", e);
            }
        }
        return remainder;
    }
}
