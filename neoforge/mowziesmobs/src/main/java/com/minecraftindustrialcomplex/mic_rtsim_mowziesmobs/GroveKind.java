package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthana;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampKind;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampMember;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import java.util.UUID;
import java.util.function.Predicate;

/**
 * The Umvuthana's grove (MOWZIE.md §4.5, §4.9): its warriors are the jar's grove warriors
 * ({@link EntityUmvuthanaMinion}, {@code mowziesmobs:umvuthana}), restricted 25 blocks round the fire pit's centre,
 * and its chief is the jar's {@link EntityUmvuthi}, a body of the camp's chief slot (MZB-402, MZB-434): made on its
 * throne with the direction the engine kept, restricted as worldgen does, and its trade memory written without
 * journaling anything. Rule U (MZB-433) holds the chief's removal while a player is near or fights it, and a chief
 * SPAWN or SEAT_HEIR while a player is near its throne.
 */
public final class GroveKind implements CampKind {
    public static final GroveKind INSTANCE = new GroveKind();

    /** A warrior is restricted this far round the centre, as the jar's own ({@code UmvuthanaGrovePieces.java:507}). */
    public static final int RESTRICT = 25;
    /** A spawned warrior appears this far from the centre, on the side away from the nearest player (§4.9). */
    public static final double EDGE = 20.0;
    /** The players a spawn turns away from: within this of the centre (a warrior's tracking range is 128). */
    public static final double SEEN_RANGE = 256.0;
    /** A player this near the throne holds a chief's spawn or seating, and this near the chief holds its removal (MZB-433). */
    public static final double HOLD_RANGE = 160.0;

    private GroveKind() {}

    /** A grove warrior, exactly (not the followers, cranes or raptors that share its base class), or the Umvuthi (MZB-402). */
    @Override
    public boolean isBody(Entity e) {
        return e.getType() == EntityHandler.UMVUTHANA_MINION.get() || e instanceof EntityUmvuthi;
    }

    @Override
    public String bodyName(Entity e) {
        return e instanceof EntityUmvuthi ? MicRtsimMowziesMobs.UMVUTHI : MicRtsimMowziesMobs.WARRIOR;
    }

    @Override
    public boolean isChief(Entity e) {
        return e instanceof EntityUmvuthi;
    }

    /** The Umvuthi is the chief slot's, outside the pool (MZB-401): FACTION_FOUND's home bodies are the warriors. */
    @Override
    public boolean pooled(Entity e) {
        return !(e instanceof EntityUmvuthi);
    }

    @Override
    public boolean chiefLines() {
        return true;
    }

    /**
     * The Umvuthi standing in the footprint (not a puppet, not a second body): present, on its throne, its health,
     * direction and trade memory (MZB-434). A camp's own chief body is preferred to an untagged one; the memory
     * is booked to the slot and generation of its tag (an untagged one is slot 0, which the engine does not union).
     */
    @Override
    public Chief chief(ServerLevel level, Camps.Site site, List<Entity> footprint) {
        EntityUmvuthi best = null;
        for (Entity e : footprint) {
            if (!(e instanceof EntityUmvuthi u) || !u.isAlive() || RtsimAttachments.isTagged(u) || RtsimApi.secondChiefBody(u))
                continue;
            if (best == null || (RtsimAttachments.campMember(best) == null && RtsimAttachments.campMember(u) != null))
                best = u;
        }
        if (best == null)
            return Chief.NONE;
        CampMember m = RtsimAttachments.campMember(best);
        List<UUID> traded = new ArrayList<>(best.getTradedPlayers());
        traded.sort(Comparator.comparingLong(UUID::getMostSignificantBits).thenComparingLong(UUID::getLeastSignificantBits));
        return new Chief(ConflictRecords.CampCensus.CHIEF | ConflictRecords.CampCensus.THRONE, best.getHealth(), best.getX(),
                best.getY(), best.getZ(), best.getDirectionData(), m == null ? 0 : m.slot(), m == null ? 0 : m.generation(), traded);
    }

