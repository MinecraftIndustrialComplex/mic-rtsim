package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongSet;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.TickingBlockEntity;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.List;

/**
 * The glass box's only reads of vanilla's non-public members (P7, one accessor class for Minecraft; PATCHES.md
 * row 35). Read only, at compile time on the server thread, through VarHandles resolved once. A member
 * that is gone makes its getter return null; the compile then takes the mean over tick orders.
 */
public final class MinecraftGlassAccess {
    private MinecraftGlassAccess() {}

    private static final VarHandle TICKERS = find("blockEntityTickers");
    private static final VarHandle PENDING_TICKERS = find("pendingBlockEntityTickers");

    private static VarHandle find(String name) {
        try {
            return MethodHandles.privateLookupIn(Level.class, MethodHandles.lookup()).findVarHandle(Level.class, name, List.class);
        } catch (ReflectiveOperationException | RuntimeException e) {
            return null;
        }
    }

    /** True when every member resolved (the GameTests assert it, so a Minecraft update fails loudly there). */
    public static boolean allResolved() {
        return TICKERS != null && PENDING_TICKERS != null;
    }

    /**
     * The block entities' places in the level's tick order: {@code Level.blockEntityTickers} (protected),
     * then {@code pendingBlockEntityTickers} (private; joined at the start of the next
     * {@code tickBlockEntities}), the order {@code tickBlockEntities} ticks them in. Only {@code wanted}
     * positions are kept; a removed ticker (a block entity gone, a chunk unloaded) is skipped. Null when the
     * members are gone. One pass over the level's tickers: call once per compile, not per block entity.
     * Row 35.
     */
    @SuppressWarnings("unchecked")
    public static Long2IntOpenHashMap tickOrder(Level level, LongSet wanted) {
        if (TICKERS == null || PENDING_TICKERS == null || level == null)
            return null;
        Long2IntOpenHashMap out = new Long2IntOpenHashMap();
        out.defaultReturnValue(-1);
        int i = 0;
        for (List<TickingBlockEntity> list : new List[] {(List<TickingBlockEntity>) TICKERS.get(level),
                (List<TickingBlockEntity>) PENDING_TICKERS.get(level)}) {
            if (list == null)
                return null;
            for (TickingBlockEntity t : list) {
                if (!t.isRemoved()) {
                    long p = t.getPos().asLong();
                    if (wanted.contains(p) && !out.containsKey(p))
                        out.put(p, i);
                }
                i++;
            }
        }
        return out;
    }
}
