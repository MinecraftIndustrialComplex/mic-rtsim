package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.mixin;

import com.bobmowzie.mowziesmobs.server.ability.Ability;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.Level;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * The summons of an Umvuthi that floats with its Invasion (M11, MZB-816). The jar's {@code SpawnFollowersAbility} makes each
 * warrior or crane and adds it with one {@code Level.addFreshEntity}; this wraps that call. When the summoner is a live chief puppet
 * of a band, the summons are adopted before the join as its slot's extras (tagged, held, listed, never saved, booking nothing when
 * they die), which the core's join check admits; a join another mod cancels is undone. For any other Umvuthi (a grove's own,
 * at its throne) the call is untouched and its summons are strays as ever. The second of the module's two Umvuthi mixins
 * with {@code EntityUmvuthiMixin}, besides the direction accessor (D3).
 */
@Mixin(targets = "com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi$SpawnFollowersAbility")
public abstract class UmvuthiSummonsMixin {

    @WrapOperation(method = "beginSection", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/world/level/Level;addFreshEntity(Lnet/minecraft/world/entity/Entity;)Z"), allow = 1)
    private boolean mic_rtsim_mowziesmobs$summons(Level level, Entity summons, Operation<Boolean> original) {
        LivingEntity user = ((Ability<?>) (Object) this).getUser();
        if (level.isClientSide() || !(user instanceof EntityUmvuthi) || !(summons instanceof Mob mob)
                || !RtsimApi.adoptSummons(user, mob))
            return original.call(level, summons);
        boolean in = original.call(level, summons);
        if (!in)
            RtsimApi.dropSummons(mob);
        return in;
    }
}
