package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds;
import net.minecraft.world.level.Level;
import net.neoforged.fml.ModList;

import java.lang.reflect.Method;
import java.util.UUID;

/**
 * The compile's frame (C32, GB6): 0 for the level itself, else the Sable sub-level the district's chunks
 * belong to. Sable stores a ship's blocks in a plot of ordinary chunks far from the world (its own chunk
 * coordinates), so the extraction is the same as on land; only the frame id tells the engine the graph
 * moves. Sable is an optional dependency and not on the compile classpath: its public API is reached by
 * reflection ({@code Sable.HELPER.getContaining(Level, int, int)}, {@code SubLevel.getUniqueId()}; Sable
 * 2.0.5, PATCHES.md "Other mods' APIs"). The id is {@link CreateIds#of} of the sub-level's UUID.
 * UNVERIFIED in a GameTest: Sable is not on the GameTest classpath (it needs Veil and Rapier's natives,
 * and its GameTest mixins remove ships that touch test boxes).
 */
public final class SableFrames {
    private SableFrames() {}

    private static boolean resolved;
    private static Object helper;
    private static Method getContaining;
    private static Method getUniqueId;

    private static void resolve() {
        resolved = true;
        if (!ModList.get().isLoaded("sable"))
            return;
        try {
            Class<?> sable = Class.forName("dev.ryanhcode.sable.Sable");
            helper = sable.getField("HELPER").get(null);
            getContaining = helper.getClass().getMethod("getContaining", Level.class, int.class, int.class);
            Class<?> access = Class.forName("dev.ryanhcode.sable.companion.SubLevelAccess");
            getUniqueId = access.getMethod("getUniqueId");
        } catch (ReflectiveOperationException | RuntimeException | LinkageError e) {
            helper = null;
            getContaining = null;
            getUniqueId = null;
        }
    }

    /** The frame of the chunk (chunkX, chunkZ): 0 on the level, else the sub-level's id. */
    public static long frame(Level level, int chunkX, int chunkZ) {
        if (!resolved)
            resolve();
        if (getContaining == null)
            return 0;
        try {
            Object sub = getContaining.invoke(helper, level, chunkX, chunkZ);
            if (sub == null)
                return 0;
            Object id = getUniqueId.invoke(sub);
            return id instanceof UUID u ? CreateIds.of(u) : 0;
        } catch (ReflectiveOperationException | RuntimeException e) {
            return 0;
        }
    }
}
