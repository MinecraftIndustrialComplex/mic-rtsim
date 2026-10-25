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

    /**
     * A plugin found a conflict camp (MOWZIE.md §4.9): FACTION_FOUND goes to the engine when the
     * conflict module is on. {@code structure} is the structure's id ({@code minecraft:pillager_outpost},
     * {@code goblinstyranny:goblin_camp}); {@code pos} a block in its start chunk, which keys the faction;
     * {@code kin} which deaths in its territory are its own (a player's kill of one is the killing
     * signal); {@code sound} its notices' sound (null: a raid horn); {@code homeBodies} and {@code census}
     * the census at discovery ({@code ConflictRecords.FactionFound.CENSUS_*}). Returns the faction key,
     * or 0 with no live session.
     */
    public static long reportCamp(net.minecraft.server.level.ServerLevel level, String structure, net.minecraft.core.BlockPos pos,
                                  java.util.function.Predicate<Entity> kin,
                                  net.minecraft.core.Holder<net.minecraft.sounds.SoundEvent> sound, boolean late,
                                  int homeBodies, int census) {
        return com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge.INSTANCE.reportCamp(level, structure, pos, kin,
                sound, late, homeBodies, census);
    }

    /**
     * A camp kind (MOWZIE.md §4.9, M5): what the core's camp reconciler needs to know of a mod's camp
     * bodies. Registered once; {@link #declareCamp} registers it too.
     */
    public static void registerCampKind(com.minecraftindustrialcomplex.mic_rtsim.conflict.CampKind kind) {
        com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge.INSTANCE.reconciler().registerKind(kind);
    }

    /**
     * A plugin found a camp whose members at home are its mod's own bodies (MOWZIE.md §4.9): the core
     * follows its footprint in and out, sends its census and plays the engine's reconciles on it.
     * {@code start} is the structure start's chunk, which keys the faction; {@code pos} the block
     * FACTION_FOUND names (clamped into that chunk); {@code spot} where its bodies are kept and spawned;
     * the footprint is the circle of {@code radius} round {@code spot}, and with a {@code halfHeight} above
     * zero only that far above and below it (a cylinder; zero is no height bound).
     * Returns the faction key, or 0 with no live session (ask again when the chunk next loads).
     */
    public static long declareCamp(net.minecraft.server.level.ServerLevel level, net.minecraft.world.level.ChunkPos start,
                                   String structure, com.minecraftindustrialcomplex.mic_rtsim.conflict.CampKind kind,
                                   net.minecraft.core.BlockPos pos, net.minecraft.world.phys.Vec3 spot,
                                   double radius, double halfHeight, boolean late) {
        var c = com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge.INSTANCE.reconciler().declare(level, start,
                structure, kind, pos, spot, radius, halfHeight, late);
        return c == null ? 0L : c.faction;
    }

    /** Whether an entity is a body a camp holds (tagged {@code mic_rtsim:camp_member}). */
    public static boolean isCampBody(Entity entity) {
        return com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps.isHeld(entity);
    }

    /**
     * A player carries off a body a camp holds (Carry On, A10): CAMP_DEATH with killer kind taken, and its
     * tag goes before Carry On saves it, so it is put down a stray. False when no camp holds it.
     */
    public static boolean takeCampBody(Entity entity, net.minecraft.server.level.ServerPlayer player) {
        return com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge.INSTANCE.reconciler().take(entity, player);
    }

    /**
     * A plugin saw a chief at home change its crystal (the Frostmaw's steal, MZB-274): CHIEF_STATE with
     * {@code flags} (1: it holds its crystal) and the body's health, for a camp's own chief body or a chief
     * puppet of a conflict party; any other entity is ignored. Latest wins per faction, written at the
     * start of the next tick.
     */
    public static void chiefState(net.minecraft.world.entity.LivingEntity body, int flags) {
        com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge.INSTANCE.reconciler().chiefState(body, flags);
    }

    /**
     * A conflict party member carried items off from a container (BAND_LOOTED). False when the entity
     * is no live member.
     */
    public static boolean looted(Entity member, java.util.List<net.minecraft.world.item.ItemStack> items) {
        return com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge.INSTANCE.looted(member, items);
    }

    /** The conflict factions' territories, by dimension and position (read only; M7's train sampler). */
    public static com.minecraftindustrialcomplex.mic_rtsim.conflict.Territories territories() {
        return com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge.INSTANCE.territories();
    }

    /**
     * Whether the conflict engine is up for a plugin that reports rail passes ({@link #railPass}): a live
     * session with the conflict module on (M7, MZB-320). The train sampler asks
     * {@code territories().anyMinding(...)} first (one array read, and with no territory minding the rail
     * nothing else matters) and this second (MZB-386).
     */
    public static boolean conflictUp() {
        return com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge.INSTANCE.up();
    }

    /**
     * A plugin saw a train pass through {@code faction}'s territory (M7, MZB-325): RAIL_PASS, journaled at
     * once. {@code train} is the plugin's stable id for it, {@code owner} its owner (null: none), the point a
     * position on the track (the sample nearest the territory's centre) in {@code dimension}, and
     * {@code entry} and {@code exit} game time. False with no live session or the conflict module off.
     */
    public static boolean railPass(long faction, long train, java.util.UUID owner,
                                   net.minecraft.resources.ResourceKey<net.minecraft.world.level.Level> dimension, double x,
                                   double y, double z, long entry, long exit) {
        return com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge.INSTANCE.railPass(faction, train, owner, dimension,
                x, y, z, entry, exit);
    }

    // ---- TALK (TALK.md §2, §4) -----------------------------------------------------------------

    /**
     * Registers a talker for an entity type, by its registry id (TK28): so a plugin for a soft
     * dependency names a type without touching the mod's classes. Callable from mod construction.
     * Several talkers on one type are asked in registration order; the first whose
     * {@link Talker#talks} says yes speaks. Plugin talkers are asked before the core's own built-ins
     * (the villager and wandering trader talkers), so a plugin can take over part of a vanilla type by
     * answering {@code talks} for the entities it means. {@code alwaysTalks} says every entity of the type talks to
     * every player, which lets the client cancel the click too (no screen opens, no second packet).
     */
    public static void registerTalker(net.minecraft.resources.ResourceLocation entityTypeId, Talker talker, boolean alwaysTalks) {
        com.minecraftindustrialcomplex.mic_rtsim.talk.Talkers.register(entityTypeId, talker, alwaysTalks);
    }

    /**
     * {@link #registerTalker(net.minecraft.resources.ResourceLocation, Talker, boolean)} by the type itself.
     *
     * @throws IllegalArgumentException when the type is not in the entity type registry
     */
    public static void registerTalker(net.minecraft.world.entity.EntityType<?> type, Talker talker, boolean alwaysTalks) {
        registerTalker(net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.getResourceKey(type)
                .orElseThrow(() -> new IllegalArgumentException("an unregistered entity type has no id: use the ResourceLocation overload of registerTalker"))
                .location(), talker, alwaysTalks);
    }
}
