package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.simibubi.create.content.contraptions.Contraption;
import com.simibubi.create.content.contraptions.actors.psi.PortableStorageInterfaceBlockEntity;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A station's Portable Storage Interface docking with a contraption (PATCHES.md, Create rows; choices
 * SG30 and SG32). Create 6.0.10 docks a PSI when the moving one's active area enters it at 0.25 blocks
 * per tick or less, which a train does on its last block of braking, before {@code Train.arriveAt}: so
 * the cargo a train brought is read here, at the first dock, before anything has moved. Item and fluid
 * interfaces share this class. Only a new connection is reported ({@code connectedEntity} differs);
 * Create's own test, unchanged.
 */
@Mixin(value = PortableStorageInterfaceBlockEntity.class, remap = false)
public abstract class PortableStorageInterfaceMixin {
    @Shadow
    protected Entity connectedEntity;

    @Inject(method = "startTransferringTo", at = @At("HEAD"))
    private void mic_rtsim$docking(Contraption contraption, float distance, CallbackInfo ci) {
        if (contraption != null && connectedEntity != contraption.entity)
            TrainWatch.docking((PortableStorageInterfaceBlockEntity) (Object) this, contraption);
    }
}
