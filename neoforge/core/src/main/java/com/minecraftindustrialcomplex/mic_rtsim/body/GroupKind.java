package com.minecraftindustrialcomplex.mic_rtsim.body;

import net.minecraft.world.phys.Vec3;

import java.util.List;

/**
 * A group body kind (C25, PLAN.md §1.4): its member kinds and where each stands at spawn, relative to
 * the anchor the engine gives. Member 0 is the leader. The layout is used only at spawn; the engine
 * has no formations.
 *
 * <p>Built in code for now; the data file format for body kinds and group layouts is Proposal P15.
 */
public record GroupKind(String name, List<Slot> slots) {

    /** One member slot: its body adapter and its offset from the anchor. */
    public record Slot(BodyAdapter adapter, Vec3 offset) {}

    public int size() {
        return slots.size();
    }

    /** A group of one body, standing on the anchor. */
    public static GroupKind single(BodyAdapter adapter) {
        return new GroupKind(adapter.name(), List.of(new Slot(adapter, Vec3.ZERO)));
    }
}
