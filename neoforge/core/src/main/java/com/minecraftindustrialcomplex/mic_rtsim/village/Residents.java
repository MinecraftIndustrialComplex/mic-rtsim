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

    private static final Map<EntityType<?>, Kind> KINDS = new HashMap<>();
    private static final Map<String, Factory> FACTORIES = new HashMap<>();

    static {
        register(EntityType.VILLAGER, new Kind(
                e -> BuiltInRegistries.VILLAGER_PROFESSION.getKey(((Villager) e).getVillagerData().getProfession()).toString(),
                e -> ((Villager) e).getBrain().getMemory(net.minecraft.world.entity.ai.memory.MemoryModuleType.MEETING_POINT)
                        .map(net.minecraft.core.GlobalPos::pos).orElse(null),
                true));
    }

    public static synchronized void register(EntityType<?> type, Kind kind) {
        KINDS.put(type, kind);
    }

    public static synchronized void registerFactory(String bodyKind, Factory factory) {
        FACTORIES.put(bodyKind, factory);
    }

    public static synchronized Kind kind(Entity e) {
        return KINDS.get(e.getType());
    }

    public static synchronized Factory factory(String bodyKind) {
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

    /** The village a resident belongs to: its home (or meeting point) if that is in a village, else where it stands. */
    public static Village villageOf(VillageRegistry villages, Entity e) {
        Kind k = kind(e);
        if (k == null)
            return null;
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
