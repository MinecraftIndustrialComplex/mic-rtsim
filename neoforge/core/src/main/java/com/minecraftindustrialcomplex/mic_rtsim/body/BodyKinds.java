package com.minecraftindustrialcomplex.mic_rtsim.body;

import com.minecraftindustrialcomplex.mic_rtsim.body.vanilla.TraderLlamaAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.body.vanilla.WanderingTraderAdapter;
import net.minecraft.world.phys.Vec3;

import java.util.Collection;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Every body adapter and group kind the bridge knows, by name. Vanilla's are built in; plugins add
 * theirs during mod construction. Each body adapter is also a group kind of one, named like the body.
 */
public final class BodyKinds {
    private BodyKinds() {}

    /** The human caravan of PLAN.md §8.3: one wandering trader and two trader llamas, as vanilla spawns them. */
    public static final String HUMAN_CARAVAN = "mic_rtsim:human_caravan";

    private static final Map<String, BodyAdapter> BODIES = new LinkedHashMap<>();
    private static final Map<String, GroupKind> GROUPS = new LinkedHashMap<>();

    public static final WanderingTraderAdapter WANDERING_TRADER = new WanderingTraderAdapter();
    public static final TraderLlamaAdapter TRADER_LLAMA = new TraderLlamaAdapter();

    static {
        registerBody(WANDERING_TRADER);
        registerBody(TRADER_LLAMA);
        // Vanilla's WanderingTraderSpawner puts two llamas within 4 blocks of the trader at random. A
        // caravan's layout is fixed instead, two blocks off on either side, so a spawn is reproducible;
        // the offsets are PLACEHOLDER until P15 decides the layout format.
        registerGroup(new GroupKind(HUMAN_CARAVAN, List.of(
                new GroupKind.Slot(WANDERING_TRADER, Vec3.ZERO),
                new GroupKind.Slot(TRADER_LLAMA, new Vec3(2, 0, 0)),
                new GroupKind.Slot(TRADER_LLAMA, new Vec3(-2, 0, 0)))));
    }

    // ---- conflict stand-ins (MZB-102) -------------------------------------------------------------

    /**
     * The entity a conflict body kind is embodied as when no plugin gives it one: a culture's abstract
     * body ({@code mic_rtsim:conflict/<culture>/<unit>}, crates/rtsim-conflict/data/cultures) stands in
     * as the vanilla illager of its class. A name that is none of these and not an entity type falls
     * back to {@link #STAND_IN_FALLBACK}.
     */
    public static final String STAND_IN_FALLBACK = "minecraft:pillager";
    private static final String CONFLICT_PREFIX = "mic_rtsim:conflict/";
    private static final Map<String, String> STAND_INS = new LinkedHashMap<>();

    static {
        // Melee to the vindicator, ranged and envoys to the pillager, siege to the ravager, by the unit
        // names of the three built-in cultures (raider, goblin, bandit).
        for (String unit : List.of("vindicator", "warrior", "thug"))
            STAND_INS.put(unit, "minecraft:vindicator");
        for (String unit : List.of("pillager", "archer", "crossbowman", "herald", "emissary", "messenger"))
            STAND_INS.put(unit, "minecraft:pillager");
        for (String unit : List.of("ravager", "brute"))
            STAND_INS.put(unit, "minecraft:ravager");
    }

    /**
     * Gives a conflict body kind the entity type it is embodied as: a full body name
     * ({@code mic_rtsim:conflict/goblin/brute}) or a bare unit name every culture's unit of that name
     * takes ({@code brute}). A later call replaces an earlier one, so a plugin may give a culture's
     * units their own mod's bodies. Call during mod construction.
     */
    public static synchronized void registerStandIn(String body, String entityType) {
        STAND_INS.put(body, entityType);
    }

    /**
     * The entity type id a conflict body kind is embodied as: a registered stand-in for the full name,
     * then for its unit name, then {@link #STAND_IN_FALLBACK} for a culture body; any other name is
     * itself (a plugin's body, an entity type). Null for null.
     */
    public static synchronized String standIn(String body) {
        if (body == null)
            return null;
        String s = STAND_INS.get(body);
        if (s != null)
            return s;
        if (body.startsWith(CONFLICT_PREFIX)) {
            String unit = body.substring(body.lastIndexOf('/') + 1);
            return STAND_INS.getOrDefault(unit, STAND_IN_FALLBACK);
        }
        return body;
    }

    public static synchronized void registerBody(BodyAdapter adapter) {
        if (BODIES.putIfAbsent(adapter.name(), adapter) != null)
            throw new IllegalStateException("body kind " + adapter.name() + " registered twice");
        GROUPS.putIfAbsent(adapter.name(), GroupKind.single(adapter));
    }

    public static synchronized void registerGroup(GroupKind kind) {
        if (kind.slots().isEmpty())
            throw new IllegalArgumentException("group kind " + kind.name() + " has no members");
        GroupKind old = GROUPS.put(kind.name(), kind);
        if (old != null && !(old.size() == 1 && BODIES.containsKey(kind.name())))
            throw new IllegalStateException("group kind " + kind.name() + " registered twice");
    }

    public static synchronized GroupKind group(String name) {
        return GROUPS.get(name);
    }

    public static synchronized BodyAdapter body(String name) {
        return BODIES.get(name);
    }

    public static synchronized Collection<GroupKind> groups() {
        return Collections.unmodifiableCollection(List.copyOf(GROUPS.values()));
    }

    public static synchronized Collection<BodyAdapter> bodies() {
        return Collections.unmodifiableCollection(List.copyOf(BODIES.values()));
    }
}
