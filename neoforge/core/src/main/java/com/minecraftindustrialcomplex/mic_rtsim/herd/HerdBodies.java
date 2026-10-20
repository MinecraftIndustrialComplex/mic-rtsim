package com.minecraftindustrialcomplex.mic_rtsim.herd;

import com.minecraftindustrialcomplex.mic_rtsim.mixin.AbstractHorseInvoker;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Leashable;
import net.minecraft.world.entity.MobSpawnType;
import net.minecraft.world.entity.OwnableEntity;
import net.minecraft.world.entity.animal.Animal;
import net.minecraft.world.entity.animal.Sheep;
import net.minecraft.world.entity.animal.goat.Goat;
import net.minecraft.world.entity.animal.horse.AbstractHorse;
import net.minecraft.world.entity.animal.horse.Horse;
import net.minecraft.world.entity.player.Player;

import java.util.List;
import java.util.UUID;

/**
 * The five vanilla herd bodies (D4, docs/boundary.md §8.2): {@code minecraft:sheep}, {@code cow},
 * {@code pig}, {@code horse} and {@code goat}, every one an {@link Animal}. Verified in the 1.21.1
 * sources (NeoForm's decompiled {@code EntityType}, {@code Sheep}, {@code Horse}, {@code AbstractHorse},
 * {@code Goat}, {@code AgeableMob}; choices Y10 to Y13 in docs/overnight-choices-herdsmc.md).
 *
 * <p><b>A body</b> is made as vanilla makes an animal ({@code finalizeSpawn}, called directly with spawn
 * type EVENT, so NeoForge's {@code FinalizeSpawnEvent} does not fire and no other mod re-rolls it), then
 * given the engine's look, age and shearing through the entity's own NBT read ({@code Color} and
 * {@code Sheared} for sheep, {@code Variant} for horses, {@code IsScreamingGoat}, {@code HasLeftHorn}
 * and {@code HasRightHorn} for goats, {@code Age} for all), and what vanilla rolls per animal from the
 * member's identity seed: a horse's health, speed and jump ({@code AbstractHorse.randomizeAttributes},
 * which vanilla feeds the level's random), a goat's lost horn. The same member comes back the same.
 */
public final class HerdBodies {
    private HerdBodies() {}

    /** The species' entity type ids, in the engine's order ({@code HerdSpecies::ALL}). */
    public static final List<String> SPECIES = List.of("minecraft:sheep", "minecraft:cow", "minecraft:pig", "minecraft:horse", "minecraft:goat");

    /** A herd species' entity type, or null for any other name. */
    public static EntityType<?> type(String name) {
        if (name == null || !SPECIES.contains(name))
            return null;
        ResourceLocation id = ResourceLocation.tryParse(name);
        return id == null ? null : BuiltInRegistries.ENTITY_TYPE.getOptional(id).orElse(null);
    }

    /**
     * Makes a member's body at (x, y, z), not yet added to the level. Null when the type made no
     * animal. {@code variant}: a sheep's dye id, a horse's {@code colour | markings << 8}, 1 for a
     * screaming goat; {@code vanillaAge}: {@code AgeableMob}'s {@code Age}.
     */
    @SuppressWarnings("deprecation") // finalizeSpawn is called directly, on purpose: no FinalizeSpawnEvent
    public static Animal create(ServerLevel level, EntityType<?> type, double x, double y, double z, long seed, int variant,
                                int vanillaAge, boolean sheared) {
        Entity e = type.create(level);
        if (!(e instanceof Animal a)) {
            if (e != null)
                e.discard();
            return null;
        }
        RandomSource r = RandomSource.create(seed);
        float yaw = r.nextFloat() * 360f;
        a.moveTo(x, y, z, yaw, 0f);
        a.setYHeadRot(yaw);
        a.finalizeSpawn(level, level.getCurrentDifficultyAt(BlockPos.containing(x, y, z)), MobSpawnType.EVENT, null);
        boolean baby = vanillaAge < 0;
        CompoundTag t = new CompoundTag();
        a.addAdditionalSaveData(t);
        t.putInt("Age", vanillaAge);
        t.putInt("ForcedAge", 0);
        if (a instanceof Sheep) {
            t.putByte("Color", (byte) (variant & 0xF));
            t.putBoolean("Sheared", sheared && !baby);
        } else if (a instanceof Horse) {
            t.putInt("Variant", variant);
        } else if (a instanceof Goat) {
            t.putBoolean("IsScreamingGoat", variant == 1);
            // Vanilla takes one horn of an adult in ten at spawn (Goat.finalizeSpawn); here from the seed.
            boolean left = !baby, right = !baby;
            if (!baby && r.nextFloat() < 0.1f) {
                if (r.nextBoolean())
                    left = false;
                else
                    right = false;
            }
            t.putBoolean("HasLeftHorn", left);
            t.putBoolean("HasRightHorn", right);
        }
        a.readAdditionalSaveData(t);
        if (a instanceof AbstractHorse h) {
            // What Horse.randomizeAttributes draws (max health, speed, jump), from the member's seed.
            ((AbstractHorseInvoker) h).mic_rtsim$randomizeAttributes(RandomSource.create(seed));
        }
        a.setHealth(a.getMaxHealth());
        return a;
    }

    /** Capture flags: {@link HerdRecords#MEMBER_SHEARED} for a sheared sheep. */
    public static int flags(Entity e) {
        return e instanceof Sheep s && s.isSheared() ? HerdRecords.MEMBER_SHEARED : 0;
    }

    public static boolean sheared(Entity e) {
        return e instanceof Sheep s && s.isSheared();
    }

    /** The look a body shows now, as HERD_MATERIALIZE's variant (GameTests). */
    public static int variant(Entity e) {
        if (e instanceof Sheep s)
            return s.getColor().getId();
        if (e instanceof Horse h) {
            CompoundTag t = new CompoundTag();
            h.addAdditionalSaveData(t);
            return t.getInt("Variant");
        }
        if (e instanceof Goat g)
            return g.isScreamingGoat() ? 1 : 0;
        return 0;
    }

    /**
     * How a body left the sim by itself (A10), or 0: a lead on it (whoever holds it: a player, or a fence
     * knot a player tied it to), or taming (a horse a player broke in).
     */
    public static int takenBy(Entity e) {
        if (e instanceof Leashable l && l.isLeashed())
            return com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Taken.LEAD;
        if (e instanceof AbstractHorse h && h.isTamed())
            return com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs.Taken.TAME;
        return 0;
    }

    /** The player behind a taking: the lead's holder, or the tamer. Null when none is known. */
    public static UUID taker(Entity e) {
        if (e instanceof Leashable l && l.getLeashHolder() instanceof Player p)
            return p.getUUID();
        if (e instanceof OwnableEntity o)
            return o.getOwnerUUID();
        return null;
    }
}
