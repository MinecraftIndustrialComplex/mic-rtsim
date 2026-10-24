package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_create.Frontier;
import com.minecraftindustrialcomplex.mic_rtsim_create.PackageStores;
import com.simibubi.create.content.logistics.packagePort.PackagePortTarget;
import com.simibubi.create.content.logistics.packagePort.frogport.FrogportBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.LevelAccessor;
import net.neoforged.neoforge.items.IItemHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * The packages a frogport puts on its chain and takes off it (SECTIONS.md §2.5, slice S4a; PATCHES.md row 78).
 * {@code FrogportBlockEntity.tick} ends each animation in one of two calls: past half of a depositing
 * animation, {@code PackagePortTarget.export} puts the package on the chain (false: Create drops it, nothing
 * is recorded); past the end of an arriving one, {@code ItemHandlerHelper.insertItem} puts it in the
 * frogport's inventory (anything left over: Create drops it, nothing is recorded). Both are wrapped, never
 * changed: for a frogport on a chain conveyor in a server level, {@link PackageStores#chainPackage} journals
 * an IND_CHAIN_PACKAGE when the frogport is a package store's. The stack is copied before the call, as both
 * calls may keep or change it. A package the frontier keyed ({@link Frontier}, slice S4c1) has its key taken
 * out before the insert, so nothing of rtsim's stays on it in the frogport; its {@code CHAIN_IN} carries the
 * key when Java still holds it, and the key is released when no {@code CHAIN_IN} carried it
 * ({@link Frontier#afterLanding}).
 */
@Mixin(value = FrogportBlockEntity.class, remap = false)
public abstract class FrogportChainMixin {
    @WrapOperation(method = "tick", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/content/logistics/packagePort/PackagePortTarget;export(Lnet/minecraft/world/level/LevelAccessor;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/item/ItemStack;Z)Z"))
    private boolean mic_rtsim$chainOut(PackagePortTarget target, LevelAccessor level, BlockPos pos, ItemStack box, boolean simulate,
            Operation<Boolean> original) {
        FrogportBlockEntity self = (FrogportBlockEntity) (Object) this;
        if (simulate || !(target instanceof PackagePortTarget.ChainConveyorFrogportTarget) || !(self.getLevel() instanceof ServerLevel)
                || box == null)
            return original.call(target, level, pos, box, simulate);
        ItemStack copy = box.copy();
        boolean done = original.call(target, level, pos, box, simulate);
        if (done)
            PackageStores.chainPackage(self, copy, IndRecords.ChainPackage.CHAIN_OUT);
        return done;
    }

    @WrapOperation(method = "tick", at = @At(value = "INVOKE",
            target = "Lnet/neoforged/neoforge/items/ItemHandlerHelper;insertItem(Lnet/neoforged/neoforge/items/IItemHandler;Lnet/minecraft/world/item/ItemStack;Z)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack mic_rtsim$chainIn(IItemHandler inventory, ItemStack box, boolean simulate, Operation<ItemStack> original) {
        FrogportBlockEntity self = (FrogportBlockEntity) (Object) this;
        if (simulate || !(self.target instanceof PackagePortTarget.ChainConveyorFrogportTarget) || !(self.getLevel() instanceof ServerLevel)
                || box == null || box.isEmpty())
            return original.call(inventory, box, simulate);
        Frontier.Key key = Frontier.key(box);
        if (key != null)
            Frontier.strip(box);
        ItemStack copy = box.copy();
        ItemStack left = original.call(inventory, box, simulate);
        if (left == null || left.isEmpty())
            PackageStores.chainPackage(self, copy, IndRecords.ChainPackage.CHAIN_IN, key);
        Frontier.afterLanding(self.getLevel(), self.getBlockPos(), key);
        return left;
    }
}
