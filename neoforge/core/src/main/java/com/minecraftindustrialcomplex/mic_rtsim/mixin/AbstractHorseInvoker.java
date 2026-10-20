package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.animal.horse.AbstractHorse;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/**
 * {@code AbstractHorse.randomizeAttributes(RandomSource)} is protected, and vanilla feeds it the level's
 * random in {@code finalizeSpawn}: a herd horse rolls its max health, speed and jump strength from its
 * member's identity seed instead, so the same horse comes back with the same stats (docs/boundary.md
 * §8.2, choice Y12). PATCHES.md row 21.
 */
@Mixin(AbstractHorse.class)
public interface AbstractHorseInvoker {
    @Invoker("randomizeAttributes")
    void mic_rtsim$randomizeAttributes(RandomSource random);
}
