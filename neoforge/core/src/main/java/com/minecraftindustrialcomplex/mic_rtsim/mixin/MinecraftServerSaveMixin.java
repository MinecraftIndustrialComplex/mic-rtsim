package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.minecraftindustrialcomplex.mic_rtsim.journal.WorldSaveOrder;
import net.minecraft.server.MinecraftServer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * Every world save's start and return (PATCHES.md row 86): the journal's file is written last, behind every level's
 * SavedData ({@link WorldSaveOrder}, docs/choices-journal-barrier.md RP-200). Vanilla's save runs unchanged between.
 * Only on the server thread: a call from another thread (C2ME sends one back to the server thread) is left alone.
 */
@Mixin(MinecraftServer.class)
public abstract class MinecraftServerSaveMixin {
    @Inject(method = "saveAllChunks", at = @At("HEAD"))
    private void mic_rtsim$saveBegins(boolean suppressLog, boolean flush, boolean forced, CallbackInfoReturnable<Boolean> cir) {
        MinecraftServer server = (MinecraftServer) (Object) this;
        if (server.isSameThread())
            WorldSaveOrder.begin(server);
    }

    @Inject(method = "saveAllChunks", at = @At("RETURN"))
    private void mic_rtsim$saveReturns(boolean suppressLog, boolean flush, boolean forced, CallbackInfoReturnable<Boolean> cir) {
        if (((MinecraftServer) (Object) this).isSameThread())
            WorldSaveOrder.end(flush);
    }
}
