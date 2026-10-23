package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.KineticJoin;
import com.minecraftindustrialcomplex.mic_rtsim_create.KineticSyncPending;
import com.minecraftindustrialcomplex.mic_rtsim_create.StaleKinetics;
import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import net.minecraft.server.level.ServerLevel;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

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
 * Create does when a source changes. The re-sync waits one tick, so that every block entity of the chunk
 * that loaded has joined its network first ({@code initialize}, which counts it out of the network's
 * unloaded share). Every other attach is Create's own. A chunk that is FULL and does not tick attaches
 * nothing, whether it loaded so (ticket level 33) or came back FULL without loading (it dropped below FULL
 * and stayed in memory): {@link StaleKinetics#chunkFull} re-syncs it, and usually has done so before a
 * chunk that loaded and ticks reaches its first attach, which then finds nothing stale.
 *
 * <p>Also holds the flag of a network update held back while the chunk was not loaded
 * ({@link KineticSyncPending}, PATCHES.md row 54), and joins a block entity to its network as its first tick
 * would ({@link KineticJoin}, row 58). Runtime fields only, never saved.
 */
@Mixin(value = KineticBlockEntity.class, remap = false)
public abstract class KineticBlockEntityMixin implements KineticSyncPending, KineticJoin {
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

    @Override
    public void mic_rtsim$join() {
        KineticBlockEntity self = (KineticBlockEntity) (Object) this;
        if (!self.hasNetwork() || !(self.getLevel() instanceof ServerLevel))
            return;
        // KineticBlockEntity.initialize, less SmartBlockEntity's (its behaviours start at the first tick)
        KineticNetwork network = self.getOrCreateNetwork();
        if (network.members.containsKey(self))
            return;
        if (!network.initialized)
            network.initFromTE(capacity, stress, networkSize);
        network.addSilently(self, lastCapacityProvided, lastStressApplied);
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
