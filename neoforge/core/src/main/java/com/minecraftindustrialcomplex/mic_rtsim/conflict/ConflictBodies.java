package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.mixin.MobAccessor;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.ai.goal.WrappedGoal;
import net.minecraft.world.entity.raid.Raider;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * The bodies a conflict party is embodied as (D3: no bespoke entities): each abstract body kind stands in
 * as an entity type ({@link BodyKinds#standIn}, MZB-102), made as vanilla makes a mob for an event and
 * kept out of vanilla's raids.
 */
public final class ConflictBodies {
    private ConflictBodies() {}

    /** Whether an entity type makes a {@link Mob}, learnt once per type. */
    private static final Map<EntityType<?>, Boolean> MOB_TYPES = new HashMap<>();

    /**
     * The entity type a body kind is embodied as, or null when it cannot be (the name is an entity type
     * that is no mob). A name that is no registered entity type falls back to
     * {@link BodyKinds#STAND_IN_FALLBACK}.
     */
    public static EntityType<?> type(ServerLevel level, String body) {
        String name = BodyKinds.standIn(body);
        if (name == null)
            return null;
        ResourceLocation id = ResourceLocation.tryParse(name);
        EntityType<?> t = id == null ? null : BuiltInRegistries.ENTITY_TYPE.getOptional(id).orElse(null);
        if (t == null)
            t = BuiltInRegistries.ENTITY_TYPE.get(ResourceLocation.parse(BodyKinds.STAND_IN_FALLBACK));
        return isMob(level, t) ? t : null;
    }

    private static synchronized boolean isMob(ServerLevel level, EntityType<?> t) {
        Boolean known = MOB_TYPES.get(t);
        if (known != null)
            return known;
        Entity probe;
        try {
            probe = t.create(level);
        } catch (RuntimeException e) {
            probe = null;
        }
        boolean mob = probe instanceof Mob;
        if (probe != null)
            probe.discard();
        MOB_TYPES.put(t, mob);
        return mob;
    }

    /** True when a body of this type is a monster that Peaceful removes (not a friendly category). */
    public static boolean hostile(EntityType<?> t) {
        return !t.getCategory().isFriendly();
    }

    /**
     * Makes a body at (x, y, z) facing {@code yaw}, not yet added to the level: {@code finalizeSpawn} with
     * spawn type EVENT, called directly (no FinalizeSpawnEvent, so no other mod re-rolls it, as
     * {@code HerdBodies} does), then out of vanilla's raids and patrols. Null when the type made no mob.
     * The bridge makes it persistence-required ({@link #hold}) before it joins the level.
     */
    @SuppressWarnings("deprecation") // finalizeSpawn is called directly, on purpose
    public static Mob create(ServerLevel level, EntityType<?> type, double x, double y, double z, float yaw) {
        Entity e = type.create(level);
        if (!(e instanceof Mob m)) {
            if (e != null)
                e.discard();
            return null;
        }
        m.moveTo(x, y, z, yaw, 0f);
        m.setYHeadRot(yaw);
        m.setYBodyRot(yaw);
        m.finalizeSpawn(level, level.getCurrentDifficultyAt(BlockPos.containing(x, y, z)), MobSpawnType.EVENT, null);
        if (m instanceof Raider r) {
            // Raider.finalizeSpawn lets every raider but a natural witch join a raid it walks into; a
            // party's bodies are the engine's, never a vanilla raid's, nor a patrol's leader (only the
            // patrol spawner sets a mob patrolling, and EVENT spawns are never made leaders).
            r.setCanJoinRaid(false);
            r.setPatrolLeader(false);
        }
        return m;
    }

    /** Makes a body persistence-required while it is a party's; returns whether it already was. */
    public static boolean hold(Mob m) {
        boolean was = m.isPersistenceRequired();
        m.setPersistenceRequired();
        return was;
    }

    /**
     * A body leaving the party as an ordinary mob (a take, MZB-102): persistence-required only if it was
     * before the bridge held it, so a carried-off raider despawns as any other would (UR-1).
     */
    public static void release(Mob m, boolean was) {
        ((MobAccessor) m).mic_rtsim$setPersistenceRequired(was);
    }

    /**
     * Takes a mob's target goals away (an envoy party's guard, MOWZIE.md §4.5) and clears its target;
     * returns them for {@link #restoreTargets}. Running goals are stopped first
     * ({@code removeAllGoals} would leave them running).
     */
    public static List<WrappedGoal> stripTargets(Mob m) {
        List<WrappedGoal> saved = new ArrayList<>(m.targetSelector.getAvailableGoals());
        for (WrappedGoal w : saved)
            if (w.isRunning())
                w.stop();
        m.targetSelector.removeAllGoals(g -> true);
        m.setTarget(null);
        return saved;
    }

    /** Gives back target goals {@link #stripTargets} took. */
    public static void restoreTargets(Mob m, List<WrappedGoal> saved) {
        if (saved == null)
            return;
        for (WrappedGoal w : saved)
            m.targetSelector.addGoal(w.getPriority(), w.getGoal());
    }
}
