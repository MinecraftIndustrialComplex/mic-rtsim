package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import com.iafenvoy.iceandfire.data.DragonColor;
import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.iafenvoy.iceandfire.entity.DragonChargeEntity;
import com.iafenvoy.iceandfire.entity.util.HomePosition;
import com.iafenvoy.iceandfire.entity.util.dragon.IafDragonAttacks;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.tags.TagKey;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.phys.Vec3;

import java.util.List;

/**
 * Every reach into Ice and Fire CE 2.1-beta.1 (verified with javap against the pack's jar, 2026-10-20;
 * PATCHES.md lists each member). Nothing else in the plugin names an Ice and Fire class.
 *
 * <ul>
 *   <li>{@code com.iafenvoy.iceandfire.entity.DragonBaseEntity extends TamableAnimal}: the three species'
 *       base class. Public: {@code addAdditionalSaveData} / {@code readAdditionalSaveData} (NBT keys
 *       {@code Hunger} int, {@code AgeTicks} int, {@code Gender} bool (true male), {@code Variant} string,
 *       {@code Sleeping} bool (read through {@code setInSittingPose}), {@code AgingDisabled} bool,
 *       {@code Flying}, {@code Hovering} bools, {@code HasHomePosition} bool; the read ends with
 *       {@code setConfigurableAttributes} and {@code refreshDirtyAttributes}); getters and setters for
 *       each; {@code isModelDead()}; {@code public final IafDragonFlightManager flightManager};
 *       {@code public HomePosition homePos}, {@code public boolean hasHomePosition};
 *       {@code public final DragonType dragonType}.</li>
 *   <li>{@code Variant} is {@code DragonColor.getName()} (e.g. {@code red}), not a namespaced id:
 *       {@code finalizeSpawn} and both lair pieces set it from {@code dragonType.colors()}, and each
 *       {@code DragonColor} adds itself to its type's list in construction order
 *       ({@code IafDragonColors}: fire red, green, bronze, gray; ice blue, white, sapphire, silver;
 *       lightning electric, amethyst, copper, black). The boundary's variant index is the position in
 *       that list.</li>
 *   <li>{@code IafDragonFlightManager.setFlightTarget(Vec3)} only stores the target; {@code update()}
 *       replaces it with the attack target's position while the dragon has a live one, picks a new random
 *       one when the dragon is within 2 blocks of it, and clamps y to {@code maxFlight}.</li>
 *   <li>{@code HomePosition(BlockPos, Level)}: the home and its dimension name, as the lair pieces set it.</li>
 *   <li>Fire (OILGATE.md §4.3, verified with CFR against the jar, 2026-10-21): every breath goes through
 *       {@code public final void breathAttack(double, double, double, boolean useCharge)}, which posts
 *       {@code com.iafenvoy.iceandfire.event.DragonFireEvent} (cancellable; {@code getDragon()}) and stops
 *       when it is cancelled. A stream posts one each tick while {@code isActuallyBreathingFire()}
 *       ({@code fireBreathTicks > 20}), for at most {@code ageInDays * maxBreathTimeMul} ticks (the pack's
 *       2.0); the flight manager's {@code HOVER_BLAST} posts one each tick of the {@code FIRECHARGE}
 *       animation and spawns the charge at its tick 20. {@code FireDragonEntity.shootFireAtMob}'s ground
 *       charge (one roll in five while {@code groundAttack} is {@code FIRE}) spawns a
 *       {@code DragonChargeEntity} (a {@code Fireball}, owner the dragon) with no event. The attack picks
 *       are public fields: {@code airAttack} ({@code IafDragonAttacks.Air}: {@code SCORCH_STREAM},
 *       {@code HOVER_BLAST}, {@code TACKLE}), {@code groundAttack} ({@code IafDragonAttacks.Ground}:
 *       {@code BITE}, {@code SHAKE_PREY}, {@code TAIL_WHIP}, {@code WING_BLAST}, {@code FIRE}) and
 *       {@code usingGroundAttack}; {@code setBreathingFire(boolean)} starts and stops a stream.</li>
 * </ul>
 */
public final class IafDragons {
    private IafDragons() {}

    /** The species entity types (verified registry names, {@code IafEntities}), in boundary species order. */
    public static final List<String> SPECIES = List.of("iceandfire:fire_dragon", "iceandfire:ice_dragon", "iceandfire:lightning_dragon");

    /** The lair structures (verified, {@code data/iceandfire/worldgen/structure}): [species][roost, cave]. */
    public static final String[][] LAIRS = {
            {"iceandfire:fire_dragon_roost", "iceandfire:fire_dragon_cave"},
            {"iceandfire:ice_dragon_roost", "iceandfire:ice_dragon_cave"},
            {"iceandfire:lightning_dragon_roost", "iceandfire:lightning_dragon_cave"},
    };

    /** {@code #iceandfire:dragon_targets}: the land animals Ice and Fire's dragons hunt (verified tag). */
    public static final TagKey<EntityType<?>> DRAGON_TARGETS =
            TagKey.create(Registries.ENTITY_TYPE, ResourceLocation.fromNamespaceAndPath("iceandfire", "dragon_targets"));

    public static boolean isDragon(Entity e) {
        return e instanceof DragonBaseEntity;
    }

    /** The species index of a dragon entity (0 fire, 1 ice, 2 lightning), or -1. */
    public static int species(Entity e) {
        ResourceLocation id = BuiltInRegistries.ENTITY_TYPE.getKey(e.getType());
        return id == null ? -1 : SPECIES.indexOf(id.toString());
    }

