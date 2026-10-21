package com.minecraftindustrialcomplex.mic_rtsim.journal;

import net.minecraft.world.level.ChunkPos;

import java.util.Collection;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.atomic.AtomicLong;

/** TEMPORARY measurement probe for the journal barrier (fix/journal-barrier). Not to be landed. */
public final class IOProbe {
    public static final AtomicLong stores = new AtomicLong();
    public static final AtomicLong loads = new AtomicLong();
    public static final AtomicLong scans = new AtomicLong();
    public static final AtomicLong runStores = new AtomicLong();
    public static final AtomicLong runStoreNanos = new AtomicLong();
    public static final AtomicLong maxPending = new AtomicLong();

    /** Implemented on IOWorker by the probe mixin. */
    public interface Worker {
        /** On an FG task: [fgDelayNanos, pendingSize, mailboxSize, idx...] and a future per position (null if not pending). */
        CompletableFuture<Object[]> mic_rtsim$probe(Collection<ChunkPos> ours, long armNanos);
    }

    private IOProbe() {}
}
