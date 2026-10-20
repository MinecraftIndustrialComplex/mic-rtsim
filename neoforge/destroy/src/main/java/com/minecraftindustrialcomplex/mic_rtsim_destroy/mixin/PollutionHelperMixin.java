package com.minecraftindustrialcomplex.mic_rtsim_destroy.mixin;

import com.minecraftindustrialcomplex.mic_rtsim_destroy.PollutionLedger;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.Level;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;
import petrolpark.mc.destroy.core.pollution.PollutionHelper;
import petrolpark.mc.destroy.core.pollution.PollutionType;

/**
 * I14's attribution (INDUSTRY.md §12, PATCHES.md): every emission in Destroy 0.4.1 goes through
 * {@code PollutionHelper.changePollution(Level, BlockPos, PollutionType, int)} ({@code pollute} and its
 * fractional and mixture paths call it; javap). The value before and after each call is read, and the
 * change is booked to the district at that position ({@link PollutionLedger}).
 */
@Mixin(value = PollutionHelper.class, remap = false)
public abstract class PollutionHelperMixin {
    private static final String TARGET =
            "changePollution(Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;Lpetrolpark/mc/destroy/core/pollution/PollutionType;I)I";

    @Inject(method = TARGET, at = @At("HEAD"))
    private static void mic_rtsim$before(Level level, BlockPos pos, PollutionType<?> type, int change, CallbackInfoReturnable<Integer> cir) {
        PollutionLedger.before(level, pos, type);
    }

    @Inject(method = TARGET, at = @At("RETURN"))
    private static void mic_rtsim$after(Level level, BlockPos pos, PollutionType<?> type, int change, CallbackInfoReturnable<Integer> cir) {
        PollutionLedger.after(level, pos, type);
    }
}
