package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.core.Holder;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.phys.Vec3;

import java.util.List;

/**
 * One kind of camp whose members at home are the jar's own bodies (MOWZIE.md §4.9, ledger 16): what a
 * plugin tells the core's camp reconciler ({@link Camps}) about its mod's bodies, so the core stays
 * mod-agnostic (D4). Mowzie's den and grove are two; a kind is stateless and called on the server thread.
 *
 * <p>Every method sees only the plugin's own entities; the core does the books, the tags, the markers and
 * the records.
 */
public interface CampKind {
    /** The census's chief fields (CAMP_CENSUS): {@code flags} are {@link ConflictRecords.CampCensus}'s CHIEF, CRYSTAL and THRONE. */
    record Chief(int flags, float health, double x, double y, double z, float direction) {
        public static final Chief NONE = new Chief(0, 0f, 0, 0, 0, 0f);
    }

    /** Whether the entity is a body of this kind's camps that a census counts (tagged or not). */
    boolean isBody(Entity e);

    /** The full body name a census body is reported as ({@code mic_rtsim:conflict/<culture>/<unit>}). */
    String bodyName(Entity e);

    /** Whether a body is its camp's chief (CAMP_DEATH's chief flag). */
    boolean isChief(Entity e);

    /** The chief's fields read from the entities standing in the camp's footprint (all of them, not only bodies). */
    Chief chief(ServerLevel level, Camps.Site site, List<Entity> footprint);

    /**
     * Makes (does not add) a body of {@code body} at (x, y, z) facing {@code yaw} for a SPAWN or a
     * HAND_BACK line; {@code lineFlags} are the line's CHIEF and CRYSTAL. Null when it cannot.
     */
    Mob make(ServerLevel level, Camps.Site site, String body, int lineFlags, double x, double y, double z, float yaw);

    /** Where a SPAWN line's body appears (on the ground, in the footprint). */
    Vec3 spawnAt(ServerLevel level, Camps.Site site, String body, int lineFlags);

    /** Whether a SPAWN line waits now (a Frostmaw's while a player is near its den, MOWZIE.md §7 test 5). */
    boolean holdSpawn(ServerLevel level, Camps.Site site, String body, int lineFlags);

    /** A tagged body of the camp joined the level, or became the camp's: what keeps it home (a restriction). */
    void joined(Camps.Site site, Mob body);

    /** Copies what makes a body itself (health, crystal, mask, weapon) from one body to another of its kind. */
    void copy(Mob from, Mob to);

    /**
     * Whether this kind plays a reconcile's lines for its chief's slot (a den's Frostmaw); a kind that
     * does not answers them stale (the grove's Umvuthi is M9's, MZB-209).
     */
    default boolean chiefLines() {
        return true;
    }

    /** Which deaths in the camp's territory are its kin (the killing signal); null for none. */
    default java.util.function.Predicate<Entity> kin() {
        return this::isBody;
    }

    /** The camp's notices' sound; null for a raid horn. */
    default Holder<SoundEvent> sound() {
        return null;
    }
}
