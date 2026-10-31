package com.minecraftindustrialcomplex.mic_rtsim.growth;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.api.Arrivals;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.village.Census;
import com.minecraftindustrialcomplex.mic_rtsim.village.Residents;
import com.minecraftindustrialcomplex.mic_rtsim.village.Walkers;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import com.minecraftindustrialcomplex.mic_rtsim.village.VillageRegistry;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.phys.Vec3;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.function.IntSupplier;
import java.util.function.IntUnaryOperator;

/**
 * A new resident for a village (SPAWN_RESIDENT, C7 growth): made by the factory a plugin registered for
 * the body kind ({@code mic_rtsim_ribbits} registers {@code ribbits:ribbit}), at the resident slot of a
 * house the sim placed or at a given position. Residents are ordinary entities saved by vanilla (C2),
 * so exactly-once rests on the op id: the journal while it remembers the result, and after that the
 * spawn chunk and the resident itself, which both carry the COMMAND_RESULT until the engine has it, and
 * the journal's next file waits for both (the resident is noted by holder, as a camp body is).
 *
 * <p>The resident's entity section is written apart from its chunk, so it can be on disk while the spawn
 * chunk and the journal are from before (docs/choices-journal-barrier.md RP-212). An order, resent or kept
 * in the villages, then waits until the entities around the spawn point are loaded: a resident that loads
 * reasserts its record, and the order is answered DUPLICATE_OP in that tick, whatever its next try, since
 * the journal forgets the op again once the engine acknowledges the copy. As a backstop the resident's
 * UUID is a function of the engine's life and the op, so one the level still knows (hidden too) answers
 * DUPLICATE_OP, and one that loads later than its second is refused by vanilla as a known UUID: a lost
 * resident, never two. Each spawn counts against the per-tick spawn quota.
 */
public final class ResidentSpawner {
    /** Ticks between tries while the house or the chunk is not ready (PLACEHOLDER). */
    public static final int RETRY_TICKS = 40;

    private final RtsimSession session;
    private final VillageRegistry villages;
    private final HouseBuilder houses;
    long spawned;

    public ResidentSpawner(RtsimSession session, VillageRegistry villages, HouseBuilder houses) {
        this.session = session;
        this.villages = villages;
        this.houses = houses;
    }

    public long spawned() {
        return spawned;
    }

    public void accept(VillageRegistry.ResidentOrder o) {
        if (o.since == 0)
            o.since = session.gameTime();
        villages.residentOrders.put(o.op, o);
        villages.setDirty();
    }

