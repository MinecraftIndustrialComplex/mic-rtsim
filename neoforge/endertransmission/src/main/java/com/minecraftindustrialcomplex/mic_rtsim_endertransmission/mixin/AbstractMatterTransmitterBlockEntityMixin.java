package com.minecraftindustrialcomplex.mic_rtsim_endertransmission.mixin;

import com.forsteri.createendertransmission.blocks.AbstractMatterTransmitterBlockEntity;
import com.minecraftindustrialcomplex.mic_rtsim_endertransmission.ChannelLinks;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * Ender Transmission keeps a channel's buffer but no list of its item and fluid endpoints (javap,
 * docs/overnight-choices-indplugins.md Q7). Registering each transmitter block entity at the end of its
 * constructor, weakly, lets the district detection find the other endpoints of a channel (PATCHES.md).
 */
@Mixin(value = AbstractMatterTransmitterBlockEntity.class, remap = false)
public abstract class AbstractMatterTransmitterBlockEntityMixin {
    @Inject(method = "<init>", at = @At("RETURN"))
    private void mic_rtsim$track(BlockEntityType<?> type, BlockPos pos, BlockState state, CallbackInfo ci) {
        ChannelLinks.track((AbstractMatterTransmitterBlockEntity<?>) (Object) this);
    }
}
