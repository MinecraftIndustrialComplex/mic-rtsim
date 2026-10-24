package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.foundation.blockEntity.behaviour.inventory.InvManipulationBehaviour;
import com.simibubi.create.foundation.item.ItemHelper.ExtractionCountMode;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

import java.util.function.Predicate;

/**
 * Count points for the movers that reach an inventory through Create's {@code InvManipulationBehaviour}
 * (SECTIONS.md §4.2, slice S3; PATCHES.md rows 64 and 65): the andesite and brass funnels (and the funnel
 * under a belt, a dropped item entity, a mechanical arm's funnel point), the mechanical crafter's output,
 * the smart observer's extraction. The mover is the behaviour's block entity; the inventory is the block the
 * behaviour is aimed at ({@code getTarget().getConnectedPos()}). Counted once, at the one call that touches
 * the inventory, and only when it is not simulated ({@code simulateNext} is read into the call's flag, so a
 * {@code simulate().insert(...)} is a probe, not a move; a client-side call returns before it).
 *
 * <p>The packager, the chute, the frogport and the basin hold this behaviour (or not) but move through their
 * own calls, hooked in their own mixins: none of those calls goes through these two methods, so no move is
 * counted twice.
 */
@Mixin(value = InvManipulationBehaviour.class, remap = false)
public abstract class InvManipulationBehaviourMixin {
    @WrapOperation(method = "extract(Lcom/simibubi/create/foundation/item/ItemHelper$ExtractionCountMode;ILjava/util/function/Predicate;)Lnet/minecraft/world/item/ItemStack;",
            at = @At(value = "INVOKE",
                    target = "Lcom/simibubi/create/foundation/item/ItemHelper;extract(Lnet/neoforged/neoforge/items/IItemHandler;Ljava/util/function/Predicate;Lcom/simibubi/create/foundation/item/ItemHelper$ExtractionCountMode;IZ)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countExtract(IItemHandler inventory, Predicate<ItemStack> test, ExtractionCountMode mode, int amount, boolean simulate,
            Operation<ItemStack> original) {
        ItemStack taken = original.call(inventory, test, mode, amount, simulate);
        Counting c = Counting.active();
        if (c != null && !simulate && !taken.isEmpty()) {
            InvManipulationBehaviour self = (InvManipulationBehaviour) (Object) this;
            if (c.isMover(self.getWorld(), self.getPos()))
                c.moved(self.getWorld(), self.getPos(), self.getTarget().getConnectedPos(), taken, taken.getCount(), false);
        }
        return taken;
    }

    @WrapOperation(method = "insert", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItemStacked(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countInsert(IItemHandler inventory, ItemStack stack, boolean simulate, Operation<ItemStack> original) {
        Counting c = Counting.active();
        ItemStack seen = c != null && !simulate ? stack.copy() : null; // the handler may change its argument
        int before = stack.getCount();
        ItemStack remainder = original.call(inventory, stack, simulate);
        if (seen != null) {
            InvManipulationBehaviour self = (InvManipulationBehaviour) (Object) this;
            if (c.isMover(self.getWorld(), self.getPos()))
                c.moved(self.getWorld(), self.getPos(), self.getTarget().getConnectedPos(), seen, before - remainder.getCount(), true);
        }
        return remainder;
    }
}
