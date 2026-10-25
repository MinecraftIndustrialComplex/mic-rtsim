package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Counting;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.foundation.blockEntity.behaviour.inventory.InvManipulationBehaviour;
import net.minecraft.world.item.ItemStack;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * A mechanical arm's insert at a funnel point (SECTIONS.md §4.2, slice S3; PATCHES.md row 81): the point puts
 * the arm's stack through the funnel's {@code InvManipulationBehaviour}, whose count point
 * ({@code InvManipulationBehaviourMixin}) would name the funnel as the mover. The compile names that move an
 * ARM edge from the arm to the store behind the funnel, and the funnel a junction of its own (with nothing
 * else feeding it, a section of its own), so the move is counted under the arm while the call runs
 * ({@link Counting#armThroughFunnel}, choice SC125). Nothing else changes: the funnel's own moves (a belt or
 * an item entity into it) still count under the funnel.
 */
@Mixin(targets = "com.simibubi.create.content.kinetics.mechanicalArm.AllArmInteractionPointTypes$FunnelPoint", remap = false)
public abstract class ArmFunnelPointMixin {
    @WrapOperation(method = "insert", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/foundation/blockEntity/behaviour/inventory/InvManipulationBehaviour;insert(Lnet/minecraft/world/item/ItemStack;)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$asTheArm(InvManipulationBehaviour inserter, ItemStack stack, Operation<ItemStack> original,
            @Local(argsOnly = true) ArmBlockEntity arm) {
        if (Counting.active() == null)
            return original.call(inserter, stack);
        Counting.armThroughFunnel(arm.getBlockPos());
        try {
            return original.call(inserter, stack);
        } finally {
            Counting.armThroughFunnel(null);
        }
    }
}
