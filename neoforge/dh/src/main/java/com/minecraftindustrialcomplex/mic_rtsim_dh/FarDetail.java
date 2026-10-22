package com.minecraftindustrialcomplex.mic_rtsim_dh;

/**
 * The far view's detail tier, a client option (research/distant-horizons-2026-10-21.md §7). The group and box
 * budgets are that table's starting numbers; the phase 1 measurement (docs/choices-dh-trains.md) says what
 * they cost. Dragons are drawn first (docs/choices-dh-feed.md), then the nearest carriages, and the rest wait
 * until nearer ones go.
 *
 * <p>Plain Java: no DH or Minecraft class.
 */
public enum FarDetail {
    /** Nothing drawn and nothing asked of the server. */
    OFF(0, 0, 0, 1.0, 0, 0, 0),
    LOW(32, 96, 4_000, 2.0, 48, 8, 2),
    MEDIUM(96, 288, 12_000, 1.0, 128, 24, 3),
    HIGH(256, 768, 30_000, 1.0, 320, 64, 3);

    /** The most groups drawn at once: a carriage is one, a dragon two (its body and its wings). */
    public final int activeGroups;
    /** The most groups kept registered with DH, drawn or not (hidden ones wait, ready, near the player). */
    public final int registeredGroups;
    /** The most boxes drawn at once. */
    public final int boxes;
    /** The longest staircase step on a diagonal or slope, in blocks. */
    public final double step;
    /** The most objects asked of the server's far feed (its own cap per tier is the same). PLACEHOLDER. */
    public final int feedObjects;
    /** The most dragons drawn at once. PLACEHOLDER. */
    public final int dragons;
    /** Wing poses in a dragon's flipbook: two (up, down) or three (up, level, down). */
    public final int wingPoses;

    FarDetail(int activeGroups, int registeredGroups, int boxes, double step, int feedObjects, int dragons,
            int wingPoses) {
        this.activeGroups = activeGroups;
        this.registeredGroups = registeredGroups;
        this.boxes = boxes;
        this.step = step;
        this.feedObjects = feedObjects;
        this.dragons = dragons;
        this.wingPoses = wingPoses;
    }
}
