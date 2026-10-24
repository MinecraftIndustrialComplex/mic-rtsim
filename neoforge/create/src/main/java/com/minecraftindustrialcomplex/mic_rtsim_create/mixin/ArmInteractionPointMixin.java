package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmInteractionPoint;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * Count points for the mechanical arm (SECTIONS.md §4.2, slice S3; PATCHES.md rows 69 and 70). The arm moves
 * items only through its interaction points: {@code ArmInteractionPoint.insert} puts the held stack into the
 * inventory at the point and {@code extract} draws a slot's stack out of it. The mover is the arm
 * ({@code ArmBlockEntity}), the inventory the point's block. The base class's two calls are hooked, at the
 * call that touches the inventory and only when it is not simulated: the arm tests a point with
 * {@code simulate} true first. The special points ({@code FunnelPoint}, {@code BeltPoint}, {@code DepotPoint},
 * the composter's) override these methods and do not reach an inventory through them, and the crafter's
 * point calls {@code super.extract}, which is one count: no arm move is counted twice. A funnel point's
 * insert goes through the funnel's own {@code InvManipulationBehaviour}, counted there under the arm
 * ({@code ArmFunnelPointMixin}, choice SC125).
 *
 * <p>No point of Create's own lands on a store node: a funnel point is counted at the funnel's behaviour
 * (under the arm, as the compile's ARM edge to the store behind it names it), and the packager's points use
 * the base calls on the packager's own handler. These two hooks therefore matter for a third-party store block that an arm reaches through the
 * base point; nothing is double counted either way.
 */
@Mixin(value = ArmInteractionPoint.class, remap = false)
public abstract class ArmInteractionPointMixin {
    @WrapOperation(method = "insert", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItem(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countInsert(IItemHandler handler, ItemStack stack, boolean simulate, Operation<ItemStack> original,
            @Local(argsOnly = true) ArmBlockEntity arm) {
        Counting c = Counting.active();
        ItemStack seen = null; // copied only for a counted mover (the handler may change its argument)
        if (c != null && !simulate) {
            try {
                if (c.isMover(arm.getLevel(), arm.getBlockPos()))
                    seen = stack.copy();
            } catch (RuntimeException e) {
                c.failed("an arm's insert", e); // never break the arm's tick
            }
        }
        int before = stack.getCount();
        ItemStack remainder = original.call(handler, stack, simulate);
        if (seen != null) {
            try {
                c.moved(arm.getLevel(), arm.getBlockPos(), ((ArmInteractionPoint) (Object) this).getPos(), seen, before - remainder.getCount(), true);
            } catch (RuntimeException e) {
                c.failed("an arm's insert", e);
            }
        }
        return remainder;
    }

    @WrapOperation(method = "extract(Lcom/simibubi/create/content/kinetics/mechanicalArm/ArmBlockEntity;IIZ)Lnet/minecraft/world/item/ItemStack;",
            at = @At(value = "INVOKE", target = "Lnet/neoforged/neoforge/items/IItemHandler;extractItem(IIZ)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$countExtract(IItemHandler handler, int slot, int amount, boolean simulate, Operation<ItemStack> original,
            @Local(argsOnly = true) ArmBlockEntity arm) {
        ItemStack taken = original.call(handler, slot, amount, simulate);
        Counting c = Counting.active();
        if (c != null && !simulate && !taken.isEmpty()) {
            try {
                if (c.isMover(arm.getLevel(), arm.getBlockPos()))
                    c.moved(arm.getLevel(), arm.getBlockPos(), ((ArmInteractionPoint) (Object) this).getPos(), taken, taken.getCount(), false);
            } catch (RuntimeException e) {
                c.failed("an arm's extract", e); // never break the arm's tick
            }
        }
        return taken;
    }
}
