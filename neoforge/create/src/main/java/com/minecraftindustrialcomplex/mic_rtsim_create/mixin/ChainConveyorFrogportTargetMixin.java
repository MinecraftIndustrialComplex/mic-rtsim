package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlock;
import com.simibubi.create.content.logistics.packagePort.PackagePortTarget;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * A frogport on a chain never looks into a chunk that does not tick (SECTIONS.md §2.8, slice S1; PATCHES.md
 * row 52). {@code ChainConveyorFrogportTarget.register} runs at each of the port's lazy ticks; when the
 * conveyor's turning no longer matches the port's {@code flipped}, it moves the port to the conveyor at the
 * connection's far end, looked up through {@code ChainConveyorBlock.getBlockEntity} (a
 * {@code Level.getBlockEntity}, which loads the chunk), up to {@code maxChainConveyorLength} (32) blocks
 * away. A far conveyor whose chunk does not tick is looked up as nothing: Create then returns, and the port
 * tries again at its next lazy tick. The port's own conveyor ({@code PackagePortTarget.be}) is already
 * guarded by Create ({@code Level.isLoaded}) and lies within {@code packagePortRange} (5) of the port.
 */
@Mixin(value = PackagePortTarget.ChainConveyorFrogportTarget.class, remap = false)
public abstract class ChainConveyorFrogportTargetMixin {
    @WrapOperation(method = "register", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/content/kinetics/chainConveyor/ChainConveyorBlock;getBlockEntity(Lnet/minecraft/world/level/BlockGetter;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/entity/BlockEntity;"))
    private BlockEntity mic_rtsim$farEndTicking(ChainConveyorBlock block, BlockGetter getter, BlockPos pos, Operation<BlockEntity> original) {
        if (getter instanceof Level level && level instanceof net.minecraft.server.level.ServerLevel && !Loaded.ticks(level, pos))
            return null;
        return original.call(block, getter, pos);
    }
}
