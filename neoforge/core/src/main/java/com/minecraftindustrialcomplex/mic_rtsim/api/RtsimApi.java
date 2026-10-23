package com.minecraftindustrialcomplex.mic_rtsim.api;

import com.minecraftindustrialcomplex.mic_rtsim.body.BodyAdapter;
import com.minecraftindustrialcomplex.mic_rtsim.body.BodyKinds;
import com.minecraftindustrialcomplex.mic_rtsim.body.GroupKind;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;

/**
 * What plugins ({@code mic_rtsim_<mod>}, PLAN.md §1.3) call. Plugins carry Java adapters and data for
 * one mod and no Rust; this is their whole entry point into the core.
 */
public final class RtsimApi {
    private RtsimApi() {}

    /** Registers a body adapter (also usable as a group of one). Call during mod construction. */
    public static void registerBody(BodyAdapter adapter) {
        BodyKinds.registerBody(adapter);
    }

    /** Registers a group kind (C25). Call during mod construction. */
    public static void registerGroup(GroupKind kind) {
        BodyKinds.registerGroup(kind);
    }

    /**
     * Registers a plugin with records and puppets of its own (a command kind range, an agent namespace,
     * the session's lifecycle; the Ice and Fire plugin's dragons). Call during mod construction.
     */
    public static void registerPlugin(SessionPlugin plugin) {
        SessionPlugins.register(plugin);
    }

    /**
     * Registers a climate source (a seasons or temperature mod's plugin) for the habitat sampler
     * (docs/boundary.md §10). Call during mod construction.
     */
    public static void registerClimate(ClimateSource source) {
        com.minecraftindustrialcomplex.mic_rtsim.habitat.Climates.register(source);
    }

    /**
     * Registers an oil source (a mod that knows where oil is: Create: Diesel Generators' plugin) for the oil
     * gate (OILGATE.md §4.1, OG-2). Call during mod construction.
     */
    public static void registerOilSource(OilSource source) {
        com.minecraftindustrialcomplex.mic_rtsim.oil.OilSources.register(source);
    }

    /**
     * Adds a far feed source (docs/boundary.md §12.3): what moves in a level, drawn for players whose far
     * renderer subscribed. Any time; the feed asks it once a second per level a subscriber is in.
     */
    public static void registerFarSource(com.minecraftindustrialcomplex.mic_rtsim.far.FarSource source) {
        com.minecraftindustrialcomplex.mic_rtsim.far.FarFeed.addSource(source);
    }

    /**
     * A far source's body is joining {@code level} as entity {@code entity} (a dragon materialized, a carriage's
     * entity spawned): call it before the entity is added, and each subscriber holding far object {@code id} is
     * told before the entity's spawn packet, so its client never draws both (docs/choices-dh-feed.md DHT40).
     * Server thread.
     */
    public static void farEntityJoining(net.minecraft.server.level.ServerLevel level, long id, int entity) {
        com.minecraftindustrialcomplex.mic_rtsim.far.FarFeed.embodied(level, id, entity);
    }

    /**
     * Registers a topology source (a world mod that glues a level's edges, Deep Time's layout) asked
     * before Toroidal World's shape and the world border (TP18). Call during mod construction.
     */
    public static void registerTopology(TopologySource source) {
        com.minecraftindustrialcomplex.mic_rtsim.topology.TopologySources.register(source);
    }

    /**
     * A level's topology changed in play (Deep Time's wrap engine installed a layout, ending its
     * edge_stop border): the bridge looks again at the next tick and tells the engine.
     */
    public static void topologyChanged() {
        RtsimSession s = RtsimSession.current();
        if (s != null)
            s.topologies().changed();
    }

    /**
     * A level's topology as the sim sees it: flat when no session runs. Steering code takes every
     * target as its image nearest the body ({@code LevelTopology.nearestImage}), because Toroidal World
     * and the wrap engine fold a body that crosses an edge back into the domain without an event.
     */
    public static com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology topology(net.minecraft.world.level.Level level) {
        RtsimSession s = RtsimSession.current();
        return s == null ? com.minecraftindustrialcomplex.mic_rtsim.topology.LevelTopology.flat() : s.topology(level);
    }

    /** True when the entity is a puppet: tagged with {@code mic_rtsim:agent}. */
    public static boolean isPuppet(Entity entity) {
        return RtsimAttachments.isTagged(entity);
    }

