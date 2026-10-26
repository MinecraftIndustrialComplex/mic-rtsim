package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.EntityType;

import java.util.HashMap;
import java.util.Map;

/**
 * The {@link Traveller}s plugins registered, by entity type id (a string, so a plugin's constructor can name a type
 * before the registries freeze, as {@link Lurkers} does).
 */
public final class Travellers {
    private Travellers() {}

    private static final Map<String, Traveller> BY_TYPE = new HashMap<>();

    /** Registers the Traveller of an entity type ({@code mowziesmobs:umvuthi}); a later call replaces an earlier one. */
    public static synchronized void register(String entityType, Traveller traveller) {
        BY_TYPE.put(entityType, traveller);
    }

    /** The Traveller for a body type, or null: it walks. Looked up once, when the party's slot is made. */
    static synchronized Traveller of(EntityType<?> type) {
        if (BY_TYPE.isEmpty())
            return null;
        return BY_TYPE.get(BuiltInRegistries.ENTITY_TYPE.getKey(type).toString());
    }
}
