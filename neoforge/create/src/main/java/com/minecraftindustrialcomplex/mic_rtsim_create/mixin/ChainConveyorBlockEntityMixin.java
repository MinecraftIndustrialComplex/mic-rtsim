package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorRoutingTable;
import net.minecraft.core.BlockPos;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * A chain conveyor never looks into a chunk that does not tick (SECTIONS.md §2.8, slice S1; PATCHES.md
 * rows 50 and 51). Create 6.0.10 looks connected conveyors and the frogports on its chains up through
 * {@code Level.getBlockEntity}, which loads a chunk that is not loaded: a conveyor on a chain into an away
 * district loaded the far chunk at every advert (about every 80 ticks), and so partly loaded the district.
 * A position in another chunk that does not tick ({@link Loaded#ticksFrom}) is looked up as nothing:
 * <ul>
 *   <li>{@code tick}'s four lookups of connected conveyors: an advert to it is skipped (it writes only into
 *       the far table, which does not tick); on a reversal the packages stay on this conveyor's side (they
 *       wait at the end of the connection); an arriving package stays at the end of its connection, as
 *       Create keeps one whose target is not a conveyor; the hand-off lookup answers nothing, and
 *       {@link #mic_rtsim$exitTicks} refuses the hand-off itself;</li>
 *   <li>{@code canAcceptPackagesFor} (a frogport's simulated export onto a connection): refused;</li>
 *   <li>{@code exportToPort} and {@code notifyPortToAnticipate} (a frogport on the chain, whose entry stays in
 *       {@code loopPorts} and {@code travelPorts} after its chunk unloads, since Create deregisters a port only
 *       when it is destroyed): not exported, as to a port that is backed up, and not told.</li>
 * </ul>
 * Create's own answer is unchanged wherever the chunk ticks. On a client or a virtual level nothing changes.
 */
@Mixin(value = ChainConveyorBlockEntity.class, remap = false)
public abstract class ChainConveyorBlockEntityMixin {
    @WrapOperation(method = {"tick", "canAcceptPackagesFor", "exportToPort", "notifyPortToAnticipate"},
            at = @At(value = "INVOKE", target = "Lnet/minecraft/world/level/Level;getBlockEntity(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/entity/BlockEntity;"),
            require = 7)
    private BlockEntity mic_rtsim$tickingOnly(Level level, BlockPos pos, Operation<BlockEntity> original) {
        BlockPos self = ((ChainConveyorBlockEntity) (Object) this).getBlockPos();
        if (!Loaded.ticksFrom(level, self, pos))
            return null;
        return original.call(level, pos);
    }

    /**
     * The hand-off of a looping package to a connected conveyor (Create's test, {@code tick}): the lookup
     * above answers nothing for a conveyor that does not tick, and Create would then hand the package on
     * without asking it for room. The exit is answered as none ({@code BlockPos.ZERO}, what
     * {@code getExitFor} gives with no route, which no connection equals): the package keeps circling here,
     * as when the far conveyor is full.
     */
    @WrapOperation(method = "tick", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/content/kinetics/chainConveyor/ChainConveyorRoutingTable;getExitFor(Lnet/minecraft/world/item/ItemStack;)Lnet/minecraft/core/BlockPos;"))
    private BlockPos mic_rtsim$exitTicks(ChainConveyorRoutingTable table, ItemStack box, Operation<BlockPos> original) {
        BlockPos exit = original.call(table, box);
        if (exit == null || exit.equals(BlockPos.ZERO))
            return exit;
        ChainConveyorBlockEntity self = (ChainConveyorBlockEntity) (Object) this;
        Level level = self.getLevel();
        if (level != null && !Loaded.ticksFrom(level, self.getBlockPos(), self.getBlockPos().offset(exit)))
            return BlockPos.ZERO;
        return exit;
    }
}
