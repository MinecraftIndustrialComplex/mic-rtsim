package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim_create.KineticSyncPending;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
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
 *       and an away member's last stress. Where the chunk is loaded the test is Create's own.</li>
 *   <li>When a block entity joins ({@code add}, or {@code addSilently} at its chunk's load), another one
 *       listed at its position (the object kept for it while its chunk was away) leaves the network: one
 *       object per position, so a part that unloads, loads and unloads again is counted once, not once per
 *       visit. At {@code addSilently} the new one takes the kept one's place without being counted out of
 *       the network's unloaded share again: the first load already did that.</li>
 *   <li>{@code sync} skips a listed block entity whose chunk is not loaded: on an overstress change
 *       {@code onSpeedChanged} runs, which on some block entities reads the blocks around it (a gantry
 *       shaft), and so loaded the chunk. The skipped one is marked ({@link KineticSyncPending}) and gets
 *       the update when its chunk is FULL again ({@code StaleKinetics.chunkFull}); one whose chunk unloaded
 *       is never saved again (its chunk saved it at the unload), and its new object loads as Create saved it.</li>
 *   <li>{@code remove}, which marks one member to recount the network at its next tick
 *       ({@code networkDirty}), also marks the first member whose chunk ticks: a member that does not tick
 *       would never recount it, and with the kept block entities such members stay listed.</li>
 * </ul>
 * Server levels only; nothing changes where every chunk of the network is loaded and no position is listed
 * twice.
 */
@Mixin(value = KineticNetwork.class, remap = false)
public abstract class KineticNetworkMixin {
    @Shadow
    public Map<KineticBlockEntity, Float> sources;
    @Shadow
    public Map<KineticBlockEntity, Float> members;

    /** The block entity that last joined at each position (runtime only). */
    @Unique
    private Long2ObjectOpenHashMap<KineticBlockEntity> mic_rtsim$byPos;

    @WrapOperation(method = {"calculateCapacity", "calculateStress"}, at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/level/Level;getBlockEntity(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/entity/BlockEntity;"),
            require = 2)
    private BlockEntity mic_rtsim$keepUnloaded(Level level, BlockPos pos, Operation<BlockEntity> original, @Local KineticBlockEntity be) {
        if (level instanceof ServerLevel && !Loaded.at(level, pos))
            return be;
        return original.call(level, pos);
    }

    @Inject(method = "add", at = @At("HEAD"))
    private void mic_rtsim$replaceKept(KineticBlockEntity be, CallbackInfo ci) {
        KineticBlockEntity kept = mic_rtsim$join(be);
        if (kept != null) {
            sources.remove(kept);
            members.remove(kept);
        }
    }

    @Inject(method = "addSilently", at = @At("HEAD"), cancellable = true)
    private void mic_rtsim$replaceKeptSilently(KineticBlockEntity be, float lastCapacity, float lastStress, CallbackInfo ci) {
        KineticBlockEntity kept = mic_rtsim$join(be);
        if (kept == null)
            return;
        sources.remove(kept);
        members.remove(kept);
        if (be.isSource())
            sources.put(be, be.calculateAddedStressCapacity());
        members.put(be, be.calculateStressApplied());
        ci.cancel();
    }

    /** Records {@code be} at its position; the other block entity listed there (kept for it), or null. */
    @Unique
    private KineticBlockEntity mic_rtsim$join(KineticBlockEntity be) {
        if (!(be.getLevel() instanceof ServerLevel) || members.containsKey(be))
            return null;
        if (mic_rtsim$byPos == null)
            mic_rtsim$byPos = new Long2ObjectOpenHashMap<>();
        KineticBlockEntity kept = mic_rtsim$byPos.put(be.getBlockPos().asLong(), be);
        return kept != null && kept != be && members.containsKey(kept) ? kept : null;
    }

    @WrapOperation(method = "sync", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/content/kinetics/KineticNetwork;updateFromNetwork(Lcom/simibubi/create/content/kinetics/base/KineticBlockEntity;)V"))
    private void mic_rtsim$syncLoaded(KineticNetwork net, KineticBlockEntity be, Operation<Void> original) {
        if (be.getLevel() instanceof ServerLevel level && !Loaded.at(level, be.getBlockPos())) {
            ((KineticSyncPending) be).mic_rtsim$setSyncPending(true);
            return;
        }
        original.call(net, be);
    }

    @Inject(method = "remove", at = @At("TAIL"))
    private void mic_rtsim$dirtyATickingMember(KineticBlockEntity removed, CallbackInfo ci) {
        if (mic_rtsim$byPos != null)
            mic_rtsim$byPos.remove(removed.getBlockPos().asLong(), removed);
        for (KineticBlockEntity m : members.keySet()) {
            if (!(m.getLevel() instanceof ServerLevel level))
                return;
            if (Loaded.ticks(level, m.getBlockPos())) {
                m.networkDirty = true;
                return;
            }
        }
    }
}
