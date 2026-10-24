package com.minecraftindustrialcomplex.mic_rtsim.industry;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.tags.TagKey;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.FlyingMob;
import net.minecraft.world.entity.OwnableEntity;
import net.minecraft.world.entity.animal.FlyingAnimal;
import net.minecraft.world.entity.animal.IronGolem;
import net.minecraft.world.entity.monster.AbstractSkeleton;
import net.minecraft.world.entity.monster.Creeper;
import net.minecraft.world.entity.monster.Enemy;
import net.minecraft.world.entity.monster.RangedAttackMob;
import net.minecraft.world.level.block.Block;

/**
 * Threat classes of entities (docs/industry-boundary.md §3.2: zombies 0, skeletons 1, phantoms 2,
 * creepers 3; "the full table is the bridge's"). Data first: the entity type tags
 * {@code #mic_rtsim:threat/ground_melee}, {@code ground_ranged}, {@code flying} and {@code siege} win, so a
 * pack can classify a modded mob; otherwise the vanilla class decides (choice Q11).
 */
public final class ThreatClasses {
    private ThreatClasses() {}

    public static final TagKey<EntityType<?>> MELEE = tag("threat/ground_melee");
    public static final TagKey<EntityType<?>> RANGED = tag("threat/ground_ranged");
    public static final TagKey<EntityType<?>> FLYING = tag("threat/flying");
    public static final TagKey<EntityType<?>> SIEGE = tag("threat/siege");
    /**
     * Never a threat, whatever its class (MOWZIE.md §4.15, ledger 20): a mob that never moves (Mowzie's
     * Foliaath) is no pressure on a district. Checked first.
     */
    public static final TagKey<EntityType<?>> IGNORED = tag("threat/ignored");

    /**
     * Blocks a burrowing attacker cannot come up through (OILGATE.md §4.6, OG-9e): a machine standing on
     * one is armoured (the survey's {@code FLAG_ARMOURED}), and an oil nest's worm stops under it.
     */
    public static final TagKey<Block> ARMOURED_FLOOR = TagKey.create(Registries.BLOCK,
            ResourceLocation.fromNamespaceAndPath(MicRtsim.MODID, "armoured_floor"));
    /**
     * Block entities a burrowing attacker leaves alone wherever they stand (OG-9e): a well's own blocks,
     * since the nest is after the machines and the well is what feeds it. Surveyed as armoured.
     */
    public static final TagKey<Block> BURROW_SPARED = TagKey.create(Registries.BLOCK,
            ResourceLocation.fromNamespaceAndPath(MicRtsim.MODID, "burrow_spared"));

    private static TagKey<EntityType<?>> tag(String path) {
        return TagKey.create(Registries.ENTITY_TYPE, ResourceLocation.fromNamespaceAndPath(MicRtsim.MODID, path));
    }

    /**
     * True for an entity that attacks districts: a hostile mob ({@link Enemy}), or one tagged with a class;
     * never one tagged {@link #IGNORED}.
     */
    public static boolean hostile(Entity e) {
        EntityType<?> t = e.getType();
        if (t.is(IGNORED))
            return false;
        return e instanceof Enemy || t.is(MELEE) || t.is(RANGED) || t.is(FLYING) || t.is(SIEGE);
    }

    /** The threat class of a hostile entity ({@code IndRecords.ThreatClass}). */
    public static int classOf(Entity e) {
        EntityType<?> t = e.getType();
        if (t.is(SIEGE))
            return IndRecords.ThreatClass.SIEGE;
        if (t.is(FLYING))
            return IndRecords.ThreatClass.FLYING;
        if (t.is(RANGED))
            return IndRecords.ThreatClass.GROUND_RANGED;
        if (t.is(MELEE))
            return IndRecords.ThreatClass.GROUND_MELEE;
        if (e instanceof Creeper)
            return IndRecords.ThreatClass.SIEGE;
        if (e instanceof FlyingMob || e instanceof FlyingAnimal)
            return IndRecords.ThreatClass.FLYING;
        if (e instanceof AbstractSkeleton || e instanceof RangedAttackMob)
            return IndRecords.ThreatClass.GROUND_RANGED;
        return IndRecords.ThreatClass.GROUND_MELEE;
    }

    /**
     * A guard (I21): an entity a player owns or made that fights for the district: a player-built iron
     * golem, or a tamed animal (wolves, Ice and Fire's tamed dragons through {@link OwnableEntity}).
     */
    public static boolean guard(Entity e) {
        if (e instanceof IronGolem g)
            return g.isPlayerCreated();
        return e instanceof OwnableEntity o && o.getOwnerUUID() != null;
    }

    /**
     * Which classes a guard can fight: every guard fights walkers and sieges; only flying or ranged guards
     * reach flyers. Bit per {@code IndRecords.ThreatClass}.
     */
    public static int guardClasses(Entity e) {
        int bits = (1 << IndRecords.ThreatClass.GROUND_MELEE) | (1 << IndRecords.ThreatClass.GROUND_RANGED)
                | (1 << IndRecords.ThreatClass.SIEGE);
        if (e instanceof FlyingAnimal || e instanceof FlyingMob || e instanceof RangedAttackMob)
            bits |= 1 << IndRecords.ThreatClass.FLYING;
        return bits;
    }
}
