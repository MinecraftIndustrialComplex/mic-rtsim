package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.SessionPlugin.EngineNames;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.OwnableEntity;
import net.minecraft.world.entity.ai.attributes.AttributeInstance;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.animal.Animal;
import net.minecraft.world.entity.animal.horse.AbstractHorse;
import net.minecraft.world.entity.TamableAnimal;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.AABB;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * The animals a raid on an unwatched district owes (MZB-424 to MZB-426, MZB-442, MZB-449; MOWZIE.md §4.5 and ledger
 * 11; docs/boundary.md §9). A raptor pack that wins a raid nobody watched leaves the district dead animals, and the
 * engine sends the debt as an OWED_HUNT; this kills them where they stand once the district next loads and answers
 * with HUNT_APPLIED.
 *
 * <p>Vanilla only. Held in memory (never saved): a hunt the engine has not had answered goes again at the next
 * session start, and a session start clears everything here (the last op applied included).
 *
 * <p>For each faction and district it keeps the latest OWED_HUNT until it applies it, and the last op it applied
 * (the engine's op, in MZB-449's rule). A command naming the last op applied again is ignored: the hunt was
 * answered once and is still answered once. When the hunt is applied, one whose {@code replaced} is the last op
 * applied kills {@code rest} animals (the replaced hunt's have died already), and any other kills {@code count}.
 * Applying a hunt sets the last op to its op. A hunt of nothing to kill (a count of 0, a rest of 0, a
 * dimension that does not exist, or a radius over {@value #MAX_RADIUS}) is applied at once and answered all the same.
 *
 * <p>A hunt waits for its district to load: every chunk the circle touches must have its entities loaded and
 * ticking. Then it kills up to its amount, the animals nearest the centre first, with {@code kill()} so that their
 * drops fall, among those the jar's raptor would hunt (no attack damage or under 3, and 0.1 &lt; width&sup2; &times;
 * height &lt; 6, {@code EntityUmvuthana.registerHuntingTargetGoals}) that nobody has marked as theirs (MZB-442: no
 * tamed, named, leashed or mounted animal). It journals HUNT_APPLIED with the number killed, even when that is 0,
 * and the journal's next file waits for the circle's entity chunks to be written ({@code ChunkWriteBarrier},
 * ledger 22), so a restart cannot see the answer without the deaths.
 */
public final class OwedHunts {
    /** How often the held hunts look at their chunks, in ticks. */
    static final int CHECK_INTERVAL = 20;
    /** The widest circle a hunt may name (the engine's is 48): a wider one is malformed and kills nothing. */
    static final int MAX_RADIUS = 128;

    private record Key(long faction, long district) {}

    /** One OWED_HUNT, as received. {@code dim} is null for a dimension the server has not got. */
    record Hunt(long faction, long district, long op, long replaced, ResourceKey<Level> dim, int count, int centreX,
                int centreZ, int radius, int rest) {
        Key key() {
            return new Key(faction, district);
        }
    }

    /** The latest OWED_HUNT of each faction and district, until it is applied. */
    private final Map<Key, Hunt> held = new LinkedHashMap<>();
    /** The op last applied for each faction and district. */
    private final Map<Key, Long> applied = new HashMap<>();

    /** This session's counters (GameTests). */
    public long received, ignored, appliedHunts, killed;
    /** A hunt arrived since the last look: the next tick looks at once. */
    private boolean fresh;

    /** A session started, stopped or failed, or the module went off: nothing of the last survives. */
    void clear() {
        held.clear();
        applied.clear();
        fresh = false;
        received = ignored = appliedHunts = killed = 0;
    }

    /** The hunts held, not yet applied (GameTests). */
    public int heldCount() {
        return held.size();
    }

    /** The op last applied for a faction and district, or 0 (GameTests). */
    public long lastApplied(long faction, long district) {
        return applied.getOrDefault(new Key(faction, district), 0L);
    }

    // ---- the command ----------------------------------------------------------------------------

    /** OWED_HUNT: kept until applied, unless it names the op last applied. */
    void owed(ByteBuffer b, int at, EngineNames names) {
        received++;
        long op = b.getLong(at + ConflictRecords.OwedHunt.OP_ID);
        String dimName = names.name(Obs.DICT_DIMENSION, b.getInt(at + ConflictRecords.OwedHunt.DIM));
        ResourceLocation loc = dimName == null ? null : ResourceLocation.tryParse(dimName);
        Hunt h = new Hunt(b.getLong(at + ConflictRecords.OwedHunt.FACTION), b.getLong(at + ConflictRecords.OwedHunt.DISTRICT), op,
                b.getLong(at + ConflictRecords.OwedHunt.REPLACED), loc == null ? null : ResourceKey.create(Registries.DIMENSION, loc),
                b.getInt(at + ConflictRecords.OwedHunt.COUNT), b.getInt(at + ConflictRecords.OwedHunt.CENTRE_X),
                b.getInt(at + ConflictRecords.OwedHunt.CENTRE_Z), b.getInt(at + ConflictRecords.OwedHunt.RADIUS),
                b.getInt(at + ConflictRecords.OwedHunt.REST));
        Long last = applied.get(h.key());
        if (last != null && last == op) {
            ignored++; // the engine sent the rest of a hunt this session applied already: answered once is enough
            return;
        }
        held.put(h.key(), h);
        fresh = true;
    }

    // ---- applying -------------------------------------------------------------------------------

    /** Every {@value #CHECK_INTERVAL} ticks, and the tick after one arrives, while a hunt is held. */
    void tick(RtsimSession s, long now) {
        if (held.isEmpty() || !fresh && now % CHECK_INTERVAL != 0)
            return;
        fresh = false;
        for (Hunt h : List.copyOf(held.values())) {
            if (held.get(h.key()) != h)
                continue;
            try {
                tryApply(s, h, now);
            } catch (RuntimeException ex) {
                MicRtsim.LOGGER.error("[mic_rtsim] applying the owed hunt of {} in {} failed", Long.toHexString(h.faction()),
                        Long.toHexString(h.district()), ex);
                held.remove(h.key(), h); // not tried again this session; the engine sends it at the next
            }
        }
    }

    /** How many animals a hunt kills now: {@code rest} when it replaced the op last applied, else {@code count}. */
    int amount(Hunt h) {
        Long last = applied.get(h.key());
        return h.replaced() != 0 && last != null && last == h.replaced() ? h.rest() : h.count();
    }

    private void tryApply(RtsimSession s, Hunt h, long now) {
        int n = amount(h);
        ServerLevel level = h.dim() == null ? null : s.server().getLevel(h.dim());
        if (n <= 0 || level == null || h.radius() < 0 || h.radius() > MAX_RADIUS) {
            answer(s, h, 0, level, List.of(), now);
            return;
        }
        List<ChunkPos> chunks = circleChunks(h);
        for (ChunkPos c : chunks)
            if (!level.areEntitiesLoaded(c.toLong()) || !RtsimSession.entitiesTickAt(level, new BlockPos(c.getMiddleBlockX(), level.getSeaLevel(), c.getMiddleBlockZ())))
                return; // the district has not loaded yet
        double cx = h.centreX() + 0.5, cz = h.centreZ() + 0.5;
        double r2 = (double) h.radius() * h.radius();
        AABB box = new AABB(cx - h.radius(), level.getMinBuildHeight(), cz - h.radius(), cx + h.radius(), level.getMaxBuildHeight(), cz + h.radius());
        List<Animal> prey = new ArrayList<>(level.getEntitiesOfClass(Animal.class, box,
                a -> eligible(a) && dist2(a, cx, cz) <= r2));
        prey.sort(Comparator.<Animal>comparingDouble(a -> dist2(a, cx, cz)).thenComparing(a -> a.getUUID()));
        int dead = 0;
        try {
            for (Animal a : prey) {
                if (dead >= n)
                    break;
                try {
                    a.kill(); // drops fall (MZB-426); another mod may cancel it, which is no kill
                } finally {
                    if (!a.isAlive())
                        dead++; // one that died before a handler threw is dead all the same
                }
            }
        } finally {
            // Whatever a handler threw, the animals killed so far are dead for good: answer with their number, so that
            // the engine does not send the same hunt again at the next session start and kill that many twice (MZB-646).
            answer(s, h, dead, level, chunks, now);
        }
    }

    /** HUNT_APPLIED, journaled, and the hunt is spent. */
    private void answer(RtsimSession s, Hunt h, int dead, ServerLevel level, List<ChunkPos> chunks, long now) {
        java.nio.ByteBuffer r = s.boundary().beginJournaled(ConflictRecords.HUNT_APPLIED, ConflictRecords.HuntApplied.SIZE, 0, now);
        r.putLong(ConflictRecords.HuntApplied.FACTION, h.faction());
        r.putLong(ConflictRecords.HuntApplied.DISTRICT, h.district());
        r.putLong(ConflictRecords.HuntApplied.OP_ID, h.op());
        r.putInt(ConflictRecords.HuntApplied.KILLED, dead);
        s.boundary().commit(r);
        for (ChunkPos c : chunks)
            s.journal().chunks().dependsOnEntities(level, c);
        applied.put(h.key(), h.op());
        held.remove(h.key(), h);
        appliedHunts++;
        killed += dead;
    }

    private static double dist2(Animal a, double cx, double cz) {
        double dx = a.getX() - cx, dz = a.getZ() - cz;
        return dx * dx + dz * dz;
    }

    /** Every chunk the circle touches. */
    static List<ChunkPos> circleChunks(Hunt h) {
        double cx = h.centreX() + 0.5, cz = h.centreZ() + 0.5;
        double r = h.radius();
        int x0 = (int) Math.floor((cx - r)) >> 4, x1 = (int) Math.floor((cx + r)) >> 4;
        int z0 = (int) Math.floor((cz - r)) >> 4, z1 = (int) Math.floor((cz + r)) >> 4;
        List<ChunkPos> out = new ArrayList<>();
        for (int x = x0; x <= x1; x++) {
            for (int z = z0; z <= z1; z++) {
                // the nearest point of the chunk's square to the centre
                double nx = Math.max(x * 16.0, Math.min(cx, x * 16.0 + 16)), nz = Math.max(z * 16.0, Math.min(cz, z * 16.0 + 16));
                if ((nx - cx) * (nx - cx) + (nz - cz) * (nz - cz) <= r * r)
                    out.add(new ChunkPos(x, z));
            }
        }
        return out;
    }

    /**
     * Whether an owed hunt may kill an animal: the jar's raptor hunts it ({@code EntityUmvuthana.java:338-342}) and
     * nobody has marked it as theirs (MZB-442).
     */
    static boolean eligible(Animal a) {
        if (!a.isAlive() || a.isRemoved())
            return false;
        AttributeInstance damage = a.getAttribute(Attributes.ATTACK_DAMAGE);
        if (damage != null && damage.getValue() >= 3.0)
            return false;
        float volume = a.getBbWidth() * a.getBbWidth() * a.getBbHeight();
        if (!((double) volume > 0.1 && volume < 6.0f))
            return false;
        // mayBeLeashed, not isLeashed: a leashed animal that has just loaded has its leash data from the save but no
        // holder yet (Leashable.tickLeash restores it in the first ticks, and for up to 100 when the holder is an
        // entity not loaded yet), and isLeashed says false until then (MZB-644).
        if (a.hasCustomName() || a.mayBeLeashed() || a.isVehicle() || a.isPassenger())
            return false;
        if (a instanceof OwnableEntity o && o.getOwnerUUID() != null)
            return false;
        if (a instanceof TamableAnimal t && t.isTame())
            return false;
        return !(a instanceof AbstractHorse horse && horse.isTamed());
    }
}
