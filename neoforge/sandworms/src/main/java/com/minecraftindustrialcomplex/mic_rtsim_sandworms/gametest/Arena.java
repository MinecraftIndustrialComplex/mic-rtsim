package com.minecraftindustrialcomplex.mic_rtsim_sandworms.gametest;

import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.phys.Vec3;

import java.util.ArrayList;
import java.util.List;

/**
 * A slab of solid stone for the worms to be "underground" in. Coordinates given to the constructor are
 * relative to the test structure's origin; the box is filled straight into the chunk sections (no light,
 * no heightmaps, no block updates: the tests read only block states), so half a million blocks cost
 * milliseconds. The chunks around it are forced so entities in it tick.
 */
final class Arena {
    final ServerLevel level;
    final BlockPos origin;
    final int x0, x1, y0, y1, z0, z1;
    private final List<ChunkPos> forced = new ArrayList<>();

    Arena(GameTestHelper helper, int rx0, int rx1, int ry0, int ry1, int rz0, int rz1) {
        this.level = helper.getLevel();
        this.origin = helper.absolutePos(BlockPos.ZERO);
        this.x0 = origin.getX() + rx0;
        this.x1 = origin.getX() + rx1;
        this.y0 = origin.getY() + ry0;
        this.y1 = origin.getY() + ry1;
        this.z0 = origin.getZ() + rz0;
        this.z1 = origin.getZ() + rz1;
        for (int cx = (x0 >> 4) - 1; cx <= (x1 >> 4) + 1; cx++)
            for (int cz = (z0 >> 4) - 1; cz <= (z1 >> 4) + 1; cz++)
                if (level.setChunkForced(cx, cz, true))
                    forced.add(new ChunkPos(cx, cz));
    }

    /** A point given relative to the structure origin, absolute. */
    Vec3 at(double rx, double ry, double rz) {
        return new Vec3(origin.getX() + rx, origin.getY() + ry, origin.getZ() + rz);
    }

    boolean contains(Vec3 p) {
        return p.x >= x0 && p.x <= x1 + 1 && p.y >= y0 && p.y <= y1 + 1 && p.z >= z0 && p.z <= z1 + 1;
    }

    void fill(BlockState state) {
        fillBox(x0, x1, y0, y1, z0, z1, state);
    }

    /** An air cube of radius r around the block holding p, floor at p's block row (so r rows above, r below the floor kept solid). */
    void room(Vec3 p, int r) {
        BlockPos c = BlockPos.containing(p);
        fillBox(c.getX() - r, c.getX() + r, c.getY(), c.getY() + 2 * r, c.getZ() - r, c.getZ() + r, Blocks.AIR.defaultBlockState());
    }

    void fillBox(int ax, int bx, int ay, int by, int az, int bz, BlockState state) {
        for (int cx = ax >> 4; cx <= bx >> 4; cx++)
            for (int cz = az >> 4; cz <= bz >> 4; cz++) {
                LevelChunk chunk = level.getChunk(cx, cz);
                for (int y = Math.max(ay, level.getMinBuildHeight()); y <= Math.min(by, level.getMaxBuildHeight() - 1); y++) {
                    LevelChunkSection section = chunk.getSection(chunk.getSectionIndex(y));
                    for (int x = Math.max(ax, cx << 4); x <= Math.min(bx, (cx << 4) + 15); x++)
                        for (int z = Math.max(az, cz << 4); z <= Math.min(bz, (cz << 4) + 15); z++)
                            section.setBlockState(x & 15, y & 15, z & 15, state, false);
                }
                chunk.setUnsaved(true);
            }
    }

    /** Sets one block the ordinary way (block entities, neighbours) for the Create machinery. */
    void set(BlockPos p, BlockState state) {
        level.setBlock(p, state, 3);
    }

    void close() {
        fill(Blocks.AIR.defaultBlockState());
        forced.forEach(p -> level.setChunkForced(p.x, p.z, false));
    }
}
