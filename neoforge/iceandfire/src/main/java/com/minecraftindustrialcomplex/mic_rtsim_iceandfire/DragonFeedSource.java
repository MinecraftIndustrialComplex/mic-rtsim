package com.minecraftindustrialcomplex.mic_rtsim_iceandfire;

import com.iafenvoy.iceandfire.entity.DragonBaseEntity;
import com.iafenvoy.iceandfire.entity.util.dragon.DragonSize;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarItem;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarLegs;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarSource;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import java.util.function.Consumer;
import net.minecraft.server.level.ServerLevel;

/**
 * Embodied dragons for the far feed (research/distant-horizons-2026-10-21.md §4.2, §5; docs/boundary.md
 * §12.3): each puppet's body where it is this second, under the engine's agent id, so the far view moves from
 * the engine's leg to the body and back without a seam (the hand-over). The body wins over the engine's leg
 * of the same id ({@link #PRIORITY}); it carries the entity's network id, so a client that has the entity
 * hides the proxy, and one that does not draws it. It carries the dragon's age in days, so a far renderer draws
 * it at its size between stages (docs/choices-dh-feed.md DHT60); the engine's legs say only the stage.
 *
 * <p>Its own file, apart from {@link DragonBridge}: the bridge only lends {@code forEachBody}.
 */
final class DragonFeedSource implements FarSource {
    /** Above the engine's legs (0). */
    static final int PRIORITY = 10;

    /** Ice and Fire's colours in its own order (IafDragonColors): fire, ice, lightning; four each. PLACEHOLDER shades. */
    static final int[][] PALETTE = {
            {0x8E1B1B, 0x2E5E2A, 0x8C6A2F, 0x5E5E5E},  // red, green, bronze, gray
            {0x6FA8C8, 0xD8E2E8, 0x2A4E9C, 0x9AA3AA},  // blue, white, sapphire, silver
            {0x2B3A8C, 0x7A4A9C, 0xA0603A, 0x222226}}; // electric, amethyst, copper, black

    /** Ice and Fire's hitbox height at scale 1 (IafEntities: 0.78 by 1.2 blocks for every species). */
    static final double BASE_HEIGHT = 1.2;

    /** Tells the core how each species looks, for the engine's legs too. */
    static void register() {
        double[] centres = centres();
        for (int s = 0; s < IafDragons.SPECIES.size(); s++)
            FarLegs.registerStyle(IafDragons.SPECIES.get(s), new FarLegs.Style(s, PALETTE[s], centres));
        com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi.registerFarSource(new DragonFeedSource());
    }

    /**
     * Feet to middle at each stage, 1 to 5: half the hitbox at the middle of the stage's growth (Ice and Fire's
     * {@code DragonBaseEntity.getAgeScale}, {@code min(renderSize * 0.35, 7)}, with its render size halfway
     * through {@link DragonSize}), so an engine leg is drawn where the body's samples put the dragon
     * (docs/choices-dh-feed.md DHT42). About 0.4, 1.1, 2.0, 3.4 and 4.2 blocks.
     */
    static double[] centres() {
        double[] out = new double[5];
        for (int s = 1; s <= out.length; s++) {
            DragonSize size = DragonSize.getSize(s);
            double renderSize = size.x0() + size.step() / 2.0;
            out[s - 1] = BASE_HEIGHT * Math.min(renderSize * 0.35, 7.0) / 2.0;
        }
        return out;
    }

    @Override
    public String name() {
        return "Ice and Fire dragon bodies";
    }

    @Override
    public void collect(ServerLevel level, long gameTime, Consumer<FarItem> out) {
        DragonBridge.INSTANCE.forEachBody((agent, e) -> {
            if (e.level() != level)
                return;
            DragonBaseEntity d = IafDragons.dragon(e);
            if (d == null || IafDragons.dead(d))
                return;
            int species = Math.max(0, IafDragons.species(e));
            int variant = IafDragons.variant(d);
            double yaw = Math.toRadians(e.getYRot());
            double hx = -Math.sin(yaw) * 0.5, hz = Math.cos(yaw) * 0.5;
            double x = e.getX(), y = e.getY() + e.getBbHeight() / 2.0, z = e.getZ();
            FarState state = IafDragons.flying(d) ? FarState.FLY : FarState.ROOST;
            out.accept(new FarItem(agent, FarKind.DRAGON, species, variant, d.getDragonStage(),
                    PALETTE[Math.min(species, PALETTE.length - 1)][Math.floorMod(variant, 4)], state,
                    new double[]{x + hx, y, z + hz, x - hx, y, z - hz}, null, e.getId(), PRIORITY, null,
                    Math.max(0, d.getAgeInDays())));
        });
    }
}