    /**
     * Takes a puppet out of the sim (A10, PLAN.md §6.2): it becomes an ordinary entity, saved by vanilla
     * from now on, and the engine removes the agent's member with a TAKEN observation. Returns false
     * when the entity is not a live puppet.
     *
     * <p><b>The Carry On hook point.</b> {@code mic_rtsim_carryon} listens to Carry On's cancellable
     * {@code tschipp.carryon.events.EntityPickupEvent(ServerPlayer, Entity)} on
     * {@code NeoForge.EVENT_BUS} and calls this with {@link TakeReason#PICKUP} before the pickup goes
     * ahead. Carry On serializes the entity it carries through {@code Entity.save}, which refuses a
     * puppet (EntitySaveMixin), so the take has to come first.
     */
    public static boolean takeFromSim(Entity entity, TakeReason reason, Player player) {
        RtsimSession s = RtsimSession.current();
        return s != null && s.takeEntity(entity, reason.code, player == null ? null : player.getUUID());
    }

    // ---- phase 1 (PLAN.md §8) -------------------------------------------------------------------

    /**
     * Declares the structures of a culture's villages (C22): they are adopted when their start
     * generates. {@code residentPool} names the template pool whose jigsaw block marks where a house's
     * resident stands (Ribbits: {@code ribbits:ribbits}), or null. Call during mod construction.
     */
    public static void registerCulture(java.util.function.Predicate<net.minecraft.core.Holder<net.minecraft.world.level.levelgen.structure.Structure>> structures,
                                       String culture, String residentPool) {
        com.minecraftindustrialcomplex.mic_rtsim.village.Cultures.register(
                new com.minecraftindustrialcomplex.mic_rtsim.village.Cultures.Rule(structures, culture, residentPool));
    }

    /** Declares a village resident type (C2: residents stay ordinary entities; the census counts them). */
    public static void registerResident(net.minecraft.world.entity.EntityType<?> type, com.minecraftindustrialcomplex.mic_rtsim.village.Residents.Kind kind) {
        com.minecraftindustrialcomplex.mic_rtsim.village.Residents.register(type, kind);
    }

    /** How SPAWN_RESIDENT makes a resident of body kind {@code body} (village growth, C7). */
    public static void registerResidentFactory(String body, com.minecraftindustrialcomplex.mic_rtsim.village.Residents.Factory factory) {
        com.minecraftindustrialcomplex.mic_rtsim.village.Residents.registerFactory(body, factory);
    }

    /** The adopted village an entity belongs to (by its home, else where it stands), or null. */
    public static com.minecraftindustrialcomplex.mic_rtsim.village.Village villageOf(Entity entity) {
        RtsimSession s = RtsimSession.current();
        if (s == null)
            return null;
        com.minecraftindustrialcomplex.mic_rtsim.village.Village v =
                com.minecraftindustrialcomplex.mic_rtsim.village.Residents.villageOf(s.phase1.villages, entity);
        return v != null ? v : s.phase1.villages.at(entity.level().dimension(), entity.blockPosition());
    }

    /** A holder's (a village's) sentiment snapshot from the engine, or null when it sent none. */
    public static com.minecraftindustrialcomplex.mic_rtsim.knowledge.Sentiments.Snapshot sentiment(long holder) {
        RtsimSession s = RtsimSession.current();
        return s == null ? null : s.phase1.sentiments.get(holder);
    }

    /**
     * A restock just reset an offer's uses: in an adopted village with a market they follow the
     * treasury rule instead (C14, C23). For plugins' restock mixins ({@code RibbitEntity.restock}).
     */
    public static void afterResetUses(Entity merchant, net.minecraft.world.item.trading.MerchantOffer offer) {
        RtsimSession s = RtsimSession.current();
        if (s != null && !s.failed())
            s.phase1.afterResetUses(merchant, offer);
    }

    /**
     * True when the sim prices this offer, so vanilla's demand must leave it alone: for plugins'
     * {@code updateDemand} mixins ({@code RibbitEntity.updateDemand}).
     */
    public static boolean pricesOffer(Entity merchant, net.minecraft.world.item.trading.MerchantOffer offer) {
        RtsimSession s = RtsimSession.current();
        return s != null && !s.failed() && s.phase1.pricesOffer(merchant, offer);
    }

    /** The live puppet group an entity is a member of, or null. */
    public static com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup group(Entity entity) {
        RtsimSession s = RtsimSession.current();
        com.minecraftindustrialcomplex.mic_rtsim.puppet.AgentTag tag = RtsimAttachments.tag(entity);
        return s == null || tag == null ? null : s.registry().live(tag, entity.getUUID());
    }
}
