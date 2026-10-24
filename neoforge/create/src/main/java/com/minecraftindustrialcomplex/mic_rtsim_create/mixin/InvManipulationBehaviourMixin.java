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
 * the smart observer's extraction. The mover is the behaviour's block entity, but for a mechanical arm's
 * insert at a funnel point, which counts under the arm ({@link Counting#moverOf}, set by
 * {@code ArmFunnelPointMixin}: the compile's edge is the arm's, choice SC125); the inventory is the block the
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
            try {
                InvManipulationBehaviour self = (InvManipulationBehaviour) (Object) this;
                if (c.isMover(self.getWorld(), self.getPos()))
                    c.moved(self.getWorld(), self.getPos(), self.getTarget().getConnectedPos(), taken, taken.getCount(), false);
            } catch (RuntimeException e) {
                c.failed("a behaviour's extract", e); // never break the block's tick
            }
        }
        return taken;
    }

    @WrapOperation(method = "insert", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItemStacked(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countInsert(IItemHandler inventory, ItemStack stack, boolean simulate, Operation<ItemStack> original) {
        Counting c = Counting.active();
        ItemStack seen = null; // copied only for a counted mover (the handler may change its argument)
        if (c != null && !simulate) {
            try {
                InvManipulationBehaviour self = (InvManipulationBehaviour) (Object) this;
                if (c.isMover(self.getWorld(), Counting.moverOf(self.getPos())))
                    seen = stack.copy();
            } catch (RuntimeException e) {
                c.failed("a behaviour's insert", e); // never break the block's tick
            }
        }
        int before = stack.getCount();
        ItemStack remainder = original.call(inventory, stack, simulate);
        if (seen != null) {
            try {
                InvManipulationBehaviour self = (InvManipulationBehaviour) (Object) this;
                c.moved(self.getWorld(), Counting.moverOf(self.getPos()), self.getTarget().getConnectedPos(), seen, before - remainder.getCount(), true);
            } catch (RuntimeException e) {
                c.failed("a behaviour's insert", e);
            }
        }
        return remainder;
    }
}
