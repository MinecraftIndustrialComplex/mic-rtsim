package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim_create.Frontier;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainPackageInteractionPacket;
import net.minecraft.world.item.ItemStack;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * A player takes a package off a chain conveyor, or puts one on (SECTIONS.md §2.5, slice S4c1; PATCHES.md row 82).
 * Create 6.0.10's {@code ChainPackageInteractionPacket.applySettings} copies the package it hands the player
 * ({@code ItemStack.copy}, twice: into the main hand or the inventory) and the one the player puts on. The copy
 * loses the frontier's key ({@link Frontier#leaves}): a package the player took is no longer held, and a key on
 * one put on is not Java's. Create's own copy is returned, only the key taken out of it.
 */
@Mixin(value = ChainPackageInteractionPacket.class, remap = false)
public abstract class ChainPackageInteractionPacketMixin {
    @WrapOperation(method = "applySettings(Lnet/minecraft/server/level/ServerPlayer;Lcom/simibubi/create/content/kinetics/chainConveyor/ChainConveyorBlockEntity;)V",
            at = @At(value = "INVOKE", target = "Lnet/minecraft/world/item/ItemStack;copy()Lnet/minecraft/world/item/ItemStack;"), require = 3)
    private ItemStack mic_rtsim$unkeyed(ItemStack stack, Operation<ItemStack> original, @Local(argsOnly = true) ChainConveyorBlockEntity be) {
        ItemStack copy = original.call(stack);
        if (be != null && copy != null && !copy.isEmpty())
            Frontier.leaves(be.getLevel(), be.getBlockPos(), copy);
        return copy;
    }
}
