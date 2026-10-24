package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.fan.AirCurrent;
import com.simibubi.create.content.kinetics.fan.EncasedFanBlock;
import com.simibubi.create.content.kinetics.fan.EncasedFanBlockEntity;
import com.simibubi.create.content.kinetics.fan.processing.FanProcessingType;
import com.simibubi.create.infrastructure.config.AllConfigs;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.Level;

import java.util.ArrayList;
import java.util.List;

/**
 * An encased fan's air current at a given speed, rebuilt from the blocks as {@code AirCurrent.rebuild} does
 * (Create 6.0.10): the current runs from the fan along its facing; it pushes when the speed, turned to the
 * facing's axis direction, is positive; its reach is {@code getMaxDistance} (lerp of the speed over
 * {@code fanRotationArgmax} from 3 to the push or pull distance) cut by {@code AirCurrent.getFlowLimit};
 * the segments carry the processing type of the last catalyst the air passed. The compile builds the
 * current from the walk's speed, not the fan's live {@code airCurrent}, so the zones do not depend on
 * whether the fan turns at that moment (a switch-driven clutch) or has rebuilt since its last block check
 * ({@code fanBlockCheckRate}) (choice GB51).
 */
final class FanCurrent {
    final Direction direction;
    final boolean pushing;
    final float maxDistance;
    private final List<int[]> bounds = new ArrayList<>();
    private final List<FanProcessingType> types = new ArrayList<>();
    /** Per segment: where its type's catalyst is (the last block that gave a type), or null. */
    private final List<BlockPos> catalysts = new ArrayList<>();

    private FanCurrent(Direction direction, boolean pushing, float maxDistance) {
        this.direction = direction;
        this.pushing = pushing;
        this.maxDistance = maxDistance;
    }

    /** The fan's current at signed speed {@code speed}; null when it does not turn. */
    static FanCurrent of(EncasedFanBlockEntity fan, float speed) {
        if (speed == 0)
            return null;
        Level level = fan.getLevel();
        Direction facing = fan.getBlockState().getValue(EncasedFanBlock.FACING);
        boolean pushing = KineticBlockEntity.convertToDirection(speed, facing) > 0;
        var k = AllConfigs.server().kinetics;
        float factor = Math.min(Math.abs(speed) / k.fanRotationArgmax.get(), 1);
        float push = net.minecraft.util.Mth.lerp(factor, 3, k.fanPushDistance.get());
        float pull = net.minecraft.util.Mth.lerp(factor, 3f, k.fanPullDistance.get());
        float max = speed > 0 ? push : pull;
        BlockPos start = fan.getBlockPos();
        // getFlowLimit reads the blocks up to the reach: stop it short of an unloaded chunk, never load one
        for (int d = 1; d <= (int) Math.ceil(max); d++)
            if (!Loaded.at(level, start.relative(facing, d))) {
                max = Math.min(max, d - 1);
                break;
            }
        FanCurrent c = new FanCurrent(facing, pushing, AirCurrent.getFlowLimit(level, start, max, facing));
        int limit = (float) (int) c.maxDistance == c.maxDistance ? (int) c.maxDistance : (int) c.maxDistance + 1;
        int searchStart = pushing ? 1 : limit, searchEnd = pushing ? limit : 1, step = pushing ? 1 : -1, toOffset = pushing ? -1 : 0;
        FanProcessingType type = null;
        BlockPos catalyst = null;
        int[] cur = null;
        FanProcessingType curType = null;
        BlockPos curCatalyst = null;
        for (int i = searchStart; i * step <= searchEnd * step; i += step) {
            BlockPos p = start.relative(facing, i);
            if (!Loaded.at(level, p))
                break;
            FanProcessingType t = FanProcessingType.getAt(level, p);
            if (t != null) {
                type = t;
                catalyst = p;
            }
            if (cur == null) {
                cur = new int[] {i + toOffset, 0};
                curType = type;
                curCatalyst = catalyst;
            } else if (curType != type || curCatalyst != catalyst) {
                cur[1] = i + toOffset;
                c.bounds.add(cur);
                c.types.add(curType);
                c.catalysts.add(curCatalyst);
                cur = new int[] {i + toOffset, 0};
                curType = type;
                curCatalyst = catalyst;
            }
        }
        if (cur != null) {
            cur[1] = searchEnd + step + toOffset;
            c.bounds.add(cur);
            c.types.add(curType);
            c.catalysts.add(curCatalyst);
        }
        return c;
    }

    /** {@code AirCurrent.getTypeAt}: the processing type at {@code offset} blocks from the fan. */
    FanProcessingType typeAt(float offset) {
        int s = segment(offset);
        return s < 0 ? null : types.get(s);
    }

    /** Where the catalyst of the type at {@code offset} is (a blaze burner, lava, water...), or null. */
    BlockPos catalystAt(float offset) {
        int s = segment(offset);
        return s < 0 ? null : catalysts.get(s);
    }

    private int segment(float offset) {
        if (offset < 0 || offset > maxDistance)
            return -1;
        for (int s = 0; s < bounds.size(); s++) {
            int end = bounds.get(s)[1];
            if (pushing ? offset <= end : offset >= end)
                return s;
        }
        return -1;
    }
}
