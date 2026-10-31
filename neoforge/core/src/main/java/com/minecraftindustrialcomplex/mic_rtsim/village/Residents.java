package com.minecraftindustrialcomplex.mic_rtsim.village;

import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.npc.Villager;
import net.minecraft.world.phys.Vec3;

import java.util.HashMap;
import java.util.Map;
import java.util.function.Function;

/**
 * The entity types that are village residents (C2: they stay vanilla entities; the sim observes them
 * in the census and nudges them through offers, gossip and goals). The core knows vanilla villagers; a
 * plugin adds its mod's residents ({@code mic_rtsim_ribbits} adds {@code ribbits:ribbit}). Residents
 * are spawned for growth through a {@link Factory} registered under the body kind name.
 */
public final class Residents {
    private Residents() {}

    /**
     * How the bridge reads one resident type.
     *
     * @param profession the resident's profession name (DICT_PROFESSION), or null
     * @param home       where the resident lives, to find its village; null = where it stands
     * @param vanillaGossip true when vanilla keeps gossip for this type (villagers): the sim writes
     *                      slights into it (C20) and prices carry no extra markup
     */
    public record Kind(Function<Entity, String> profession, Function<Entity, BlockPos> home, boolean vanillaGossip) {}

    /** Makes a new resident (not yet added to the level). Returns null when it cannot. */
    public interface Factory {
        Entity create(ServerLevel level, Vec3 pos, String profession);
    }

    /**
     * Picks the profession of a renewal arrival of one body kind (slice S6, EQC180): asked only for an arrival order that names
     * none, with the village and the order's op, which makes the choice a function of the order.
     */
    public interface ProfessionChooser {
        String choose(ServerLevel level, VillageRegistry villages, Village village, long op);
    }

    /**
     * Makes a block a resident's home (slice S6 review, EQC330): the plugin's hook for a type whose own goals pull it home at
     * night ({@code ribbits:ribbit}'s go-home goal). A settler walks in from 96 to 176 blocks out and is made there, which is
     * where such a type takes its home from; it is homed at the point it walks to, and at the village's meeting point when the
     * walk ends.
     */
    public interface Rehomer {
        void rehome(Entity e, BlockPos home);
    }

    // Copy on write: registration is rare (mod construction, setup), lookups are on every damage event,
    // interaction and census entity, so they read a published immutable map with no lock.
    private static volatile Map<EntityType<?>, Kind> KINDS = Map.of();
    private static volatile Map<String, Factory> FACTORIES = Map.of();
    private static volatile Map<String, ProfessionChooser> CHOOSERS = Map.of();
    private static volatile Map<EntityType<?>, Rehomer> REHOMERS = Map.of();

    static {
        register(EntityType.VILLAGER, new Kind(
                e -> BuiltInRegistries.VILLAGER_PROFESSION.getKey(((Villager) e).getVillagerData().getProfession()).toString(),
                e -> ((Villager) e).getBrain().getMemory(net.minecraft.world.entity.ai.memory.MemoryModuleType.MEETING_POINT)
                        .map(net.minecraft.core.GlobalPos::pos).orElse(null),
                true));
        // A villager a village's settlers or growth bring (EQC176): an ordinary adult, its type from the biome it stands in
        // (finalizeSpawn does not set it for an event spawn), no profession (the workstations give one), never a baby.
        registerFactory("minecraft:villager", (level, pos, profession) -> {
            Villager v = EntityType.VILLAGER.create(level);
            if (v == null)
                return null;
            v.moveTo(pos.x, pos.y, pos.z, level.random.nextFloat() * 360f, 0f);
            v.finalizeSpawn(level, level.getCurrentDifficultyAt(v.blockPosition()), net.minecraft.world.entity.MobSpawnType.EVENT,
                    new net.minecraft.world.entity.AgeableMob.AgeableMobGroupData(false));
            v.setVillagerData(v.getVillagerData().setType(net.minecraft.world.entity.npc.VillagerType.byBiome(level.getBiome(v.blockPosition()))));
            return v;
        });
        // A bought work's body (SINK.md: one iron golem at the meeting point). Not player made, so it
        // defends the village as vanilla's own village golems do; the spawner makes it persistent.
        registerFactory("minecraft:iron_golem", (level, pos, profession) -> {
            var g = EntityType.IRON_GOLEM.create(level);
            if (g == null)
                return null;
            g.moveTo(pos.x, pos.y, pos.z, level.random.nextFloat() * 360f, 0f);
            g.setPlayerCreated(false);
            return g;
        });
    }

    public static synchronized void register(EntityType<?> type, Kind kind) {
        Map<EntityType<?>, Kind> m = new HashMap<>(KINDS);
        m.put(type, kind);
        KINDS = Map.copyOf(m);
    }

    public static synchronized void registerFactory(String bodyKind, Factory factory) {
        Map<String, Factory> m = new HashMap<>(FACTORIES);
        m.put(bodyKind, factory);
        FACTORIES = Map.copyOf(m);
    }

    public static synchronized void registerProfessionChooser(String bodyKind, ProfessionChooser chooser) {
        Map<String, ProfessionChooser> m = new HashMap<>(CHOOSERS);
        m.put(bodyKind, chooser);
        CHOOSERS = Map.copyOf(m);
    }

    public static synchronized void registerRehomer(EntityType<?> type, Rehomer rehomer) {
        Map<EntityType<?>, Rehomer> m = new HashMap<>(REHOMERS);
        m.put(type, rehomer);
        REHOMERS = Map.copyOf(m);
    }

    /** Homes a resident at {@code home} when its type has a hook for it (EQC330); nothing otherwise. */
    public static void rehome(Entity e, BlockPos home) {
        Rehomer r = REHOMERS.get(e.getType());
        if (r != null)
            r.rehome(e, home);
    }

    /** The profession a renewal arrival of this body kind takes, or null when the plugin registered no chooser. */
    public static String chooseProfession(String bodyKind, ServerLevel level, VillageRegistry villages, Village village, long op) {
        ProfessionChooser c = CHOOSERS.get(bodyKind);
        return c == null ? null : c.choose(level, villages, village, op);
    }

    public static Kind kind(Entity e) {
        return KINDS.get(e.getType());
    }

    public static Factory factory(String bodyKind) {
        return FACTORIES.get(bodyKind);
    }

    public static String profession(Entity e) {
        Kind k = kind(e);
        if (k == null)
            return null;
        try {
            return k.profession().apply(e);
        } catch (RuntimeException ex) {
            return null;
        }
    }

    /**
     * The village a resident belongs to: the one it walks to if it is a settler on the road (outside the village's box, and
     * near another village's perhaps: EQC178, EQC333), else its home (or meeting point) if that is in a village, else where it
     * stands.
     */
    public static Village villageOf(VillageRegistry villages, Entity e) {
        Kind k = kind(e);
        if (k == null)
            return null;
        if (!villages.walkers.isEmpty()) {
            VillageRegistry.Walker w = villages.walkers.get(e.getUUID());
            Village walking = w == null ? null : villages.get(w.village);
            if (walking != null)
                return walking;
        }
        BlockPos home = null;
        try {
            home = k.home().apply(e);
        } catch (RuntimeException ignored) {
        }
        if (home != null) {
            Village v = villages.at(e.level().dimension(), home);
            if (v != null)
                return v;
        }
        return villages.at(e.level().dimension(), e.blockPosition());
    }
}
