package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

import net.minecraft.world.level.block.entity.BlockEntity;

/**
 * A plugin's part of the glass box's compile (GLASSBOX.md §7.1, tier 3's Java extractor; G-B5): it turns
 * the block entities it knows into graph nodes, edges, recipes, kinetic domains and rate tables. Returned
 * by {@code IndustryPlugin.glass()}. Everything runs on the server thread at compile time only; the
 * engine never calls back (PLAN.md §3.2).
 *
 * <p>A compile is sliced over ticks ({@code glass.compileSliceMicros}): {@link Compile#visit} is offered
 * every block entity of the district's chunks one at a time, then {@link Compile#finish} is called once
 * per tick until it returns true. A compile is thrown away and restarted when the district is edited or
 * its fingerprint changes meanwhile, so neither may change the world.
 */
public interface GlassExtractor {

    /**
     * The compile-relevant configuration of a block entity this extractor knows (filters and their
     * counts, modes, scroll values, thresholds, an arm's points), hashed; 0 for none. Never contents,
     * progress or speeds: those are the model's state, not its structure (G-B3).
     */
    default long fingerprint(BlockEntity be) {
        return 0;
    }

    /** Starts one district's compile into {@code graph}. */
    Compile begin(GraphBuilder graph);

    interface Compile {
        /**
         * One block entity of the district's loaded chunks, in a stable order. Return true when this
         * extractor models it; an unclaimed one with an item, fluid or energy capability joins the
         * residual black box (G-B13), unless it is a boundary store.
         */
        boolean visit(BlockEntity be);

        /**
         * After every visit: edges, domains, recipes and rate tables. Called again on a later tick while
         * it returns false (its own slicing); {@code deadlineNanos} is the {@code System.nanoTime()} it
         * should stop by.
         */
        boolean finish(long deadlineNanos);
    }
}
