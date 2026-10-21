package com.minecraftindustrialcomplex.mic_rtsim_sandworms;

import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.Entity;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.LivingDamageEvent;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Passive probes the GameTests read: each server tick's wall time and every damage event while a test records. They change nothing. */
public final class Probes {
    private Probes() {}

    private static long tickStart;
    /** Nanoseconds of each server tick (Pre to Post) while {@link #clockOn}. */
    public static final List<Long> TICKS = Collections.synchronizedList(new ArrayList<>());
    public static volatile boolean clockOn;

    /** Damage events while recording: "victim <- source type by direct entity @tick: amount". */
    public static final List<String> HITS = Collections.synchronizedList(new ArrayList<>());
    public static volatile boolean recordHits;

    /** Block break events (a worm that broke blocks through the API would fire them; setBlock-based removal would not). */
    public static final List<String> BREAKS = Collections.synchronizedList(new ArrayList<>());

    static void register() {
        NeoForge.EVENT_BUS.addListener(ServerTickEvent.Pre.class, e -> tickStart = System.nanoTime());
        NeoForge.EVENT_BUS.addListener(ServerTickEvent.Post.class, e -> {
            if (clockOn)
                TICKS.add(System.nanoTime() - tickStart);
        });
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, true, LivingDamageEvent.Post.class, e -> {
            if (recordHits) {
                Entity direct = e.getSource().getDirectEntity();
                HITS.add(id(e.getEntity()) + "#" + e.getEntity().getId() + " <- " + e.getSource().getMsgId() + " by " + (direct == null ? "-" : id(direct))
                        + " @" + e.getEntity().level().getGameTime() + ": " + e.getNewDamage());
            }
        });
        NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, true, BlockEvent.BreakEvent.class, e -> BREAKS.add(e.getPos().toShortString()));
    }

    private static String id(Entity e) {
        return BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()).toString();
    }
}
