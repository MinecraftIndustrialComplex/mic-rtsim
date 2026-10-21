package com.minecraftindustrialcomplex.mic_rtsim_terraentity;

import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.Entity;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.living.FinalizeSpawnEvent;
import net.neoforged.neoforge.event.entity.living.LivingDamageEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Passive probes the GameTests read: how long each server tick took, which entities joined or tried to
 * spawn naturally, and every damage event while a test records. They change nothing.
 */
public final class Probes {
    private Probes() {}

    private static long tickStart;
    /** Nanoseconds of each server tick (Pre to Post) while {@link #clockOn}. */
    public static final List<Long> TICKS = Collections.synchronizedList(new ArrayList<>());
    public static volatile boolean clockOn;

    /** terra_entity (and, as the control, every other) entity ids: finalize events by spawn type, and joins. */
    public static final Map<String, Integer> FINALIZE = new ConcurrentHashMap<>();
    public static final Map<String, Integer> JOINS = new ConcurrentHashMap<>();
    public static volatile boolean countSpawns;

    /** Damage events while recording: "victim <- source type / direct entity: amount". */
    public static final List<String> HITS = Collections.synchronizedList(new ArrayList<>());
    public static volatile boolean recordHits;

    static void register() {
        NeoForge.EVENT_BUS.addListener(ServerTickEvent.Pre.class, e -> tickStart = System.nanoTime());
        NeoForge.EVENT_BUS.addListener(ServerTickEvent.Post.class, e -> {
            if (clockOn)
                TICKS.add(System.nanoTime() - tickStart);
        });
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, true, FinalizeSpawnEvent.class, e -> {
            if (countSpawns)
                FINALIZE.merge(id(e.getEntity()) + " " + e.getSpawnType().name() + (e.isSpawnCancelled() ? " cancelled" : ""), 1, Integer::sum);
        });
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, true, EntityJoinLevelEvent.class, e -> {
            if (countSpawns && !e.getLevel().isClientSide)
                JOINS.merge(id(e.getEntity()), 1, Integer::sum);
        });
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, true, LivingDamageEvent.Post.class, e -> {
            if (recordHits) {
                Entity direct = e.getSource().getDirectEntity();
                HITS.add(id(e.getEntity()) + "#" + e.getEntity().getId() + " <- " + e.getSource().getMsgId() + " by " + (direct == null ? "-" : id(direct))
                        + " @" + e.getEntity().level().getGameTime() + ": " + e.getNewDamage());
            }
        });
    }

    private static String id(Entity e) {
        return BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()).toString();
    }

    public static Map<String, Integer> sorted(Map<String, Integer> m) {
        return new TreeMap<>(m);
    }
}
