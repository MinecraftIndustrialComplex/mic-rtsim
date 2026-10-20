package com.minecraftindustrialcomplex.mic_rtsim.api;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;

/** How a creature left the sim (PLAN.md §6.2); the TAKEN observation's aux. */
public enum TakeReason {
    LEAD(Obs.Taken.LEAD),
    TAME(Obs.Taken.TAME),
    PICKUP(Obs.Taken.PICKUP),
    REMOVED_BY_OTHER(Obs.Taken.REMOVED_BY_OTHER),
    PLUGIN(Obs.Taken.PLUGIN);

    public final int code;

    TakeReason(int code) {
        this.code = code;
    }
}
