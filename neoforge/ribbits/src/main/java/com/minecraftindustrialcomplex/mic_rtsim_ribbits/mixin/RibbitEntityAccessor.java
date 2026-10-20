package com.minecraftindustrialcomplex.mic_rtsim_ribbits.mixin;

import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.entity.goal.RibbitApplyBuffGoal;
import net.minecraft.core.BlockPos;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/**
 * Two private fields of {@code RibbitEntity} (Ribbits 4.1.6):
 * <ul>
 *   <li>{@code homePosition}: Ribbits has no setter (it is set in {@code finalizeSpawn}, by a player's
 *       amethyst shard, and from NBT). A travelling puppet is re-homed to its waypoint, so Ribbits' own
 *       GoHome goal does not walk it back to where it spawned.</li>
 *   <li>{@code applyBuffGoal}: read only by the GameTests, to drive a sorcerer's buff goal.</li>
 * </ul>
 */
@Mixin(RibbitEntity.class)
public interface RibbitEntityAccessor {
    @Accessor("homePosition")
    void mic_rtsim_ribbits$setHomePosition(BlockPos pos);

    @Accessor("applyBuffGoal")
    RibbitApplyBuffGoal mic_rtsim_ribbits$applyBuffGoal();
}