    public static EntityType<?> speciesType(String name) {
        ResourceLocation id = name == null ? null : ResourceLocation.tryParse(name);
        if (id == null || !SPECIES.contains(name))
            return null;
        return BuiltInRegistries.ENTITY_TYPE.getOptional(id).orElse(null);
    }

    /** The lair structure name's species and kind as {species, 0 roost or 1 cave}, or null. */
    public static int[] lairKind(String structure) {
        for (int s = 0; s < LAIRS.length; s++)
            for (int k = 0; k < 2; k++)
                if (LAIRS[s][k].equals(structure))
                    return new int[] {s, k};
        return null;
    }

    // ---- reading a body -------------------------------------------------------------------------

    public static DragonBaseEntity dragon(Entity e) {
        return e instanceof DragonBaseEntity d ? d : null;
    }

    public static int hunger(DragonBaseEntity d) {
        return d.getHunger();
    }

    public static int ageTicks(DragonBaseEntity d) {
        return d.getAgeInTicks();
    }

    public static boolean male(DragonBaseEntity d) {
        return d.isMale();
    }

    public static boolean sleeping(DragonBaseEntity d) {
        return d.isSleeping();
    }

    public static boolean flying(DragonBaseEntity d) {
        return d.isFlying() || d.isHovering();
    }

    public static boolean agingDisabled(DragonBaseEntity d) {
        return d.isAgingDisabled();
    }

    /** True once the dragon died: Ice and Fire keeps the body as a corpse to harvest ({@code tickDeath}). */
    public static boolean dead(DragonBaseEntity d) {
        return d.isModelDead() || d.isDeadOrDying() || d.isRemoved();
    }

    /** The variant index of a dragon's colour within its species (0 to 3); 0 when unknown. */
    public static int variant(DragonBaseEntity d) {
        String name = d.getVariant();
        List<DragonColor> colors = d.dragonType.colors();
        for (int i = 0; i < colors.size(); i++)
            if (colors.get(i).getName().equals(name))
                return i;
        return 0;
    }

    /** The home Ice and Fire keeps, or null. */
    public static BlockPos home(DragonBaseEntity d) {
        return d.hasHomePosition && d.homePos != null ? d.homePos.getPosition() : null;
    }

    // ---- writing a body -------------------------------------------------------------------------

    /**
     * Writes the engine's state into a dragon before it joins a level, through Ice and Fire's own NBT
     * read (docs/boundary.md §6.5): {@code AgeTicks} and {@code AgingDisabled} = true (the engine owns
     * age), {@code Hunger}, {@code Gender}, {@code Variant}, {@code Sleeping}; the read refreshes the
     * attributes from the age, so max health follows. The home goes into {@code homePos} directly
     * (Ice and Fire's {@code HomePosition}): the NBT path drops a home with any coordinate exactly 0.
     * Not on a live dragon: Ice and Fire's save removes its multipart parts.
     */
    public static void write(DragonBaseEntity d, ServerLevel level, int ageTicks, int hunger, boolean male, int variant,
                             boolean sleeping, BlockPos home) {
        CompoundTag t = new CompoundTag();
        d.addAdditionalSaveData(t);
        t.putInt("AgeTicks", Math.max(0, ageTicks));
        t.putBoolean("AgingDisabled", true);
        t.putInt("Hunger", Math.max(0, Math.min(100, hunger)));
        t.putBoolean("Gender", male);
        List<DragonColor> colors = d.dragonType.colors();
        if (!colors.isEmpty())
            t.putString("Variant", colors.get(Math.floorMod(variant, colors.size())).getName());
        t.putBoolean("Sleeping", sleeping);
        t.putBoolean("Flying", false);
        t.putBoolean("Hovering", false);
        t.putBoolean("HasHomePosition", false);
        d.readAdditionalSaveData(t);
        if (home != null)
            setHome(d, level, home);
    }

    public static void setHome(DragonBaseEntity d, ServerLevel level, BlockPos home) {
        d.homePos = new HomePosition(home, level);
        d.hasHomePosition = true;
    }

    // ---- steering -------------------------------------------------------------------------------

    /** Points the dragon's flight at a target and keeps it flying (awake, airborne). */
    public static void flyTo(DragonBaseEntity d, Vec3 target) {
        if (d.isSleeping())
            d.setInSittingPose(false);
        if (!d.isFlying())
            d.setFlying(true);
        d.flightManager.setFlightTarget(target);
    }

    /** Lands where it is: Ice and Fire's own landing takes over. */
    public static void land(DragonBaseEntity d) {
        d.setFlying(false);
        d.setHovering(false);
    }

    public static Vec3 flightTarget(DragonBaseEntity d) {
        return d.flightManager.getFlightTarget();
    }

    /** A dragon's fire charge entity, and the dragon that shot it (null when it is not one). */
    public static DragonBaseEntity chargeOwner(Entity e) {
        return e instanceof DragonChargeEntity c && c.getOwner() instanceof DragonBaseEntity d ? d : null;
    }

    /**
     * A dragon out of oil fights with bite and claws (OG-13): no fire attack is picked (the air attack
     * becomes the tackle, the ground fire a bite) and a stream it began stops.
     */
    public static void withoutFire(DragonBaseEntity d) {
        if (d.airAttack != IafDragonAttacks.Air.TACKLE)
            d.airAttack = IafDragonAttacks.Air.TACKLE;
        if (d.groundAttack == IafDragonAttacks.Ground.FIRE)
            d.groundAttack = IafDragonAttacks.Ground.BITE;
        if (d.isBreathingFire())
            d.setBreathingFire(false);
    }
}
