package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.BlockPos;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.level.block.state.BlockState;

/**
 * A block a reconcile removed (C18, I23): where, its state before, and the block-entity data a rebuild may
 * carry (a plugin's {@link IndustryPlugin#rebuildData}, else null).
 */
public record DestroyedBlock(BlockPos pos, BlockState state, CompoundTag rebuildData) {}
