package com.minecraftindustrialcomplex.mic_rtsim_dh;

/**
 * The far view's detail tier, a client option (research/distant-horizons-2026-10-21.md §7). The group and box
 * budgets are that table's starting numbers; the phase 1 measurement (docs/choices-dh-trains.md) says what
 * they cost. The nearest carriages are drawn first, and the rest wait until nearer ones go.
 *
 * <p>Plain Java: no DH or Minecraft class.
 */
public enum FarDetail {
    /** Nothing drawn and nothing asked of the server. */
    OFF(0, 0, 0, 1.0),
    LOW(32, 96, 4_000, 2.0),
    MEDIUM(96, 288, 12_000, 1.0),
    HIGH(256, 768, 30_000, 1.0);

    /** The most carriages drawn at once (one DH group each). */
    public final int activeGroups;
    /** The most groups kept registered with DH, drawn or not (hidden ones wait, ready, near the player). */
    public final int registeredGroups;
    /** The most boxes drawn at once. */
    public final int boxes;
    /** The longest staircase step on a diagonal or slope, in blocks. */
    public final double step;

    FarDetail(int activeGroups, int registeredGroups, int boxes, double step) {
        this.activeGroups = activeGroups;
        this.registeredGroups = registeredGroups;
        this.boxes = boxes;
        this.step = step;
    }
}
