package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.KineticJoin;
import com.minecraftindustrialcomplex.mic_rtsim_create.KineticSyncPending;
import com.minecraftindustrialcomplex.mic_rtsim_create.StaleKinetics;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import net.minecraft.server.level.ServerLevel;
import org.jetbrains.annotations.Nullable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

import java.util.Objects;

/**
 * A part of a kinetic network that comes back stale takes its source's speed instead of breaking
 * (SECTIONS.md §2.8, slice S1; PATCHES.md row 56). Rotation no longer propagates into a chunk that is not
 * loaded ({@link RotationPropagatorMixin}), so a chain conveyor in an away chunk keeps its last speed while
 * the loaded end's source changes: a boiler's level, a motor's setting, a reversal. When the chunk loads,
 * each block entity's first tick attaches it ({@code attachKinetics}, Create's
 * {@code propagateNewSource}), which on a speed that no longer matches a neighbour in the same network,
 * or on opposite signs, breaks the block ({@code Level.destroyBlock}). Create already does this to a shaft
 * across a chunk border whose source changed while the chunk was not loaded; for a chain it never
 * happened, because Create loaded the far chunk.
 *
 * <p>At a block entity's first attach, when it takes its speed from a block entity in another chunk that
 * is loaded and no longer gives it that speed or network ({@link StaleKinetics#staleSource}), it is
 * re-synced instead: Create's own {@code propagateMissingSource} clears it and everything that takes its
 * speed from it, then {@code propagateNewSource} from the source gives the part the source's speed, as
 * Create does when a source changes. The re-sync waits one tick, so that the block entities of the chunk
 * that loaded have run their first tick ({@code initialize}) before it; one it reaches before that joins
 * its network as below. Every other attach is Create's own. A chunk that is FULL and does not tick
 * attaches nothing, whether it loaded so (ticket level 33) or came back FULL without loading (it dropped
 * below FULL and stayed in memory): {@link StaleKinetics#chunkFull} re-syncs it, and for a chunk loaded on
 * the server thread usually has done so before the chunk, if it ticks, reaches its first attach, which then
 * finds nothing stale; when the FULL report comes later (under load), the attach re-syncs it here.
 *
 * <p>A block entity that has not joined its network (its chunk is FULL and has not ticked) joins it as its
 * first tick would ({@link KineticJoin}, PATCHES.md row 58) wherever Create lists it again or takes it out
 * first: at {@code removeSource} and {@code setSpeed} (before the speed the join reads changes), at a
 * {@code setNetwork} to another network, at {@code remove} and at {@code clearKineticInformation}; a
 * generator also at {@code updateGeneratedRotation} ({@code GeneratingKineticBlockEntityMixin}), and a
 * powered shaft before its engine changes it ({@code PoweredShaftBlockEntityMixin}). A
 * re-sync's frontier reaches such block entities in every FULL chunk around, and Create's {@code add} would
 * list one without counting it out of the network's unloaded share, or its removal would leave its share
 * there.
 *
 * <p>Also holds the flag of a network update held back while the chunk was not loaded
 * ({@link KineticSyncPending}, PATCHES.md row 54). Runtime fields only, never saved.
 */
@Mixin(value = KineticBlockEntity.class, remap = false)
public abstract class KineticBlockEntityMixin implements KineticSyncPending, KineticJoin {
    @Shadow
    public @Nullable Long network;
    @Shadow
    protected float capacity;
    @Shadow
    protected float stress;
    @Shadow
    private int networkSize;
    @Shadow
    protected float lastStressApplied;
    @Shadow
    protected float lastCapacityProvided;
    @Unique
    private boolean mic_rtsim$attached;
    @Unique
    private boolean mic_rtsim$deferred;
    @Unique
    private boolean mic_rtsim$syncPending;
    /** Create's {@code initialize} has run, or {@link #mic_rtsim$join} ran in its place. */
    @Unique
    private boolean mic_rtsim$initialized;

    @Override
    public void mic_rtsim$join() {
        KineticBlockEntity self = (KineticBlockEntity) (Object) this;
        if (mic_rtsim$initialized || !(self.getLevel() instanceof ServerLevel))
            return;
        mic_rtsim$initialized = true;
        if (network == null)
            return;
        // KineticBlockEntity.initialize, less SmartBlockEntity's (its behaviours start at the first tick)
        KineticNetwork net = self.getOrCreateNetwork();
        if (!net.initialized)
            net.initFromTE(capacity, stress, networkSize);
        net.addSilently(self, lastCapacityProvided, lastStressApplied);
    }

    @Inject(method = "initialize", at = @At("HEAD"))
    private void mic_rtsim$initializes(CallbackInfo ci) {
        // Create's own join follows; after an earlier one it finds a member (or no network) and does nothing
        mic_rtsim$initialized = true;
    }

    // Where Create lists a block entity again, or takes it out of its network, before its first tick: each
    // before the speed, the network or the stress the join reads changes

    @Inject(method = "removeSource", at = @At("HEAD"))
    private void mic_rtsim$joinBeforeRemoveSource(CallbackInfo ci) {
        if (!mic_rtsim$initialized && network != null)
            mic_rtsim$join();
    }

    @Inject(method = "setSpeed", at = @At("HEAD"))
    private void mic_rtsim$joinBeforeSetSpeed(float speed, CallbackInfo ci) {
        if (!mic_rtsim$initialized && network != null)
            mic_rtsim$join();
    }

    @Inject(method = "setNetwork", at = @At("HEAD"))
    private void mic_rtsim$joinBeforeSetNetwork(@Nullable Long networkIn, CallbackInfo ci) {
        if (!mic_rtsim$initialized && network != null && !Objects.equals(network, networkIn))
            mic_rtsim$join();
    }

    @Inject(method = "remove", at = @At("HEAD"))
    private void mic_rtsim$joinBeforeRemove(CallbackInfo ci) {
        if (!mic_rtsim$initialized && network != null)
            mic_rtsim$join();
    }

    @Inject(method = "clearKineticInformation", at = @At("HEAD"))
    private void mic_rtsim$joinBeforeClear(CallbackInfo ci) {
        if (!mic_rtsim$initialized && network != null)
            mic_rtsim$join();
    }

    @Override
    public boolean mic_rtsim$syncPending() {
        return mic_rtsim$syncPending;
    }

    @Override
    public void mic_rtsim$setSyncPending(boolean pending) {
        mic_rtsim$syncPending = pending;
    }

    @Inject(method = "attachKinetics", at = @At("HEAD"), cancellable = true)
    private void mic_rtsim$resyncStale(CallbackInfo ci) {
        if (mic_rtsim$attached)
            return;
        KineticBlockEntity self = (KineticBlockEntity) (Object) this;
        KineticBlockEntity source = StaleKinetics.staleSource(self);
        if (source == null) {
            mic_rtsim$attached = true;
            return;
        }
        if (!mic_rtsim$deferred) {
            // updateSpeed stays true: the next tick attaches again
            mic_rtsim$deferred = true;
            ci.cancel();
            return;
        }
        mic_rtsim$attached = true;
        self.updateSpeed = false;
        StaleKinetics.resync(self, source);
        ci.cancel();
    }
}
