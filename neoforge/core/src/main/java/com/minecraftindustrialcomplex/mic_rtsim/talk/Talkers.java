package com.minecraftindustrialcomplex.mic_rtsim.talk;

import com.minecraftindustrialcomplex.mic_rtsim.api.Talker;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * The registry of talkers: entity type id to the talkers asked for it, in registration order (TK28).
 * Registered during mod construction, read on every interaction; copy-on-write like {@code Residents}.
 */
public final class Talkers {
    private Talkers() {}

    private static volatile Map<ResourceLocation, List<Talker>> byType = Map.of();
    /** The core's own talkers: always asked after every plugin's (TALK.md §4). */
    private static volatile Map<ResourceLocation, List<Talker>> builtin = Map.of();
    private static volatile Set<ResourceLocation> always = Set.of();

    /** A plugin's talker: asked before the core's built-ins, in registration order. */
    public static synchronized void register(ResourceLocation typeId, Talker talker, boolean alwaysTalks) {
        byType = added(byType, typeId, talker);
        markAlways(typeId, alwaysTalks);
    }

    /** One of the core's own talkers: always asked after the plugins'. */
    public static synchronized void registerBuiltin(ResourceLocation typeId, Talker talker, boolean alwaysTalks) {
        builtin = added(builtin, typeId, talker);
        markAlways(typeId, alwaysTalks);
    }

    private static Map<ResourceLocation, List<Talker>> added(Map<ResourceLocation, List<Talker>> from, ResourceLocation typeId, Talker talker) {
        Map<ResourceLocation, List<Talker>> next = new java.util.HashMap<>(from);
        List<Talker> list = new ArrayList<>(next.getOrDefault(typeId, List.of()));
        list.add(talker);
        next.put(typeId, List.copyOf(list));
        return Map.copyOf(next);
    }

    private static void markAlways(ResourceLocation typeId, boolean alwaysTalks) {
        if (alwaysTalks) {
            Set<ResourceLocation> a = new HashSet<>(always);
            a.add(typeId);
            always = Set.copyOf(a);
        }
    }

    /**
     * The first talker of this entity's type whose {@code talks} says yes for this player, plugins before
     * the core's own, or null. A talker that throws is skipped (logged once).
     */
    public static Talker find(Entity entity, ServerPlayer player) {
        ResourceLocation id = BuiltInRegistries.ENTITY_TYPE.getKey(entity.getType());
        Talker t = first(byType.get(id), entity, player);
        return t != null ? t : first(builtin.get(id), entity, player);
    }

    private static Talker first(List<Talker> list, Entity entity, ServerPlayer player) {
        if (list == null)
            return null;
        for (Talker t : list)
            try {
                if (t.talks(entity, player))
                    return t;
            } catch (Throwable e) {
                Talk.failed(t, "talks", e);
            }
        return null;
    }

    /** True when the type was registered as always talking: the client cancels its clicks too (§2). */
    public static boolean alwaysTalks(EntityType<?> type) {
        Set<ResourceLocation> a = always;
        return !a.isEmpty() && a.contains(BuiltInRegistries.ENTITY_TYPE.getKey(type));
    }

    public static boolean has(EntityType<?> type) {
        ResourceLocation id = BuiltInRegistries.ENTITY_TYPE.getKey(type);
        return byType.containsKey(id) || builtin.containsKey(id);
    }
}