    /**
     * As the fire pit's own: made, {@code finalizeSpawn} with STRUCTURE called directly as the jar's worldgen does
     * (a warrior's weapon rolled there, its mask in its constructor; MZB-219). The Umvuthi (MZB-434): its
     * direction set before it joins the level (the jar reads it only at its first tick), then
     * {@code finalizeSpawn(STRUCTURE)}, which does not restrict, and {@code restrictTo(home, -1)} as worldgen does.
     * Its trade memory is {@link #chiefBody}'s, written by the reconciler.
     */
    @Override
    @SuppressWarnings("deprecation")
    public Mob make(ServerLevel level, Camps.Site site, String body, int lineFlags, double x, double y, double z, float yaw) {
        if (MicRtsimMowziesMobs.UMVUTHI.equals(body)) {
            EntityUmvuthi u = EntityHandler.UMVUTHI.get().create(level);
            if (u == null)
                return null;
            u.moveTo(x, y, z, yaw, 0f);
            u.setDirection(site.throneDirection());
            u.finalizeSpawn(level, level.getCurrentDifficultyAt(u.blockPosition()), MobSpawnType.STRUCTURE, null);
            u.restrictTo(site.hasThrone() ? BlockPos.containing(site.throne()) : u.blockPosition(), -1);
            return u;
        }
        if (!MicRtsimMowziesMobs.WARRIOR.equals(body))
            return null;
        EntityUmvuthanaMinion m = EntityHandler.UMVUTHANA_MINION.get().create(level);
        if (m == null)
            return null;
        m.moveTo(x, y, z, yaw, 0f);
        m.setYHeadRot(yaw);
        m.finalizeSpawn(level, level.getCurrentDifficultyAt(m.blockPosition()), MobSpawnType.STRUCTURE, null);
        return m;
    }

    /** The places tried for one spawn, as the jar's fire pit does (MZB-243). */
    public static final int SPAWN_TRIES = 20;

