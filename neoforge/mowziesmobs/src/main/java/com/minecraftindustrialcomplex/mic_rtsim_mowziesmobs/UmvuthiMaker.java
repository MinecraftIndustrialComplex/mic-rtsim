package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.AloftRules;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBodies;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin.EntityUmvuthiAccessor;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobSpawnType;

import java.util.List;

/**
 * The grove's chief as a conflict body (MZB-806): the jar's Umvuthi, turned to a quarter and floating.
 *
 * <p>Not {@code ConflictBodies.create}: the Umvuthi's {@code direction} is private and read only at its first tick
 * (yaw is set from it every tick), so a yaw alone would be turned back to whatever the constructor rolled. The body is
 * made with the synced direction of the yaw's quarter (so its first tick copies it), {@code finalizeSpawn} is called
 * as {@code create} does (it may turn the body toward a player holding the spawn egg, which an EVENT spawn does not,
 * and restricts it where it stands), and the quarter is set again after it, in the synced value, the private field and
 * the three yaws, so nothing the spawn did to it stays. It floats from the first tick ({@code noGravity}); the
 * {@code Traveller} takes it from there.
 */
final class UmvuthiMaker implements ConflictBodies.Maker {
    @Override
    @SuppressWarnings("deprecation") // finalizeSpawn is called directly, as ConflictBodies.create does
    public Mob make(ServerLevel level, double x, double y, double z, float yaw, List<Mob> extras) {
        EntityUmvuthi u = EntityHandler.UMVUTHI.get().create(level);
        if (u == null)
            return null;
        int quarter = AloftRules.quarter(yaw);
        u.moveTo(x, y, z, AloftRules.quarterYaw(quarter), 0f);
        u.setDirection(quarter);
        u.finalizeSpawn(level, level.getCurrentDifficultyAt(BlockPos.containing(x, y, z)), MobSpawnType.EVENT, null);
        face(u, quarter);
        u.setNoGravity(true);
        return u;
    }

    /** Turns the body to a quarter in every place the jar keeps its facing (MZB-813). */
    static void face(EntityUmvuthi u, int quarter) {
        u.setDirection(quarter);
        ((EntityUmvuthiAccessor) u).mic_rtsim_mowziesmobs$setDirection(quarter);
        float yaw = AloftRules.quarterYaw(quarter);
        u.setYRot(yaw);
        u.setYBodyRot(yaw);
        u.setYHeadRot(yaw);
    }
}
