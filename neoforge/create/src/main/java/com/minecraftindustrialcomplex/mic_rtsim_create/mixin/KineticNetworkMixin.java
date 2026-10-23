package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.llamalad7.mixinextras.injector.wrapmethod.WrapMethod;
import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import com.minecraftindustrialcomplex.mic_rtsim_create.KineticSyncPending;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
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

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;

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
 *       and an away member's last stress. So does the lookup that finds, in its place, a new object of the
 *       same network that has not joined yet (its chunk loaded and is FULL, but has not ticked: a block
 *       entity joins at its first tick); it leaves when that one joins. Otherwise the test is Create's own.</li>
 *   <li>When a block entity joins ({@code add}, or {@code addSilently} at its chunk's load), another one
 *       that joined at its position before (the object kept for it while its chunk was away) leaves the
 *       network: one object per position, so a part that unloads, loads and unloads again is counted once,
 *       not once per visit. At {@code addSilently} a block entity at a position where one joined before
 *       takes its place without being counted out of the network's unloaded share again (the first join
 *       there did that), and takes the network's current capacity and stress: it loaded what its chunk
 *       saved at the unload, which misses every change the network made while it was away. A generator's
 *       {@code updateCapacityFor} and {@code updateStressFor} list a block entity before its first tick:
 *       it takes the kept one's place in the map it enters, and joins as it enters {@code members}. The
 *       positions that joined are kept apart from the objects, and no recount forgets them, so no object
 *       at a position takes its share out of the unloaded share a second time.</li>
 *   <li>{@code sync} skips a listed block entity whose chunk is not loaded: on an overstress change
 *       {@code onSpeedChanged} runs, which on some block entities reads the blocks around it (a gantry
 *       shaft), and so loaded the chunk. One whose chunk dropped below FULL and stays in memory is marked
 *       ({@link KineticSyncPending}) and gets the update when its chunk is FULL again
 *       ({@code StaleKinetics.chunkFull}). One whose chunk unloaded was removed at the unload and is skipped
 *       unmarked: its chunk saved it then and never saves it again, its new object gets the update as it
 *       joins (above), and its {@code onSpeedChanged} would act on the world for a part that is gone.</li>
 *   <li>{@code remove}, which marks one member to recount the network at its next tick
 *       ({@code networkDirty}), also marks the first member whose chunk ticks: a member that does not tick
 *       would never recount it, and with the kept block entities such members stay listed. A block entity
 *       removed before it joined takes the one kept for it out with it (Create returns early for a
 *       non-member); one removed during a {@code sync} (an {@code onSpeedChanged} that breaks it) takes it
 *       out once the sync ends, since the sync is iterating {@code members}. So does a join during a
 *       {@code sync} (row 58, ahead of such a change): its share leaves the unloaded share at once, and it
 *       becomes a member once the sync ends, unless it left by then.</li>
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
    @Shadow
    public Long id;
    @Shadow
    private float unloadedCapacity;
    @Shadow
    private float unloadedStress;
    @Shadow
    private int unloadedMembers;

    @Shadow
    private static float getStressMultiplierForSpeed(float speed) {
        throw new AssertionError();
    }

    /** The block entity that last joined at each position (runtime only); a recount that drops it forgets it. */
    @Unique
    private Long2ObjectOpenHashMap<KineticBlockEntity> mic_rtsim$byPos;
    /**
     * Every position where a block entity joined (runtime only, never forgotten): its share is out of the
     * unloaded share, which is the network's count when it first loaded, so no later object there takes it
     * out again.
     */
    @Unique
    private LongOpenHashSet mic_rtsim$joined;
    /** How deep in {@code sync} this network is; this mixin's changes to its maps wait for it to end. */
    @Unique
    private int mic_rtsim$syncing;
    /** Changes to {@code members} and {@code sources} that wait for the sync iterating {@code members} to end. */
    @Unique
    private List<Runnable> mic_rtsim$afterSync;

    @WrapOperation(method = {"calculateCapacity", "calculateStress"}, at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/level/Level;getBlockEntity(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/entity/BlockEntity;"),
            require = 2)
    private BlockEntity mic_rtsim$keepUnloaded(Level level, BlockPos pos, Operation<BlockEntity> original, @Local KineticBlockEntity be) {
        if (!(level instanceof ServerLevel))
            return original.call(level, pos);
        if (!Loaded.at(level, pos))
            return be;
        BlockEntity found = original.call(level, pos);
        if (Loaded.keptFor((KineticNetwork) (Object) this, be, found))
            return be;
        // dropped by this recount (another block, or a part that joined in its place): forget its position
        if (found != be && mic_rtsim$byPos != null)
            mic_rtsim$byPos.remove(pos.asLong(), be);
        return found;
    }

    @Inject(method = "add", at = @At("HEAD"))
    private void mic_rtsim$replaceKept(KineticBlockEntity be, CallbackInfo ci) {
        if (!(be.getLevel() instanceof ServerLevel) || members.containsKey(be))
            return;
        KineticBlockEntity kept = mic_rtsim$join(be);
        // a part placed anew was never in the unloaded share: a later object here must not take it out
        mic_rtsim$joined.add(be.getBlockPos().asLong());
        if (kept != null) {
            sources.remove(kept);
            members.remove(kept);
        }
    }

    @Inject(method = "addSilently", at = @At("HEAD"), cancellable = true)
    private void mic_rtsim$replaceKeptSilently(KineticBlockEntity be, float lastCapacity, float lastStress, CallbackInfo ci) {
        // a member already (listed by a generator's update before its first tick): Create returns early
        if (!(be.getLevel() instanceof ServerLevel) || members.containsKey(be))
            return;
        if (mic_rtsim$syncing > 0) {
            mic_rtsim$joinAfterSync(be, lastCapacity, lastStress);
            ci.cancel();
            return;
        }
        KineticBlockEntity kept = mic_rtsim$join(be);
        if (kept != null) {
            sources.remove(kept);
            members.remove(kept);
        }
        if (mic_rtsim$joined.add(be.getBlockPos().asLong()))
            return; // the first join here: Create's own, which takes its share out of the unloaded share
        if (be.isSource())
            sources.put(be, be.calculateAddedStressCapacity());
        members.put(be, be.calculateStressApplied());
        ((KineticNetworkInvoker) (Object) this).mic_rtsim$updateFromNetwork(be);
        ci.cancel();
    }

    /**
     * A join during a {@code sync} of this network (row 58's join, ahead of a change an
     * {@code onSpeedChanged} makes to a block entity that has not ticked): {@code sync} is iterating
     * {@code members}, so a new key there would end it with a ConcurrentModificationException. The share
     * leaves the unloaded share now, as Create's {@code addSilently} takes it (once per position, as above);
     * the block entity becomes a member, in the kept one's place, once the sync ends, unless by then it left
     * (removed, unsourced, or in another network: Create's {@code remove} returned early for a non-member).
     */
    @Unique
    private void mic_rtsim$joinAfterSync(KineticBlockEntity be, float lastCapacity, float lastStress) {
        long pos = be.getBlockPos().asLong();
        KineticBlockEntity kept = mic_rtsim$join(be);
        if (mic_rtsim$joined.add(pos)) {
            if (be.isSource())
                unloadedCapacity -= lastCapacity * getStressMultiplierForSpeed(be.getGeneratedSpeed());
            unloadedStress -= lastStress * getStressMultiplierForSpeed(be.getTheoreticalSpeed());
            unloadedMembers = Math.max(0, unloadedMembers - 1);
            unloadedCapacity = Math.max(0, unloadedCapacity);
            unloadedStress = Math.max(0, unloadedStress);
        }
        mic_rtsim$afterSync(() -> {
            if (kept != null) {
                sources.remove(kept);
                members.remove(kept);
            }
            if (!be.isRemoved() && Objects.equals(be.network, id) && !members.containsKey(be)
                    && mic_rtsim$byPos.get(pos) == be) {
                if (be.isSource())
                    sources.put(be, be.calculateAddedStressCapacity());
                members.put(be, be.calculateStressApplied());
                ((KineticNetworkInvoker) (Object) this).mic_rtsim$updateFromNetwork(be);
            }
            // its share left the unloaded share; if it left the network too, nothing else recounts it
            mic_rtsim$dirtyTicking();
        });
    }

    /** Runs {@code change} now, or once the sync iterating {@code members} ends. */
    @Unique
    private void mic_rtsim$afterSync(Runnable change) {
        if (mic_rtsim$syncing == 0) {
            change.run();
            return;
        }
        if (mic_rtsim$afterSync == null)
            mic_rtsim$afterSync = new ArrayList<>();
        mic_rtsim$afterSync.add(change);
    }

    /**
     * A generator's capacity update lists a block entity that has not joined (its chunk is FULL and has not
     * ticked) in {@code sources}: it takes the place there of the one kept for it. The kept one stays a
     * member, counted for its stress, until the new one is.
     */
    @Inject(method = "updateCapacityFor", at = @At("HEAD"))
    private void mic_rtsim$replaceKeptSource(KineticBlockEntity be, float capacity, CallbackInfo ci) {
        if (mic_rtsim$byPos == null || !(be.getLevel() instanceof ServerLevel) || members.containsKey(be))
            return;
        KineticBlockEntity kept = mic_rtsim$byPos.get(be.getBlockPos().asLong());
        if (kept != null && kept != be)
            sources.remove(kept);
    }

    /**
     * A generator's stress update makes a block entity that has not joined a member: it joins there, in
     * the kept one's place, and takes the network's current capacity and stress as at {@code addSilently};
     * its own join later finds it a member (Create's {@code addSilently} returns early). Not marked joined:
     * where nothing joined before, Create never takes its share out of the unloaded share, and the next
     * object there will. A generator's own update joins it first (row 58, {@code GeneratingKineticBlockEntityMixin});
     * this is for any other caller.
     */
    @Inject(method = "updateStressFor", at = @At("HEAD"))
    private void mic_rtsim$replaceKeptMember(KineticBlockEntity be, float stress, CallbackInfo ci) {
        if (!(be.getLevel() instanceof ServerLevel) || members.containsKey(be))
            return;
        KineticBlockEntity kept = mic_rtsim$join(be);
        if (kept != null) {
            sources.remove(kept);
            members.remove(kept);
            ((KineticNetworkInvoker) (Object) this).mic_rtsim$updateFromNetwork(be);
        }
    }

    /**
     * Records {@code be}, a block entity of a server level that is not a member, at its position; the other
     * block entity that joined there before and no recount has dropped since (kept for it while its chunk
     * was away), or null.
     */
    @Unique
    private KineticBlockEntity mic_rtsim$join(KineticBlockEntity be) {
        if (mic_rtsim$byPos == null) {
            mic_rtsim$byPos = new Long2ObjectOpenHashMap<>();
            mic_rtsim$joined = new LongOpenHashSet();
        }
        KineticBlockEntity kept = mic_rtsim$byPos.put(be.getBlockPos().asLong(), be);
        return kept != null && kept != be ? kept : null;
    }

    @WrapMethod(method = "sync")
    private void mic_rtsim$syncThenLeave(Operation<Void> original) {
        mic_rtsim$syncing++;
        try {
            original.call();
        } finally {
            if (--mic_rtsim$syncing == 0 && mic_rtsim$afterSync != null && !mic_rtsim$afterSync.isEmpty()) {
                List<Runnable> changes = mic_rtsim$afterSync;
                mic_rtsim$afterSync = null;
                for (Runnable change : changes)
                    change.run();
            }
        }
    }

    @WrapOperation(method = "sync", at = @At(value = "INVOKE",
            target = "Lcom/simibubi/create/content/kinetics/KineticNetwork;updateFromNetwork(Lcom/simibubi/create/content/kinetics/base/KineticBlockEntity;)V"))
    private void mic_rtsim$syncLoaded(KineticNetwork net, KineticBlockEntity be, Operation<Void> original) {
        if (be.getLevel() instanceof ServerLevel level) {
            // a kept block entity whose chunk unloaded: its new object gets the update as it joins
            if (be.isRemoved())
                return;
            if (!Loaded.at(level, be.getBlockPos())) {
                ((KineticSyncPending) be).mic_rtsim$setSyncPending(true);
                return;
            }
        }
        original.call(net, be);
    }

    /**
     * A block entity that leaves before it joined (the new object of a kept one, broken before its first
     * tick): Create's {@code remove} returns early for it, so the kept one would stay counted until some other
     * recount. It leaves with it.
     */
    @Inject(method = "remove", at = @At("HEAD"))
    private void mic_rtsim$removeKeptOfUnjoined(KineticBlockEntity removed, CallbackInfo ci) {
        if (mic_rtsim$byPos == null || members.containsKey(removed) || !(removed.getLevel() instanceof ServerLevel))
            return;
        long key = removed.getBlockPos().asLong();
        KineticBlockEntity kept = mic_rtsim$byPos.get(key);
        // the kept one is dead (its chunk unloaded); a live member at the position is never taken out
        if (kept == null || kept == removed || !kept.isRemoved() || !members.containsKey(kept))
            return;
        mic_rtsim$afterSync(() -> mic_rtsim$leave(kept));
    }

    /** Takes a dead kept block entity out of the network, if it is listed still. */
    @Unique
    private void mic_rtsim$leave(KineticBlockEntity kept) {
        if (!members.containsKey(kept) || !kept.isRemoved())
            return;
        sources.remove(kept);
        members.remove(kept);
        if (mic_rtsim$byPos != null)
            mic_rtsim$byPos.remove(kept.getBlockPos().asLong(), kept);
        mic_rtsim$dirtyTicking();
    }

    @Inject(method = "remove", at = @At("TAIL"))
    private void mic_rtsim$dirtyATickingMember(KineticBlockEntity removed, CallbackInfo ci) {
        if (mic_rtsim$byPos != null)
            mic_rtsim$byPos.remove(removed.getBlockPos().asLong(), removed);
        mic_rtsim$dirtyTicking();
    }

    /** Marks the first member whose chunk ticks to recount the network at its next tick. */
    @Unique
    private void mic_rtsim$dirtyTicking() {
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
