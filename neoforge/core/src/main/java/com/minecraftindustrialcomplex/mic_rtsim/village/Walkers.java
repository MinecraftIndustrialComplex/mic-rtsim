package com.minecraftindustrialcomplex.mic_rtsim.village;

import com.minecraftindustrialcomplex.mic_rtsim.api.Arrivals;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.growth.ResidentSpawner;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.behavior.BlockPosTracker;
import net.minecraft.world.entity.ai.memory.MemoryModuleType;
import net.minecraft.world.entity.ai.memory.WalkTarget;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.level.levelgen.Heightmap;

import java.util.ArrayList;

/**
 * Settlers walking in from beyond the edge (EQUILIBRIUM.md §2.2's WalkIn, slice S6, EQC177 and EQC178). A watched village's
 * arrival is spawned 96 to 176 blocks out ({@link ResidentSpawner}) and registered in {@link VillageRegistry#walkers}; this
 * steers each every {@link #WALK_STEP_TICKS}, a hop of {@link #HOP_BLOCKS} at a time toward the meeting point, which vanilla's
 * {@code MoveToTargetSink} (a villager) or the navigation (a Ribbit) walks. The walk ends within {@link #DONE_BLOCKS} of the
 * meeting point, where the schedule, beds and job sites take over.
 *
 * <p>The sim said they arrived, and the walk is only how it looks: a walker nobody can see (no player within the watch radius
 * of it or of the village) is put at the meeting point at once, and so is one still walking at its deadline. A walker whose
 * chunk is not loaded stays until it is, and is forgotten a day after its deadline.
 */
public final class Walkers {
    /** Ticks between two steerings (PLACEHOLDER). */
    public static final int WALK_STEP_TICKS = 40;
    /** How far toward the meeting point one steering aims (PLACEHOLDER). */
    public static final int HOP_BLOCKS = 24;
    /** Within this many blocks of the meeting point a walk is done (PLACEHOLDER). */
    public static final int DONE_BLOCKS = 12;
    /** A walk lasts at most this long (PLACEHOLDER: 6,000 ticks, five minutes). */
    public static final long WALK_TIMEOUT_TICKS = 6_000;
    /** A walker never found is forgotten this long after its deadline. */
    static final long STRAY_TICKS = 24_000;

    private final RtsimSession session;
    private final VillageRegistry villages;
    long placed;
    long arrived;

    public Walkers(RtsimSession session, VillageRegistry villages) {
        this.session = session;
        this.villages = villages;
    }

    public long placed() {
        return placed;
    }

    public long arrived() {
        return arrived;
    }

    public void tick(long ticks, long now) {
        if (ticks % WALK_STEP_TICKS != 0 || villages.walkers.isEmpty())
            return;
        for (VillageRegistry.Walker w : new ArrayList<>(villages.walkers.values())) {
            Village v = villages.get(w.village);
            ServerLevel level = session.server().getLevel(w.dimension);
            if (v == null || level == null) {
                forget(w);
                continue;
            }
            Entity e = level.getEntity(w.id);
            if (e == null) {
                if (now - w.deadline > STRAY_TICKS)
                    forget(w);
                continue;
            }
            if (!e.isAlive()) {
                forget(w);
                continue;
            }
            if (Arrivals.horizontal(e, v.meeting) <= DONE_BLOCKS) {
                Residents.rehome(e, v.meeting);
                arrived++;
                forget(w);
                continue;
            }
            boolean unseen = !Arrivals.watched(level, e.blockPosition(), Arrivals.WATCH_BLOCKS)
                    && !Arrivals.watched(level, v.box, Arrivals.WATCH_BLOCKS);
            if (now >= w.deadline || unseen) {
                if (level.getChunkSource().getChunkNow(v.meeting.getX() >> 4, v.meeting.getZ() >> 4) == null)
                    continue; // nowhere loaded to put it: it waits for the meeting point's chunk
                if (e instanceof Mob m)
                    m.getNavigation().stop();
                ResidentSpawner.standNear(level, e, v.meeting);
                Residents.rehome(e, v.meeting);
                placed++;
                forget(w);
                continue;
            }
            steer(level, e, v.meeting);
        }
    }

    private void forget(VillageRegistry.Walker w) {
        villages.walkers.remove(w.id);
        villages.setDirty();
    }

    /** One hop toward the meeting point: a villager's walk target, a Ribbit's navigation. */
    private static void steer(ServerLevel level, Entity e, BlockPos meet) {
        double dx = meet.getX() + 0.5 - e.getX(), dz = meet.getZ() + 0.5 - e.getZ();
        double len = Math.hypot(dx, dz);
        if (len < 1e-6)
            return;
        double step = Math.min(len, HOP_BLOCKS);
        int tx = (int) Math.floor(e.getX() + dx / len * step), tz = (int) Math.floor(e.getZ() + dz / len * step);
        if (level.getChunkSource().getChunkNow(tx >> 4, tz >> 4) == null)
            return;
        BlockPos target = new BlockPos(tx, level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, tx, tz), tz);
        if (e instanceof Villager v)
            v.getBrain().setMemory(MemoryModuleType.WALK_TARGET, new WalkTarget(new BlockPosTracker(target), 0.5f, 2));
        else if (e instanceof Mob m)
            m.getNavigation().moveTo(target.getX() + 0.5, target.getY(), target.getZ() + 0.5, 0.5);
    }
}
