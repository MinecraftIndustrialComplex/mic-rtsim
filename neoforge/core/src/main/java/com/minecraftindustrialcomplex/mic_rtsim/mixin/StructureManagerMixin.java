package com.minecraftindustrialcomplex.mic_rtsim.mixin;

import com.minecraftindustrialcomplex.mic_rtsim.village.VillageAdoption;
import net.minecraft.core.SectionPos;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.StructureManager;
import net.minecraft.world.level.chunk.StructureAccess;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * A village joins the sim when its structure start generates (C22). {@code ChunkGenerator.tryGenerateStructure}
 * hands every valid start to {@code StructureManager.setStartForStructure} during the chunk's
 * STRUCTURE_STARTS step, on a worldgen thread (1.21.1 source). {@link VillageAdoption#generated} only
 * queues the starts of village structures; the server thread adopts them.
 */
@Mixin(StructureManager.class)
public abstract class StructureManagerMixin {
    @Shadow
    @Final
    private LevelAccessor level;

    @Inject(method = "setStartForStructure", at = @At("HEAD"))
    private void mic_rtsim$structureStarted(SectionPos sectionPos, Structure structure, StructureStart start, StructureAccess access, CallbackInfo ci) {
        VillageAdoption.generated(this.level, structure, start);
    }
}
