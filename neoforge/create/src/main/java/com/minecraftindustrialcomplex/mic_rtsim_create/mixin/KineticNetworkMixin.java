package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

import java.util.Map;

/**
 * A kinetic network's recount never loads a chunk (SECTIONS.md §2.6 and §2.8, slice S1; PATCHES.md rows 53
 * and 54). Create 6.0.10 keeps a block entity whose chunk unloaded in the network's {@code sources} and
 * {@code members} ({@code SmartBlockEntity.setRemoved} skips {@code remove()} on a chunk unload), as it last
 * was. {@code calculateCapacity} and {@code calculateStress} look each listed one up through
 * {@code Level.getBlockEntity} and drop it when another block entity is there: the lookup loaded its chunk,
 * whose new block entity is another object, so every recount of a network reaching an away district loaded
 * that district's chunks and dropped them from the count.
 * <ul>
 *   <li>The recount's lookup of a block entity whose chunk is not loaded answers the listed block entity
 *       itself: it stays, as Create last saw it, so the loaded part keeps an away source's last capacity
 *       and an away member's last stress. Where the chunk is loaded the test is Create's own: the old object
 *       goes as soon as its chunk has loaded (its new block entity has joined by then, through
 *       {@code addSilently}).</li>
 *   <li>{@code sync} skips a listed block entity whose chunk is not loaded: what it writes into that
 *       object is never saved (its chunk saved it at the unload), and on an overstress change
 *       {@code onSpeedChanged} runs, which on some block entities reads the blocks around it (a gantry
 *       shaft), and so loaded the chunk.</li>
 *   <li>{@code remove}, which marks one member to recount the network at its next tick
 *       ({@code networkDirty}), also marks the first member whose chunk is loaded: a member that does not
 *       tick would never recount it, and with the kept block entities such members stay listed.</li>
 * </ul>
 * Server levels only; nothing changes where every chunk of the network is loaded.
 */
@Mixin(value = KineticNetwork.class, remap = false)
public abstract class KineticNetworkMixin {
    @Shadow
    public Map<KineticBlockEntity, Float> members;

    @WrapOperation(method = {"calculateCapacity", "calculateStress"}, at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/level/Level;getBlockEntity(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/entity/BlockEntity;"),
            require = 2)
    private BlockEntity mic_rtsim$keepUnloaded(Level level, BlockPos pos, Operation<BlockEntity> original, @Local KineticBlockEntity be) {
        if (level instanceof ServerLevel && !Loaded.at(level, pos))
            return be;
        return original.call(level, pos);
    }

    @WrapOperation(method = "sync", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/content/kinetics/KineticNetwork;updateFromNetwork(Lcom/simibubi/create/content/kinetics/base/KineticBlockEntity;)V"))
    private void mic_rtsim$syncLoaded(KineticNetwork net, KineticBlockEntity be, Operation<Void> original) {
        if (be.getLevel() instanceof ServerLevel level && !Loaded.at(level, be.getBlockPos()))
            return;
        original.call(net, be);
    }

    @Inject(method = "remove", at = @At("TAIL"))
    private void mic_rtsim$dirtyALoadedMember(KineticBlockEntity removed, CallbackInfo ci) {
        for (KineticBlockEntity m : members.keySet()) {
            if (!(m.getLevel() instanceof ServerLevel level))
                return;
            if (Loaded.at(level, m.getBlockPos())) {
                m.networkDirty = true;
                return;
            }
        }
    }
}