    /**
     * A warrior: at the grove's edge on the side away from the nearest player within {@link #SEEN_RANGE}, so it
     * walks in (§4.9, "Coming home"); with none, where the fire pit places its own (2 to 11 blocks out). Up
     * to {@link #SPAWN_TRIES} places are tried, as the jar does: each on loaded ground, free of collisions
     * and of fluid for the warrior's box (the later tries jitter round the first). Null when none passes:
     * the line is answered stale. The Umvuthi: its throne, else the fire pit's centre on the ground there
     * (MZB-404), the later tries jittering within three blocks.
     */
    @Override
    public Vec3 spawnAt(ServerLevel level, Camps.Site site, String body, int lineFlags) {
        if (MicRtsimMowziesMobs.UMVUTHI.equals(body))
            return throneAt(level, site);
        Vec3 c = site.spot;
        Player near = null;
        double nd = SEEN_RANGE * SEEN_RANGE;
        for (Player p : level.players()) {
            if (p.isSpectator())
                continue;
            double d = p.distanceToSqr(c);
            if (d < nd) {
                nd = d;
                near = p;
            }
        }
        boolean away = near != null && (near.getX() != c.x || near.getZ() != c.z);
        double base = away ? Math.atan2(c.z - near.getZ(), c.x - near.getX()) : 0; // away from the player
        EntityType<?> type = EntityHandler.UMVUTHANA_MINION.get();
        for (int i = 0; i < SPAWN_TRIES; i++) {
            double angle, dist;
            if (away) {
                angle = i == 0 ? base : base + (level.random.nextDouble() - 0.5) * (Math.PI / 3);
                dist = i == 0 ? EDGE : EDGE - level.random.nextDouble() * 4;
            } else {
                angle = level.random.nextDouble() * Math.PI * 2;
                dist = 2 + level.random.nextInt(10);
            }
            int x = (int) Math.floor(c.x + Math.cos(angle) * dist), z = (int) Math.floor(c.z + Math.sin(angle) * dist);
            if (!level.hasChunk(x >> 4, z >> 4))
                continue;
            int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, x, z);
            if (y <= level.getMinBuildHeight())
                continue;
            AABB box = type.getDimensions().makeBoundingBox(x + 0.5, y, z + 0.5);
            if (level.noCollision(box) && !level.containsAnyLiquid(box))
                return new Vec3(x + 0.5, y, z + 0.5);
        }
        return null;
    }

    /** The Umvuthi's place: the throne the engine kept as it is, else the ground at the fire pit's centre; jittered when blocked. */
    private static Vec3 throneAt(ServerLevel level, Camps.Site site) {
        Vec3 t = site.throne();
        EntityType<?> type = EntityHandler.UMVUTHI.get();
        for (int i = 0; i < SPAWN_TRIES; i++) {
            double x = t.x, z = t.z, y = t.y;
            if (i > 0) {
                x += (level.random.nextDouble() - 0.5) * 6;
                z += (level.random.nextDouble() - 0.5) * 6;
            }
            if (!level.hasChunk(Math.floorDiv((int) Math.floor(x), 16), Math.floorDiv((int) Math.floor(z), 16)))
                continue;
            if (i > 0 || !site.hasThrone()) {
                y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, (int) Math.floor(x), (int) Math.floor(z));
                if (y <= level.getMinBuildHeight())
                    continue;
            }
            AABB box = type.getDimensions().makeBoundingBox(x, y, z);
            if (level.noCollision(box) && !level.containsAnyLiquid(box))
                return new Vec3(x, y, z);
        }
        return null;
    }

    /**
     * A chief SPAWN, SEAT_HEIR and the MASK that goes with it wait while a non-spectator player is within
     * {@link #HOLD_RANGE} of the throne (MZB-433); a warrior's spawn never waits.
     */
    @Override
    public boolean holdSpawn(ServerLevel level, Camps.Site site, String body, int lineFlags) {
        if (!MicRtsimMowziesMobs.UMVUTHI.equals(body) && (lineFlags & ConflictRecords.CampReconcile.LINE_CHIEF) == 0)
            return false;
        return playerWithin(level, site.throne(), HOLD_RANGE);
    }

    /** Whether a non-spectator player of the level stands within {@code range} of {@code at}. */
    public static boolean playerWithin(ServerLevel level, Vec3 at, double range) {
        double r2 = range * range;
        for (Player p : level.players())
            if (!p.isSpectator() && p.distanceToSqr(at) <= r2)
                return true;
        return false;
    }

    /**
     * Rule U (MZB-433): the Umvuthi's removal waits while a non-spectator player is within {@link #HOLD_RANGE} of it
     * or it has a player as its target. A warrior goes at once.
     */
    @Override
    public boolean holdRemoval(ServerLevel level, Camps.Site site, Mob body) {
        if (!(body instanceof EntityUmvuthi))
            return false;
        return body.getTarget() instanceof Player || playerWithin(level, body.position(), HOLD_RANGE);
    }

    /** A fight a player began (MZB-410): the Umvuthi has a player as its target and a player hit it lately. */
    @Override
    public boolean inFight(ServerLevel level, Mob body) {
        return body instanceof EntityUmvuthi && body.getTarget() instanceof Player p && !p.isSpectator()
                && FightWatch.recent(body, level.getGameTime());
    }

    /**
     * The engine's trade memory onto the chief body (MZB-421): unioned with what the body has, as
     * {@code setTradedPlayersCompound} (a ListTag of {@code NbtUtils.createUUID}), never {@code rememberTrade}, so
     * nothing is journaled. A health above zero is the engine's (a puppet that came home), capped at the body's maximum.
     */
    @Override
    public void chiefBody(Mob body, ChiefBody memory) {
        if (!(body instanceof EntityUmvuthi u))
            return;
        Set<UUID> all = new LinkedHashSet<>(u.getTradedPlayers());
        all.addAll(memory.traded());
        ListTag list = new ListTag();
        for (UUID id : all)
            list.add(NbtUtils.createUUID(id));
        u.setTradedPlayersCompound(list);
        if (memory.health() > 0f)
            u.setHealth(Math.min(memory.health(), u.getMaxHealth()));
    }

    /**
     * Restricted round the centre (the jar's own restore it from their save; made and adopted ones are set here). The
     * Umvuthi keeps the home the jar saved; one with none (never set) gets its place (restricted with radius -1, as the
     * jar's own).
     */
    @Override
    public void joined(Camps.Site site, Mob body) {
        if (body instanceof EntityUmvuthi) {
            if (BlockPos.ZERO.equals(body.getRestrictCenter()))
                body.restrictTo(body.blockPosition(), -1);
            return;
        }
        body.restrictTo(BlockPos.containing(site.spot), RESTRICT);
    }

    @Override
    public void copy(Mob from, Mob to) {
        to.setHealth(Math.min(from.getHealth(), to.getMaxHealth()));
        if (from instanceof EntityUmvuthana a && to instanceof EntityUmvuthana b) {
            b.setMask(a.getMaskType());
            b.setWeapon(a.getWeapon());
        }
    }

    /** The grove's kin: every Umvuthana and the Umvuthi (the killing signal). */
    @Override
    public Predicate<Entity> kin() {
        return e -> e instanceof EntityUmvuthana || e instanceof EntityUmvuthi;
    }

    /** The grove's notices' sound: the jar's gong ({@code mowziesmobs:block.gong}), else a direct event of that name. */
    @Override
    public Holder<SoundEvent> sound() {
        ResourceLocation id = ResourceLocation.fromNamespaceAndPath("mowziesmobs", "block.gong");
        Holder<SoundEvent> known = BuiltInRegistries.SOUND_EVENT.getHolder(id).orElse(null);
        return known != null ? known : Holder.direct(SoundEvent.createVariableRangeEvent(id));
    }
}
