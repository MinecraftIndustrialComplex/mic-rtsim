package com.minecraftindustrialcomplex.mic_rtsim_create.ringmixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.AppliedOpRing;
import com.minecraftindustrialcomplex.mic_rtsim_create.PortRing;
import com.minecraftindustrialcomplex.mic_rtsim_create.PortRingHolder;
import com.simibubi.create.content.trains.station.GlobalPackagePort;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;

/**
 * The station port's rings (docs/choices-journal-barrier.md RP-206): two fields on Create's
 * {@code GlobalPackagePort}, saved by {@link StationPortsRingMixin} in the port's entry of its station's
 * "Ports" list in create_tracks.dat, beside the offline buffer they guard. No injection: a field and an
 * interface cannot fail to apply on their own, and {@link PortRing#available} probes the pair.
 */
@Mixin(value = GlobalPackagePort.class, remap = false)
public abstract class PortRingMixin implements PortRingHolder {
    @Unique
    private AppliedOpRing mic_rtsim$buffer;
    @Unique
    private AppliedOpRing mic_rtsim$postbox;

    @Override
    public AppliedOpRing mic_rtsim$bufferOps() {
        if (mic_rtsim$buffer == null)
            mic_rtsim$buffer = new AppliedOpRing(PortRing.CAPACITY);
        return mic_rtsim$buffer;
    }

    @Override
    public AppliedOpRing mic_rtsim$bufferOpsIfAny() {
        return mic_rtsim$buffer;
    }

    @Override
    public AppliedOpRing mic_rtsim$postboxOps() {
        if (mic_rtsim$postbox == null)
            mic_rtsim$postbox = new AppliedOpRing(PortRing.CAPACITY);
        return mic_rtsim$postbox;
    }

    @Override
    public AppliedOpRing mic_rtsim$postboxOpsIfAny() {
        return mic_rtsim$postbox;
    }
}
