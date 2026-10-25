package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.minecraftindustrialcomplex.mic_rtsim.bridge.ReinstallGuard;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.neoforged.neoforge.common.CommonHooks;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * NeoForge's read of {@code level.dat}'s mod list at a world's start (PATCHES.md row 47): the reinstall guard sees
 * what the last save listed before the start rewrites it ({@link ReinstallGuard}, UR-1). Reads only; NeoForge's own
 * handling (the mod mismatch event) runs unchanged after it.
 */
@Mixin(CommonHooks.class)
public abstract class CommonHooksLevelDatMixin {
    @Inject(method = "readAdditionalLevelSaveData", at = @At("HEAD"))
    private static void mic_rtsim$levelDatRead(CompoundTag rootTag, LevelStorageSource.LevelDirectory levelDirectory, CallbackInfo ci) {
        ReinstallGuard.levelDatRead(rootTag, levelDirectory.path());
    }
}
