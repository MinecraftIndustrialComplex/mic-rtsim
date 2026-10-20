package com.minecraftindustrialcomplex.mic_rtsim_endertransmission.mixin;

import com.forsteri.createendertransmission.blocks.AbstractMatterTransmitterBlockEntity;
import com.minecraftindustrialcomplex.mic_rtsim_endertransmission.ChannelWatch;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

import java.util.function.Supplier;

/**
 * Ender Transmission's transmitters are passive capability proxies onto a shared channel buffer: funnels,
 * pipes and hoppers move items and fluids through them, and the mod itself never ticks a transfer
 * (javap, docs/overnight-choices-indplugins.md Q7). Wrapping the block entity's public capability supplier
 * at the end of its constructor makes every handler a lookup returns a measuring one, so each transfer
 * through a transmitter in a district is a channel port (PATCHES.md).
 */
@Mixin(value = AbstractMatterTransmitterBlockEntity.class, remap = false)
public abstract class AbstractMatterTransmitterBlockEntityMixin {
    @SuppressWarnings("rawtypes")
    @Shadow
    public Supplier capability;

    @SuppressWarnings({"unchecked", "rawtypes"})
    @Inject(method = "<init>", at = @At("RETURN"))
    private void mic_rtsim$measure(BlockEntityType<?> type, BlockPos pos, BlockState state, CallbackInfo ci) {
        Supplier original = this.capability;
        AbstractMatterTransmitterBlockEntity<?> self = (AbstractMatterTransmitterBlockEntity<?>) (Object) this;
        this.capability = () -> ChannelWatch.wrap(self, original.get());
        ChannelWatch.track(self);
    }
}
