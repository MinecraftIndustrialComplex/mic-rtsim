package com.minecraftindustrialcomplex.mic_rtsim_create;

/**
 * What {@code ringmixin.TrainRingMixin} adds to Create's {@code Train} (RP-190): the train's ring of applied
 * IND_CARRIAGE_DELTA op ids. Outside the mixin package, so the plugin's own code can name it; whether
 * {@code Train} implements it is the first half of {@link TrainRing#available}.
 */
public interface AppliedOpsHolder {
    /** The train's ring, made on first use. */
    AppliedOpRing mic_rtsim$appliedOps();

    /** The train's ring, or null when it never had one (a train that took no carriage delta). */
    AppliedOpRing mic_rtsim$appliedOpsIfAny();
}
