package com.minecraftindustrialcomplex.mic_rtsim.far;

/**
 * What a far mover is doing, so a renderer can pose it (a dragon's wings): FAR_LEG's {@code aux} and the far
 * feed's state byte (docs/boundary.md §12). {@link #MOVING} is a sampled mover with nothing more to say (a
 * carriage).
 */
public enum FarState {
    /** Gone: forgotten. Never on a mover in the registry. */
    GONE(0),
    /** Flying a straight leg. */
    FLY(1),
    /** Circling a place ({@link FarMover#circle()}). */
    HUNT(2),
    /** At rest: a dragon folds its wings. */
    ROOST(3),
    /** A body in the game, sampled where the server sees it. */
    BODY(4),
    /** Sampled, with no pose of its own (a carriage). */
    MOVING(5);

    public final int wire;

    FarState(int wire) {
        this.wire = wire;
    }

    /** The state {@code wire} numbers, or null. */
    public static FarState ofWire(int wire) {
        for (FarState s : values())
            if (s.wire == wire)
                return s;
        return null;
    }
}
