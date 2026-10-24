package com.minecraftindustrialcomplex.mic_rtsim.conflict;

import net.minecraft.core.Holder;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.phys.Vec3;

import java.util.List;
import java.util.UUID;

/**
 * One kind of camp whose members at home are the jar's own bodies (MOWZIE.md §4.9, ledger 16): what a
 * plugin tells the core's camp reconciler ({@link Camps}) about its mod's bodies, so the core stays
 * mod-agnostic (D4). Mowzie's den and grove are two; a kind is stateless and called on the server thread.
 *
 * <p>Every method sees only the plugin's own entities; the core does the books, the tags, the markers and
 * the records.
 */
public interface CampKind {
    /**
     * The census's chief fields (CAMP_CENSUS): {@code flags} are {@link ConflictRecords.CampCensus}'s CHIEF, CRYSTAL
     * and THRONE. {@code traded} is the chief body's trade memory (players by game profile id) and
     * {@code tradedSlot} and {@code tradedGeneration} the body it is the memory of (MZB-420, MZB-434); a kind with
     * no memory leaves them empty.
     */
    record Chief(int flags, float health, double x, double y, double z, float direction, int tradedSlot, int tradedGeneration,
                 List<UUID> traded) {
        public static final Chief NONE = new Chief(0, 0f, 0, 0, 0, 0f);

        public Chief(int flags, float health, double x, double y, double z, float direction) {
            this(flags, health, x, y, z, direction, 0, 0, List.of());
        }
    }

    /**
     * The chief's trade memory at home (CHIEF_BODY, MZB-421): the players who traded with it, by game profile id,
     * and the engine's health and flags for it (health 0 at home: the body keeps its own).
     */
    record ChiefBody(float health, int flags, List<UUID> traded) {}

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

    /** Where a SPAWN line's body appears (on the ground, in the footprint); null when it finds no free place (the line is stale). */
    Vec3 spawnAt(ServerLevel level, Camps.Site site, String body, int lineFlags);

    /** Whether a SPAWN line waits now (a Frostmaw's while a player is near its den, MOWZIE.md §7 test 5). */
    boolean holdSpawn(ServerLevel level, Camps.Site site, String body, int lineFlags);

    /** A tagged body of the camp joined the level, or became the camp's: what keeps it home (a restriction). */
    void joined(Camps.Site site, Mob body);

    /**
     * Whether a TAKE, REMOVE or SET_OUT-as-take of this body waits now (Rule U, MZB-433): a body a player is
     * near or has as a target. Held with no time limit, answered stale when the camp's epoch ends. False for a
     * kind whose bodies go at once.
     */
    default boolean holdRemoval(ServerLevel level, Camps.Site site, Mob body) {
        return false;
    }

    /**
     * Writes the chief's trade memory onto its body (MZB-421): when the chief body is made for a SPAWN, and at
     * once onto a standing chief body that the engine's memory outgrew. It must not journal anything (the jar's
     * own {@code rememberTrade} is a payment's, not a copy's).
     */
    default void chiefBody(Mob body, ChiefBody memory) {}

    /**
     * Whether this body is in a fight a player began (it has a player as its target and a player hit it lately):
     * a seat over it waits (MZB-410).
     */
    default boolean inFight(ServerLevel level, Mob body) {
        return false;
    }

    /** Copies what makes a body itself (health, crystal, mask, weapon) from one body to another of its kind. */
    void copy(Mob from, Mob to);

    /**
     * Whether this kind plays a reconcile's lines for its chief's slot (a den's Frostmaw, a grove's Umvuthi); a
     * kind that does not answers them stale.
     */
    default boolean chiefLines() {
        return true;
    }

    /** The most bodies this kind's camp keeps at home (a den's one Frostmaw); the census lists no more strays than are free. */
    default int slots(Camps.Site site) {
        return Integer.MAX_VALUE;
    }

    /**
     * How far from the spot a loose body of this kind is its camp's stray, beyond the footprint (MZB-232): a
     * late camp's first census lists the untagged bodies within it, and one that joins within it while the
     * camp has a free slot asks for a new census. Zero for none.
     */
    default double strayRadius(Camps.Site site) {
        return 0;
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
