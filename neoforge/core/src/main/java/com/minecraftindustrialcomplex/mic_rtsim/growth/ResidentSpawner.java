package com.minecraftindustrialcomplex.mic_rtsim.growth;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Cmd;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.village.Residents;
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
 * spawn chunk and the resident itself, which both carry the COMMAND_RESULT until the engine has it.
 * Each spawn counts against the per-tick spawn quota.
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

    private void step(VillageRegistry.ResidentOrder o, long now, IntUnaryOperator spend) {
        ServerLevel level = session.server().getLevel(o.dimension);
        if (level == null) {
            finish(o, Obs.CommandResult.FAILED, null, null, now);
            return;
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
        }
        BlockPos pos = BlockPos.containing(at);
        LevelChunk chunk = level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4);
        if (chunk == null || !level.isPositionEntityTicking(pos)) {
            o.nextTry = now + RETRY_TICKS;
            return;
        }
        if (RtsimSession.chunkCarriesOp(chunk, o.op)) {
            finish(o, Obs.CommandResult.DUPLICATE_OP, chunk, null, now);
            return;
        }
        Residents.Factory factory = Residents.factory(o.body);
        Entity e = factory == null ? null : factory.create(level, at, o.profession);
        if (e == null) {
            MicRtsim.LOGGER.warn("[mic_rtsim] resident {}: no factory made a {}", o.op, o.body);
            finish(o, Obs.CommandResult.FAILED, null, null, now);
            return;
        }
        if (e instanceof Mob mob)
            mob.setPersistenceRequired();
        if (!level.addFreshEntity(e)) {
            finish(o, Obs.CommandResult.FAILED, null, null, now);
            return;
        }
        spend.applyAsInt(1);
        spawned++;
        finish(o, Obs.CommandResult.APPLIED, chunk, e, now);
    }

    private void finish(VillageRegistry.ResidentOrder o, int status, LevelChunk chunk, Entity resident, long now) {
        villages.residentOrders.remove(o.op);
        villages.setDirty();
        ByteBuffer r = session.commandResult(o.cmdSeq, o.op, status, now);
        if (status == Obs.CommandResult.APPLIED && chunk != null) {
            session.carryResultInChunk(chunk, r);
            if (resident != null)
                session.carryOnEntity(resident, List.of(r.array()), r.getLong(Obs.OBS_SEQ));
        }
    }
}
