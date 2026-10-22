package com.minecraftindustrialcomplex.mic_rtsim.far;

/**
 * One far object as a server source sees it this second (docs/boundary.md §12.3): what it is, and either where
 * it is now (a sampled {@code seg}) or the leg it flies ({@code leg}), never both. Coordinates are its level's
 * blocks. Plain Java: the far feed's filter and its per-subscriber diff are unit tested on these.
 *
 * @param id       the object's id, unique across sources: an engine agent id, or a source's own namespace
 *                 (a carriage's is {@code 0x54 << 56} and up)
 * @param kind     what it is
 * @param style    the kind's sub-type (a dragon's species index)
 * @param variant  its colour variant, 0 to 3
 * @param size     how big (a dragon's stage, 1 to 5), or 0
 * @param rgb      the colour, 0xRRGGBB
 * @param state    what it is doing
 * @param seg      a sampled object's segment now (6 values, leading x, y, z then trailing), or null
 * @param leg      a flown leg, or null
 * @param entity   the real entity's network id, or -1 when it has no body in the level
 * @param priority which source wins when two give the same id (higher wins): a body beats the engine's leg
 */
public record FarItem(long id, FarKind kind, int style, int variant, int size, int rgb, FarState state,
        double[] seg, Leg leg, int entity, int priority) {

    public FarItem {
        if ((seg == null) == (leg == null))
            throw new IllegalArgumentException("a far item is sampled or flies a leg, not " + (seg == null ? "neither" : "both"));
        if (seg != null && seg.length != 6)
            throw new IllegalArgumentException("a segment is 6 values, got " + seg.length);
        if (state == null || state == FarState.GONE)
            throw new IllegalArgumentException("an item's state must be set and not GONE");
    }

    /**
     * A leg in game time: from (x0, y, z0) at t0 to (x1, y, z1) at t1 (a flight), or circling (x0, z0) at
     * {@code radius} until t1 (a hunt), or still at (x0, y, z0) (a roost: x1 = x0, z1 = z0).
     */
    public record Leg(int y, int x0, int z0, int x1, int z1, long t0, long t1, int radius) {
        /** Where it is at {@code gameTime}: {x, z}. */
        public double[] at(long gameTime) {
            if (t1 <= t0 || gameTime <= t0)
                return new double[]{x0, z0};
            if (gameTime >= t1)
                return new double[]{x1, z1};
            double f = (double) (gameTime - t0) / (double) (t1 - t0);
            return new double[]{x0 + (x1 - x0) * f, z0 + (z1 - z0) * f};
        }
    }

    /** Where it is at {@code gameTime}, x and z. */
    public double[] position(long gameTime) {
        if (seg != null)
            return new double[]{(seg[0] + seg[3]) / 2.0, (seg[2] + seg[5]) / 2.0};
        return leg.at(gameTime);
    }

    /** The same item with its coordinates moved (the image nearest a subscriber, through a seam). */
    public FarItem moved(com.minecraftindustrialcomplex.mic_rtsim.topology.Iso iso) {
        if (iso.isIdentity())
            return this;
        if (seg != null) {
            double[] l = iso.point(seg[0], seg[2]), t = iso.point(seg[3], seg[5]);
            return new FarItem(id, kind, style, variant, size, rgb, state,
                    new double[]{l[0], seg[1], l[1], t[0], seg[4], t[1]}, null, entity, priority);
        }
        // Leg ends are block coordinates: move the block centres and take the block back
        double[] a = iso.point(leg.x0 + 0.5, leg.z0 + 0.5), b = iso.point(leg.x1 + 0.5, leg.z1 + 0.5);
        Leg m = new Leg(leg.y, (int) Math.floor(a[0]), (int) Math.floor(a[1]), (int) Math.floor(b[0]),
                (int) Math.floor(b[1]), leg.t0, leg.t1, leg.radius);
        return new FarItem(id, kind, style, variant, size, rgb, state, null, m, entity, priority);
    }
}
