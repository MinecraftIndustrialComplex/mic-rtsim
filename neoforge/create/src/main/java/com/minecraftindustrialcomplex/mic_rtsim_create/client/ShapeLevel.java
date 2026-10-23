package com.minecraftindustrialcomplex.mic_rtsim_create.client;

import com.minecraftindustrialcomplex.mic_rtsim.far.CarriageShape;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.BiomeColors;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.level.BlockAndTintGetter;
import net.minecraft.world.level.ColorResolver;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.Biomes;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.lighting.LevelLightEngine;
import net.minecraft.world.level.material.FluidState;

/**
 * A far carriage's blocks as a tiny level for the block renderer (docs/choices-dh-feed.md DHT64): it reads
 * neighbours from it, so faces between two blocks of the carriage are culled and shaded the way a chunk mesh
 * does it. Light is open sky and no block light (the far view has no light data for a carriage; the lightmap
 * then darkens it at night as it does DH's terrain); tints are plains'. A state id this client does not know
 * is air.
 */
final class ShapeLevel implements BlockAndTintGetter {
    private final Long2ObjectOpenHashMap<BlockState> blocks = new Long2ObjectOpenHashMap<>();
    private final Biome plains;

    ShapeLevel(CarriageShape shape) {
        for (CarriageShape.Block b : shape.blocks())
            blocks.put(BlockPos.asLong(b.x(), b.y(), b.z()), Block.stateById(b.state()));
        var level = Minecraft.getInstance().level;
        plains = level == null ? null : level.registryAccess().registryOrThrow(Registries.BIOME).get(Biomes.PLAINS);
    }

    @Override
    public BlockState getBlockState(BlockPos pos) {
        BlockState s = blocks.get(pos.asLong());
        return s == null ? Blocks.AIR.defaultBlockState() : s;
    }

    @Override
    public FluidState getFluidState(BlockPos pos) {
        return getBlockState(pos).getFluidState();
    }

    @Override
    public BlockEntity getBlockEntity(BlockPos pos) {
        return null;
    }

    @Override
    public float getShade(Direction direction, boolean shade) {
        if (!shade)
            return 1.0F;
        return switch (direction) {
            case DOWN -> 0.5F;
            case UP -> 1.0F;
            case NORTH, SOUTH -> 0.8F;
            case WEST, EAST -> 0.6F;
        };
    }

    @Override
    public LevelLightEngine getLightEngine() {
        return null;
    }

    @Override
    public int getBrightness(LightLayer layer, BlockPos pos) {
        return layer == LightLayer.SKY ? 15 : 0;
    }

    @Override
    public int getRawBrightness(BlockPos pos, int amount) {
        return Math.max(0, 15 - amount);
    }

    @Override
    public boolean canSeeSky(BlockPos pos) {
        return true;
    }

    @Override
    public int getBlockTint(BlockPos pos, ColorResolver resolver) {
        if (plains != null)
            return resolver.getColor(plains, pos.getX(), pos.getZ());
        return resolver == BiomeColors.WATER_COLOR_RESOLVER ? 0x3F76E4 : 0x79C05A;
    }

    @Override
    public int getHeight() {
        return 384;
    }

    @Override
    public int getMinBuildHeight() {
        return -64;
    }
}
