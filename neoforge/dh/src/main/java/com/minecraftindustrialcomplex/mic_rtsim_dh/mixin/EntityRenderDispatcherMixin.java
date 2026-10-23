package com.minecraftindustrialcomplex.mic_rtsim_dh.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_dh.FarHide;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.entity.EntityRenderDispatcher;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Leaves out a dragon's body that DH's vanilla fade would fade away while its far box is drawn in its place
 * (docs/choices-dh-feed.md DHT47, {@link FarHide#bodyHidden}). Vanilla's entity pass asks this before anything
 * else it draws an entity by, with the camera's position.
 */
@Mixin(EntityRenderDispatcher.class)
abstract class EntityRenderDispatcherMixin {
    @Inject(method = "shouldRender", at = @At("HEAD"), cancellable = true)
    private <E extends Entity> void mic_rtsim_dh$farBoxInstead(E entity, Frustum frustum, double camX, double camY,
            double camZ, CallbackInfoReturnable<Boolean> cir) {
        if (FarHide.bodyHidden(entity, camX, camY, camZ))
            cir.setReturnValue(false);
    }
}
