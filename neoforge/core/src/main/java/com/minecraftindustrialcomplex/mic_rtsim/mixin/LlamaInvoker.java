package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.world.entity.animal.horse.Llama;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** {@code Llama.setStrength(int)} is private; a captured llama is respawned with its own strength. */
@Mixin(Llama.class)
public interface LlamaInvoker {
    @Invoker("setStrength")
    void mic_rtsim$setStrength(int strength);
}
