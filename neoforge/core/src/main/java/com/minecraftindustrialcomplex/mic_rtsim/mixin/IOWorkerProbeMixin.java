package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.minecraftindustrialcomplex.mic_rtsim.journal.IOProbe;
import com.mojang.datafixers.util.Either;
import net.minecraft.util.thread.ProcessorMailbox;
import net.minecraft.util.thread.StrictQueue;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.IOWorker;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

import java.lang.reflect.Field;
import java.util.Collection;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.function.Supplier;

/** TEMPORARY measurement probe (fix/journal-barrier). Not to be landed. */
@Mixin(IOWorker.class)
public abstract class IOWorkerProbeMixin implements IOProbe.Worker {
    @Shadow @Final private Map<ChunkPos, Object> pendingWrites;
    @Shadow @Final private ProcessorMailbox<StrictQueue.IntRunnable> mailbox;

    @Shadow
    private <T> CompletableFuture<T> submitTask(Supplier<Either<T, Exception>> task) {
        throw new AssertionError();
    }

    @Unique private static final ThreadLocal<long[]> mic_rtsim$start = ThreadLocal.withInitial(() -> new long[1]);
    @Unique private static volatile Field mic_rtsim$result;

    @Inject(method = "store", at = @At("HEAD"))
    private void mic_rtsim$store(CallbackInfoReturnable<?> ci) {
        IOProbe.stores.incrementAndGet();
    }

    @Inject(method = "loadAsync", at = @At("HEAD"))
    private void mic_rtsim$load(CallbackInfoReturnable<?> ci) {
        IOProbe.loads.incrementAndGet();
    }

    @Inject(method = "scanChunk", at = @At("HEAD"))
    private void mic_rtsim$scan(CallbackInfoReturnable<?> ci) {
        IOProbe.scans.incrementAndGet();
    }

    @Inject(method = "runStore", at = @At("HEAD"))
    private void mic_rtsim$runStoreHead(CallbackInfo ci) {
        mic_rtsim$start.get()[0] = System.nanoTime();
        int n = pendingWrites.size();
        IOProbe.maxPending.accumulateAndGet(n, Math::max);
    }

    @Inject(method = "runStore", at = @At("RETURN"))
    private void mic_rtsim$runStoreReturn(CallbackInfo ci) {
        IOProbe.runStores.incrementAndGet();
        IOProbe.runStoreNanos.addAndGet(System.nanoTime() - mic_rtsim$start.get()[0]);
    }

    @Override
    public CompletableFuture<Object[]> mic_rtsim$probe(Collection<ChunkPos> ours, long armNanos) {
        int mailboxAtArm = mailbox.size();
        return submitTask(() -> {
            try {
                long fg = System.nanoTime() - armNanos;
                Object[] out = new Object[4 + ours.size() * 2];
                out[0] = fg;
                out[1] = pendingWrites.size();
                out[2] = mailboxAtArm;
                out[3] = mailbox.size();
                int k = 4;
                for (ChunkPos p : ours) {
                    int idx = -1, i = 0;
                    for (ChunkPos q : pendingWrites.keySet()) {
                        if (q.equals(p)) {
                            idx = i;
                            break;
                        }
                        i++;
                    }
                    Object ps = pendingWrites.get(p);
                    CompletableFuture<?> f = null;
                    if (ps != null) {
                        Field r = mic_rtsim$result;
                        if (r == null) {
                            r = ps.getClass().getDeclaredField("result");
                            r.setAccessible(true);
                            mic_rtsim$result = r;
                        }
                        f = (CompletableFuture<?>) r.get(ps);
                    }
                    long[] at = {-1};
                    if (f != null)
                        f.whenComplete((v, t) -> at[0] = System.nanoTime() - armNanos);
                    out[k++] = idx;
                    out[k++] = f == null ? null : at;
                }
                return Either.left(out);
            } catch (Exception e) {
                return Either.right(e);
            }
        });
    }
}
