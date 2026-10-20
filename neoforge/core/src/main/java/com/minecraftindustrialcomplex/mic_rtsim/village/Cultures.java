package com.minecraftindustrialcomplex.mic_rtsim.village;

import net.minecraft.core.Holder;
import net.minecraft.tags.StructureTags;
import net.minecraft.world.level.levelgen.structure.Structure;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Predicate;

/**
 * Which structures are villages, and of which culture (C6, C9, C22). The culture names are the ones
 * rtsim-settle uses ({@code human}, {@code ribbit}); the engine maps them to currencies and data.
 * The core knows vanilla's {@code #minecraft:village} tag; a plugin adds its mod's villages
 * ({@code mic_rtsim_ribbits} adds {@code ribbits:ribbit_village}).
 */
public final class Cultures {
    private Cultures() {}

    public static final String HUMAN = "human";

    /**
     * One rule: the structures it matches, their culture, and the template pool a house's resident
     * slot belongs to (a jigsaw block in a house template whose pool is this one marks where a
     * resident of the house stands), or null.
     */
    public record Rule(Predicate<Holder<Structure>> structures, String culture, String residentPool) {}

    // Copy on write: rules are registered during mod construction and read without a lock.
    private static volatile List<Rule> RULES = List.of(new Rule(h -> h.is(StructureTags.VILLAGE), HUMAN, null));

    public static synchronized void register(Rule rule) {
        List<Rule> l = new ArrayList<>(RULES);
        l.add(rule);
        RULES = List.copyOf(l);
    }

    /** The rule for a structure, or null when it is not a village the sim adopts. Later rules win. */
    public static Rule match(Holder<Structure> structure) {
        List<Rule> rules = RULES;
        for (int i = rules.size() - 1; i >= 0; i--)
            if (rules.get(i).structures().test(structure))
                return rules.get(i);
        return null;
    }

    /** The resident pool of a culture, or null. */
    public static String residentPool(String culture) {
        List<Rule> rules = RULES;
        for (int i = rules.size() - 1; i >= 0; i--)
            if (rules.get(i).culture().equals(culture) && rules.get(i).residentPool() != null)
                return rules.get(i).residentPool();
        return null;
    }
}
