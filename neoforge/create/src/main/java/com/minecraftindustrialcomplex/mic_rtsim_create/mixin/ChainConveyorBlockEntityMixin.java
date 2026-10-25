package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim_create.Frontier;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorPackage;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorRoutingTable;
import net.minecraft.core.BlockPos;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

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
 *
 * <p>Slice S4c1 adds the frontier (SECTIONS.md §2.5, Q5; PATCHES.md row 80; {@link Frontier}): before the
 * routing table ticks, routes toward a far frogport are kept, and on the first tick since the conveyor loaded
 * the packages last held on it are looked for ({@link Frontier#beforeRoutes}); a refused
 * hand-off holds its package ({@link Frontier#hold}); a destroyed conveyor's kept routes go, and a package
 * Create drops (its conveyor destroyed, its connection removed) loses its key ({@link Frontier#leaves}).
 * Slice S4c2 refuses every hand-off of a package the far replay drew (PATCHES.md row 51), and the first tick
 * also takes off the drawn packages whose removal waited for the conveyor to load.
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
     * as when the far conveyor is full. A package whose route there is a far frogport's is held
     * ({@link Frontier#hold}: keyed and recorded once). A package the far replay drew circles here whatever
     * ticks ahead (slice S4c2, {@link Frontier#refusesDrawn}).
     */
    @WrapOperation(method = "tick", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/content/kinetics/chainConveyor/ChainConveyorRoutingTable;getExitFor(Lnet/minecraft/world/item/ItemStack;)Lnet/minecraft/core/BlockPos;"))
    private BlockPos mic_rtsim$exitTicks(ChainConveyorRoutingTable table, ItemStack box, Operation<BlockPos> original) {
        BlockPos exit = original.call(table, box);
        if (exit == null || exit.equals(BlockPos.ZERO))
            return exit;
        ChainConveyorBlockEntity self = (ChainConveyorBlockEntity) (Object) this;
        if (Frontier.refusesDrawn(self, box))
            return BlockPos.ZERO;
        Level level = self.getLevel();
        if (level != null && !Loaded.ticksFrom(level, self.getBlockPos(), self.getBlockPos().offset(exit))) {
            Frontier.hold(self, table, box);
            return BlockPos.ZERO;
        }
        return exit;
    }

    /** The game time of the next tick a route entry may expire at; 0 before the first tick since this conveyor loaded. */
    @Unique
    private long mic_rtsim$scanAt;

    /** The frontier's kept routes, before Create's table ticks (and so before an entry expires). */
    @WrapOperation(method = "tick", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/content/kinetics/chainConveyor/ChainConveyorRoutingTable;tick()V"))
    private void mic_rtsim$keepRoutes(ChainConveyorRoutingTable table, Operation<Void> original) {
        mic_rtsim$scanAt = Frontier.beforeRoutes((ChainConveyorBlockEntity) (Object) this, table, mic_rtsim$scanAt);
        original.call(table);
    }

    @Inject(method = "destroy", at = @At("HEAD"))
    private void mic_rtsim$destroyed(CallbackInfo ci) {
        Frontier.destroyed((ChainConveyorBlockEntity) (Object) this);
    }

    /** Create drops a package off the chain (private {@code drop}: from {@code destroy} and {@code removeConnectionTo}). */
    @Inject(method = "drop", at = @At("HEAD"))
    private void mic_rtsim$dropped(ChainConveyorPackage box, CallbackInfo ci) {
        ChainConveyorBlockEntity self = (ChainConveyorBlockEntity) (Object) this;
        if (box != null)
            Frontier.leaves(self.getLevel(), self.getBlockPos(), box.item);
    }
}