    /**
     * Spawns what is ready. {@code budget} reads the spawn quota left this tick and {@code spend}
     * takes from it.
     */
    public void tick(long now, IntSupplier budget, IntUnaryOperator spend) {
        if (villages.residentOrders.isEmpty())
            return;
        for (VillageRegistry.ResidentOrder o : new ArrayList<>(villages.residentOrders.values())) {
            // Every tick, not at the next try: a resident that loaded this tick reasserted its record before this
            // runs, and the journal forgets the op again once the engine acknowledges the copy (RP-212).
            if (session.boundary().journal().commandOpApplied(o.op)) {
                finish(o, Obs.CommandResult.DUPLICATE_OP, null, null, now);
                continue;
            }
            if (now < o.nextTry)
                continue;
            if (budget.getAsInt() <= 0)
                return;
            try {
                step(o, now, spend);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] resident {} failed", o.op, ex);
                finish(o, Obs.CommandResult.FAILED, null, null, now);
            }
        }
    }

    /** An arrival order the watched rule has held this long is dropped, as the engine releases its booking (EQC174). */
    public static final long ARRIVAL_MAX_WAIT = 192L * 24_000L;
    /** How long a placement waits for the village's whole reach to load before it places without the bed count (EQC179). */
    static final long COMPLETE_WAIT = 600;
    /** Distances from the meeting point a walk-in's spawn is tried at, farthest first (EQC177). */
    public static final int[] WALK_DISTANCES = {176, 160, 144, 128, 112, 96};

    /** When each held arrival first found its village's reach not loaded (this life only). */
    private final java.util.Map<Long, Long> incompleteSince = new java.util.HashMap<>();

    private void step(VillageRegistry.ResidentOrder o, long now, IntUnaryOperator spend) {
        ServerLevel level = session.server().getLevel(o.dimension);
        if (level == null) {
            finish(o, Obs.CommandResult.FAILED, null, null, now);
            return;
        }
        boolean arrival = (o.flags & Cmd.SpawnResident.FLAG_ARRIVAL) != 0;
        Village arriving = null;
        if (arrival) {
            arriving = villages.get(o.village);
            if (arriving == null) {
                MicRtsim.LOGGER.warn("[mic_rtsim] resident {}: no village {}", o.op, o.village);
                finish(o, Obs.CommandResult.FAILED, null, null, now);
                return;
            }
            if (o.since != 0 && now - o.since > ARRIVAL_MAX_WAIT) {
                finish(o, Obs.CommandResult.FAILED, null, null, now);
                return;
            }
            // Never into a watched village (EQC174): it waits, or walks in from the edge (EQC177).
            if (Arrivals.watched(level, arriving.box, Arrivals.WATCH_BLOCKS)) {
                if ((o.flags & Cmd.SpawnResident.FLAG_WALK_IN) == 0) {
                    o.nextTry = now + RETRY_TICKS;
                    return;
                }
                walkIn(level, arriving, o, now, spend);
                return;
            }
        }
        Vec3 at = new Vec3(o.x, o.y, o.z);
        if ((o.flags & Cmd.SpawnResident.FLAG_AT_HOUSE) != 0) {
            Village owner = villages.houseOwner(o.house);
            Village.House house = owner == null ? null : owner.house(o.house);
            if (house == null) {
                if (houses.building(o.house)) {
                    o.nextTry = now + RETRY_TICKS;
                    return;
                }
                MicRtsim.LOGGER.warn("[mic_rtsim] resident {}: no house {}", o.op, o.house);
                finish(o, Obs.CommandResult.FAILED, null, null, now);
                return;
            }
            at = Vec3.atBottomCenterOf(house.slot());
        } else if ((o.flags & Cmd.SpawnResident.FLAG_AT_VILLAGE) != 0) {
            Village v = villages.get(o.village);
            if (v == null) {
                MicRtsim.LOGGER.warn("[mic_rtsim] resident {}: no village {}", o.op, o.village);
                finish(o, Obs.CommandResult.FAILED, null, null, now);
                return;
            }
            at = Vec3.atBottomCenterOf(v.meeting);
        }
        spawnAt(level, arriving, o, at, (o.flags & Cmd.SpawnResident.FLAG_AT_VILLAGE) != 0, false, now, spend);
    }

    /**
     * A watched village's arrival comes from the edge (EQC177): the first point on the ray from the meeting point toward the
     * order's source, 176 down to 96 blocks out, that is entity-ticking, standable and at least 128 blocks from every player.
     * It walks in ({@link Walkers}); with no such point the order waits.
     */
    private void walkIn(ServerLevel level, Village v, VillageRegistry.ResidentOrder o, long now, IntUnaryOperator spend) {
        BlockPos spot = level.isPositionEntityTicking(v.meeting)
                ? Arrivals.spotOnRay(level, v.meeting, o.x, o.z, WALK_DISTANCES, Arrivals.WATCH_BLOCKS) : null;
        if (spot == null) {
            o.nextTry = now + RETRY_TICKS;
            return;
        }
        spawnAt(level, v, o, Vec3.atBottomCenterOf(spot), false, true, now, spend);
    }

    /**
     * The tail every placement shares: the chunk, the exactly-once guards, the bed clamp of a human arrival, the factory and
     * the answer. {@code village} is non-null for an arrival; {@code walker} registers the body for {@link Walkers}.
     */
    private void spawnAt(ServerLevel level, Village village, VillageRegistry.ResidentOrder o, Vec3 at, boolean standNear,
                         boolean walker, long now, IntUnaryOperator spend) {
        BlockPos pos = BlockPos.containing(at);
        LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        if (chunk == null || !level.isPositionEntityTicking(pos)) {
            o.nextTry = now + RETRY_TICKS;
            return;
        }
        if (RtsimSession.chunkCarriesOp(chunk, o.op)) {
            adoptStray(level, village, o, now);
            finish(o, Obs.CommandResult.DUPLICATE_OP, chunk, null, now);
            return;
        }
        // A resident saved ahead of the spawn chunk loads with its entity section, which can come after the chunk
        // is entity-ticking; it may stand in any chunk standNear reaches.
        if (!entitiesLoadedAround(level, pos)) {
            o.nextTry = now + RETRY_TICKS;
            return;
        }
        java.util.UUID id = residentId(o.op);
        if (((com.minecraftindustrialcomplex.mic_rtsim.mixin.ServerLevelAccessor) level).mic_rtsim$entityManager().isLoaded(id)) {
            adoptStray(level, village, o, now);
            finish(o, Obs.CommandResult.DUPLICATE_OP, chunk, null, now);
            return;
        }
        // A human arrival never takes more than the free beds (EQC179): vanilla breeds only into free beds. The count is
        // fresh, and only a complete one clamps; a village whose reach is still loading is waited for, a little.
        if (village != null && "minecraft:villager".equals(o.body)) {
            Census.Scan scan = Census.scan(villages, level, village);
            if (!scan.complete()) {
                long first = incompleteSince.computeIfAbsent(o.op, k -> now);
                if (now - first < COMPLETE_WAIT) {
                    o.nextTry = now + RETRY_TICKS;
                    return;
                }
            } else if (scan.beds() - scan.residents() <= 0) {
                incompleteSince.remove(o.op);
                finish(o, Obs.CommandResult.NOT_FREE, null, null, now);
                return;
            }
        }
        incompleteSince.remove(o.op);
        String profession = o.profession;
        if (profession == null && village != null)
            profession = Residents.chooseProfession(o.body, level, villages, village, o.op);
        Residents.Factory factory = Residents.factory(o.body);
        Entity e = factory == null ? null : factory.create(level, at, profession);
        if (e == null) {
            MicRtsim.LOGGER.warn("[mic_rtsim] resident {}: no factory made a {}", o.op, o.body);
            finish(o, Obs.CommandResult.FAILED, null, null, now);
            return;
        }
        if (standNear)
            standNear(level, e, pos);
        // A settler from the edge is made 96 to 176 blocks out, where a Ribbit takes its home from: home it at the village, or
        // its go-home goal walks it back out every night (EQC330).
        if (walker && village != null)
            Residents.rehome(e, village.meeting);
        if (e instanceof Mob mob)
            mob.setPersistenceRequired();
        e.setUUID(id);
        if (!level.addFreshEntity(e)) {
            finish(o, Obs.CommandResult.FAILED, null, null, now);
            return;
        }
        spend.applyAsInt(1);
        spawned++;
        if (walker && village != null)
            registerWalker(level, village, o, id, now);
        finish(o, Obs.CommandResult.APPLIED, chunk, e, now);
    }

    /** Enters a settler in the registry's walkers ({@link Walkers}, EQC178). */
    private void registerWalker(ServerLevel level, Village village, VillageRegistry.ResidentOrder o, java.util.UUID id, long now) {
        VillageRegistry.Walker w = new VillageRegistry.Walker();
        w.id = id;
        w.village = village.id;
        w.dimension = level.dimension();
        w.sx = o.x;
        w.sz = o.z;
        w.spawnedAt = now;
        w.deadline = now + Walkers.WALK_TIMEOUT_TICKS;
        villages.walkers.put(id, w);
        villages.setDirty();
    }

    /**
     * A resent walk-in order the guards answer DUPLICATE_OP is for a settler that already exists, and one saved ahead of the
     * villages' file (a crash between its chunk's write and the file's) is on no list: it would stand out on the road, counted
     * by nobody and never walked in. It is entered now, if nothing lists it, and the walkers' pass finds it where it stands, or
     * waits for its chunk (EQC332). Only a duplicate the spawn guards found: one the journal answers has its walker in the file,
     * which is written before the journal's.
     */
    private void adoptStray(ServerLevel level, Village village, VillageRegistry.ResidentOrder o, long now) {
        if (village == null || (o.flags & Cmd.SpawnResident.FLAG_WALK_IN) == 0)
            return;
        java.util.UUID id = residentId(o.op);
        if (!villages.walkers.containsKey(id))
            registerWalker(level, village, o, id, now);
    }

    /** How far {@link #standNear} may move a resident from its spawn point, in blocks along x or z. */
    static final int REACH = 5;

    /** Whether the entities of every chunk within {@link #REACH} of {@code pos} are loaded. */
    static boolean entitiesLoadedAround(ServerLevel level, BlockPos pos) {
        for (int cx = (pos.getX() - REACH) >> 4; cx <= (pos.getX() + REACH) >> 4; cx++)
            for (int cz = (pos.getZ() - REACH) >> 4; cz <= (pos.getZ() + REACH) >> 4; cz++)
                if (!level.areEntitiesLoaded(net.minecraft.world.level.ChunkPos.asLong(cx, cz)))
                    return false;
        return true;
    }

    /**
     * The resident's UUID: one per op in an engine's life (RP-212), so a second spawn of the op meets the first
     * as a known UUID. A new engine life numbers its ops afresh, and its residents get other UUIDs.
     */
    java.util.UUID residentId(long op) {
        return java.util.UUID.nameUUIDFromBytes(("mic_rtsim:resident:" + session.engineLife() + ":" + op)
                .getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }

    /** Around a meeting point: the ground of each column, nearest first (PLACEHOLDER offsets). */
    private static final int[][] AROUND = {
            {2, 0}, {-2, 0}, {0, 2}, {0, -2}, {3, 3}, {-3, 3}, {3, -3}, {-3, -3}, {5, 0}, {-5, 0}, {0, 5}, {0, -5}};

    /** Heights tried in each column, nearest the meeting point's first (a roof or a canopy above is never the ground). */
    private static final int[] DY = {0, 1, -1, 2, -2, 3, -3, 4, -4, 5, -5, 6, -6};

    /**
     * Stands {@code e} on the ground beside the meeting point {@code meet} (a bell has a post or a
     * roof over it): in the first of a few nearby columns, the height nearest the meeting point's,
     * within 6 blocks, where the block below holds it up, the body fits and stands in no liquid;
     * else at the meeting point itself.
     */
    public static void standNear(ServerLevel level, Entity e, BlockPos meet) {
        for (int[] d : AROUND) {
            BlockPos c = meet.offset(d[0], 0, d[1]);
            if (!level.isLoaded(c))
                continue;
            for (int dy : DY) {
                BlockPos at = c.above(dy);
                BlockPos below = at.below();
                if (!level.getBlockState(below).isFaceSturdy(level, below, net.minecraft.core.Direction.UP))
                    continue;
                e.moveTo(at.getX() + 0.5, at.getY(), at.getZ() + 0.5, e.getYRot(), e.getXRot());
                if (level.noCollision(e) && !level.containsAnyLiquid(e.getBoundingBox()))
                    return;
            }
        }
        e.moveTo(meet.getX() + 0.5, meet.getY(), meet.getZ() + 0.5, e.getYRot(), e.getXRot());
    }

    private void finish(VillageRegistry.ResidentOrder o, int status, LevelChunk chunk, Entity resident, long now) {
        villages.residentOrders.remove(o.op);
        villages.setDirty();
        ByteBuffer r = session.commandResult(o.cmdSeq, o.op, status, now);
        if (status == Obs.CommandResult.APPLIED && chunk != null) {
            session.carryResultInChunk(chunk, r);
            if (resident != null)
                session.carryOnHolder(resident, List.of(r.array()), r.getLong(Obs.OBS_SEQ));
        }
    }
}
