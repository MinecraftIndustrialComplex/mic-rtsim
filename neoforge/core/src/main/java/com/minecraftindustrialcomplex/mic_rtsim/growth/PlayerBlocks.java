package com.minecraftindustrialcomplex.mic_rtsim.growth;

import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.mojang.serialization.Codec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.chunk.LevelChunk;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * The blocks players placed in one chunk, as a 4096-bit set per section that has any
 * ({@code mic_rtsim:player_blocks}, saved with the chunk). Village growth never places over them
 * (PLAN.md §8.8). 512 bytes per touched section; nothing for untouched chunks.
 *
 * <p>Marked from {@code BlockEvent.EntityPlaceEvent} by a player (choice G12). A block a player later
 * breaks stays marked: the mark is conservative, it only ever makes a plot less free. Blocks placed
 * before the mod was installed, by other mods' machines, or by buckets are not marked; the free-plot
 * test also refuses any block entity and any block that is not air, plants, water or terrain.
 */
public final class PlayerBlocks {
    private final Map<Integer, long[]> sections = new HashMap<>();

    public static final PlayerBlocks EMPTY = new PlayerBlocks();

    private record Section(int y, long[] bits) {
        static final Codec<Section> CODEC = RecordCodecBuilder.create(i -> i.group(
                Codec.INT.fieldOf("y").forGetter(Section::y),
                Codec.LONG_STREAM.xmap(java.util.stream.LongStream::toArray, java.util.Arrays::stream).fieldOf("bits").forGetter(Section::bits)
        ).apply(i, Section::new));
    }

    public static final Codec<PlayerBlocks> CODEC = Section.CODEC.listOf().xmap(list -> {
        PlayerBlocks p = new PlayerBlocks();
        for (Section s : list)
            if (s.bits().length == 64)
                p.sections.put(s.y(), s.bits().clone());
        return p;
    }, p -> {
        List<Section> out = new ArrayList<>();
        p.sections.forEach((y, bits) -> out.add(new Section(y, bits.clone())));
        return out;
    });

    public boolean isEmpty() {
        return sections.isEmpty();
    }

    private static int index(BlockPos pos) {
        return ((pos.getY() & 15) << 8) | ((pos.getZ() & 15) << 4) | (pos.getX() & 15);
    }

    public boolean has(BlockPos pos) {
        long[] bits = sections.get(pos.getY() >> 4);
        if (bits == null)
            return false;
        int i = index(pos);
        return (bits[i >>> 6] & (1L << (i & 63))) != 0;
    }

    private void set(BlockPos pos) {
        long[] bits = sections.computeIfAbsent(pos.getY() >> 4, k -> new long[64]);
        int i = index(pos);
        bits[i >>> 6] |= 1L << (i & 63);
    }

    /**
     * Marks a block a player placed. Each chunk owns its own instance (made here, or decoded with the
     * chunk), so the mark is set in place: no copy per placed block.
     */
    public static void mark(LevelChunk chunk, BlockPos pos) {
        PlayerBlocks p = chunk.getExistingDataOrNull(RtsimAttachments.PLAYER_BLOCKS);
        if (p != null && p.has(pos))
            return;
        if (p == null || p == EMPTY) {
            p = new PlayerBlocks();
            chunk.setData(RtsimAttachments.PLAYER_BLOCKS, p);
        }
        p.set(pos);
        chunk.setUnsaved(true);
    }

    /** True when a player placed the block at {@code pos} (the chunk must be the one holding it). */
    public static boolean placedByPlayer(LevelChunk chunk, BlockPos pos) {
        PlayerBlocks p = chunk.getExistingDataOrNull(RtsimAttachments.PLAYER_BLOCKS);
        return p != null && p.has(pos);
    }

    /** True when any marked block of the chunk is inside the box (cheap: walks only marked sections). */
    public static boolean anyIn(LevelChunk chunk, net.minecraft.world.level.levelgen.structure.BoundingBox box) {
        PlayerBlocks p = chunk.getExistingDataOrNull(RtsimAttachments.PLAYER_BLOCKS);
        if (p == null)
            return false;
        int cx = chunk.getPos().getMinBlockX(), cz = chunk.getPos().getMinBlockZ();
        for (Map.Entry<Integer, long[]> e : p.sections.entrySet()) {
            int sy = e.getKey() << 4;
            if (sy + 15 < box.minY() || sy > box.maxY())
                continue;
            long[] bits = e.getValue();
            for (int w = 0; w < 64; w++) {
                long word = bits[w];
                while (word != 0) {
                    int b = Long.numberOfTrailingZeros(word);
                    word &= word - 1;
                    int i = (w << 6) | b;
                    int x = cx + (i & 15), z = cz + ((i >> 4) & 15), y = sy + (i >> 8);
                    if (box.isInside(x, y, z))
                        return true;
                }
            }
        }
        return false;
    }
}
