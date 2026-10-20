package com.minecraftindustrialcomplex.mic_rtsim_ribbits;

import com.minecraftindustrialcomplex.mic_rtsim.body.BodyAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberCapture;
import com.minecraftindustrialcomplex.mic_rtsim.body.MemberSpawn;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim_ribbits.mixin.RibbitEntityAccessor;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.module.RibbitProfessionModule;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.phys.Vec3;

/**
 * {@code mic_rtsim_ribbits:ribbit_musician}: one ribbit of a touring band (C7, P16's recommendation).
 * Ribbits gives the music goal only to nitwits, and picks an instrument when the ribbit joins a band
 * (it is not in {@code RibbitData} beforehand), so a musician is a nitwit. The band's leader walks to
 * the engine's waypoints; the others follow it; at a stop they play around it as their master
 * ({@code RibbitPlayMusicGoalMixin}). Body bit A is the umbrella.
 */
public final class RibbitMusicianAdapter implements BodyAdapter {
    public static final String NAME = "mic_rtsim_ribbits:ribbit_musician";
    /** Followers walk back to the leader beyond this distance (PLACEHOLDER). */
    public static final double FOLLOW = 6;

    @Override
    public String name() {
        return NAME;
    }

    @Override
    public Entity create(ServerLevel level, Vec3 pos, MemberSpawn spawn) {
        RibbitEntity r = Ribbits.create(level, pos, RibbitProfessionModule.NITWIT, Ribbits.umbrella(spawn.bodyA()));
        if (r != null && spawn.health() > 0f)
            r.setHealth(Math.min(spawn.health(), r.getMaxHealth()));
        return r;
    }

    @Override
    public void capture(Entity entity, PuppetGroup group, MemberCapture out) {
        RibbitEntity r = (RibbitEntity) entity;
        if (group != null && group.members.length > 0 && group.members[0].uuid.equals(entity.getUUID()))
            RibbitBands.forget(group.agent); // the band goes back to the sim; its waypoint goes with its leader
        out.health = r.getHealth();
        out.maxHealth = r.getMaxHealth();
        out.x = r.getX();
        out.y = r.getY();
        out.z = r.getZ();
        if (r.getLeashHolder() != null)
            out.flags |= Obs.MemberState.FLAG_LEASHED_ELSEWHERE;
        out.bodyA = Ribbits.umbrellaIndex(r.getRibbitData().getUmbrellaType());
    }

    @Override
    public void steer(Entity entity, Vec3 target, float speed) {
        RibbitEntity r = (RibbitEntity) entity;
        ((RibbitEntityAccessor) r).mic_rtsim_ribbits$setHomePosition(BlockPos.containing(target));
        r.getNavigation().moveTo(target.x, target.y, target.z, speed > 0 ? speed : 1.0);
        RibbitBands.steered(r, target);
    }

    /** Followers keep up with the leader, and are homed where the leader is going. */
    @Override
    public void maintain(Entity entity, PuppetGroup group) {
        Entity leader = group.leader();
        if (leader == null || leader == entity || !(entity instanceof RibbitEntity r))
            return;
        Vec3 target = RibbitBands.target(group.agent);
        if (target != null)
            ((RibbitEntityAccessor) r).mic_rtsim_ribbits$setHomePosition(BlockPos.containing(target));
        if (!r.getPlayingInstrument() && r.distanceToSqr(leader) > FOLLOW * FOLLOW && r.getNavigation().isDone())
            r.getNavigation().moveTo(leader, 1.0);
    }

    @Override
    public int takenBy(Entity entity, PuppetGroup group) {
        RibbitEntity r = (RibbitEntity) entity;
        return r.getLeashHolder() instanceof Player p && p.isAlive() ? Obs.Taken.LEAD : 0;
    }
}
