package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin;

import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/**
 * The Umvuthi's private {@code direction} (MZB-813), 1 to 4 (its yaw is {@code (direction - 1) * 90}, set from it
 * every tick). The jar copies the synced value into the field only at {@code tickCount} 1, so a body that must turn
 * after that sets both. Not {@code ServerEntity}'s anything: the design's "accessor on ServerEntity" named a field
 * that class does not have. The module's other Umvuthi mixin is {@code EntityUmvuthiMixin}, hence the name.
 */
@Mixin(EntityUmvuthi.class)
public interface EntityUmvuthiAccessor {
    @Accessor(value = "direction", remap = false)
    int mic_rtsim_mowziesmobs$direction();

    @Accessor(value = "direction", remap = false)
    void mic_rtsim_mowziesmobs$setDirection(int direction);
}
