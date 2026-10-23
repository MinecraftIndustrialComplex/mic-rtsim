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
    private static volatile Set<ResourceLocation> always = Set.of();

    public static synchronized void register(ResourceLocation typeId, Talker talker, boolean alwaysTalks) {
        Map<ResourceLocation, List<Talker>> next = new java.util.HashMap<>(byType);
        List<Talker> list = new ArrayList<>(next.getOrDefault(typeId, List.of()));
        list.add(talker);
        next.put(typeId, List.copyOf(list));
        byType = Map.copyOf(next);
        if (alwaysTalks) {
            Set<ResourceLocation> a = new HashSet<>(always);
            a.add(typeId);
            always = Set.copyOf(a);
        }
    }

    /** The first talker of this entity's type whose {@code talks} says yes for this player, or null. */
    public static Talker find(Entity entity, ServerPlayer player) {
        List<Talker> list = byType.get(BuiltInRegistries.ENTITY_TYPE.getKey(entity.getType()));
        if (list == null)
            return null;
        for (Talker t : list)
            if (t.talks(entity, player))
                return t;
        return null;
    }

    /** True when the type was registered as always talking: the client cancels its clicks too (§2). */
    public static boolean alwaysTalks(EntityType<?> type) {
        Set<ResourceLocation> a = always;
        return !a.isEmpty() && a.contains(BuiltInRegistries.ENTITY_TYPE.getKey(type));
    }

    public static boolean has(EntityType<?> type) {
        return byType.containsKey(BuiltInRegistries.ENTITY_TYPE.getKey(type));
    }
}
