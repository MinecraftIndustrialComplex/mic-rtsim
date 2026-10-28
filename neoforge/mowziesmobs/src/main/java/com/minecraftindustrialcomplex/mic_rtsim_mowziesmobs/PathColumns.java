package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.pathfinder.NodeEvaluator;
import net.minecraft.world.level.pathfinder.PathComputationType;
import net.minecraft.world.level.pathfinder.PathType;

/**
 * What the jar's {@code MMPathNavigateGround.sweep} asks of the world for one column of its walk (1.8.2, decompiled with
 * vineflower), answered as the jar answers it, for {@link PathSweep}: every cell of the column from {@code y0} to {@code y1}
 * (exclusive) must be pathfindable by land; the block under {@code y0} must not be water, lava or open air; and the cell at
 * {@code y0} must be one the mob may path through (a malus of 0 up to but not including 8) that is not fire, danger fire or
 * other damage. One per sweep, as the jar made its block position per sweep.
 */
public final class PathColumns implements PathSweep.Columns {
    private final Level level;
    private final NodeEvaluator evaluator;
    private final Mob mob;
    private final BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();

    public PathColumns(Level level, NodeEvaluator evaluator, Mob mob) {
        this.level = level;
        this.evaluator = evaluator;
        this.mob = mob;
    }

    @Override
    public boolean column(int x, int z, int y0, int y1, int stepY) {
        for (int y = y0; stepY > 0 ? y < y1 : y > y1; y += stepY) {
            BlockState block = level.getBlockState(pos.set(x, y, z));
            if (!block.isPathfindable(PathComputationType.LAND))
                return false;
        }
        PathType below = evaluator.getPathType(mob, new BlockPos(x, y0 - 1, z));
        if (below == PathType.WATER || below == PathType.LAVA || below == PathType.OPEN)
            return false;
        PathType in = evaluator.getPathType(mob, new BlockPos(x, y0, z));
        float priority = mob.getPathfindingMalus(in);
        if (priority < 0.0F || priority >= 8.0F)
            return false;
        return in != PathType.DAMAGE_FIRE && in != PathType.DANGER_FIRE && in != PathType.DAMAGE_OTHER;
    }
}
