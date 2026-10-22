package com.minecraftindustrialcomplex.mic_rtsim_iceandfire.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.industry.Deposits;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.chunk.LevelChunk;

import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * GameTests only (registered when {@code DragonConfig.Test.ENABLED}): a pumpjack stand-in. Each chunk in
 * the drilling set reports a Diesel oil deposit ({@code d/0/createdieselgenerators:oil/<chunk hash>}, the
 * good Diesel Generators' plugin reports for a pumpjack's chunk) with oil left, once per sweep, while it is
 * a loaded member chunk of a district: what the engine reads as a drilled well near a nest (OG-9e).
 */
public final class TestWells implements IndustryPlugin {
    public static final ResourceLocation OIL = ResourceLocation.fromNamespaceAndPath("createdieselgenerators", "oil");
    /** What a drilling chunk starts with, in millibuckets. */
    public static final long START = 1_000_000;

    private record Key(ResourceKey<Level> dim, long chunk) {}

    /** The drilling chunks and the oil each has left. */
    private static final Map<Key, Long> DRILLING = new ConcurrentHashMap<>();

    /** Chunk (cx, cz) of {@code dim} drills from now on. */
    public static void drill(ResourceKey<Level> dim, int cx, int cz) {
        DRILLING.putIfAbsent(new Key(dim, ChunkPos.asLong(cx, cz)), START);
    }

    /** Chunk (cx, cz) of {@code dim} stops drilling. */
    public static void stop(ResourceKey<Level> dim, int cx, int cz) {
        DRILLING.remove(new Key(dim, ChunkPos.asLong(cx, cz)));
    }

    /** No chunk drills (the end of each test). */
    public static void clear() {
        DRILLING.clear();
    }

    public static boolean drilling(ResourceKey<Level> dim, int cx, int cz) {
        return DRILLING.containsKey(new Key(dim, ChunkPos.asLong(cx, cz)));
    }

    @Override
    public void deposits(ServerLevel level, District district, LevelChunk chunk, DepositSink out) {
        ChunkPos cp = chunk.getPos();
        Long left = DRILLING.get(new Key(level.dimension(), cp.toLong()));
        if (left != null && left > 0)
            out.add(IndRecords.DepositPlugin.DIESEL_OIL, Deposits.name(OIL, level.dimension(), cp.x, cp.z), left, false);
    }

    @Override
    public Long applyDeposit(ServerLevel level, int chunkX, int chunkZ, String good, long delta) {
        if (!OIL.equals(Deposits.kindIn(good, level.dimension(), chunkX, chunkZ)))
            return null;
        Key k = new Key(level.dimension(), ChunkPos.asLong(chunkX, chunkZ));
        Long before = DRILLING.get(k);
        if (before == null)
            return null; // not a test well: Diesel's own plugin's, if it is loaded
        long after = Math.max(1, before + delta); // a test well never runs dry
        DRILLING.put(k, after);
        return after - before;
    }
}
