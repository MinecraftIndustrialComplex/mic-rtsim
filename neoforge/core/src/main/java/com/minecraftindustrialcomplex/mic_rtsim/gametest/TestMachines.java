package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.BaseEntityBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.RenderShape;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityTicker;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.MapColor;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.capabilities.RegisterCapabilitiesEvent;
import net.neoforged.neoforge.energy.EnergyStorage;
import net.neoforged.neoforge.items.ItemStackHandler;
import net.neoforged.neoforge.registries.DeferredBlock;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredRegister;

import java.util.function.Supplier;

/**
 * Blocks for the core's GameTests, registered only when {@code mic_rtsim.gametest} is set (the core's
 * GameTest server; no shipped build has the property). The core's server has no pack mod, so these are the
 * non-vanilla block entities the district seed rule (DS-10) needs to be tested on:
 * <ul>
 *   <li>{@code test_machine}: ticks on the server and has an energy capability: a machine by the generic rule;</li>
 *   <li>{@code test_inert}: an energy capability and no ticker: not a machine (a battery-like block);</li>
 *   <li>{@code test_store}: ticks and has an item capability, but is in {@code #mic_rtsim:district_not_seed}: storage;</li>
 *   <li>{@code test_marker}: no ticker and no capability, but in {@code #mic_rtsim:district_seed}: a machine by tag;</li>
 *   <li>{@code test_furnace}: a modded furnace (an {@code AbstractFurnaceBlockEntity}) that ticks and has an item
 *       capability, in no tag: the generic rule would take it for a machine, the furnace rule does not.</li>
 * </ul>
 */
public final class TestMachines {
    private TestMachines() {}

    private static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks(MicRtsim.MODID);
    private static final DeferredRegister<BlockEntityType<?>> TYPES = DeferredRegister.create(Registries.BLOCK_ENTITY_TYPE, MicRtsim.MODID);

    private enum Kind { MACHINE, INERT, STORE, MARKER }

    private static final class Entity extends BlockEntity {
        final EnergyStorage energy = new EnergyStorage(1000);
        final ItemStackHandler items = new ItemStackHandler(9);

        Entity(BlockEntityType<?> type, BlockPos pos, BlockState state) {
            super(type, pos, state);
        }
    }

    private static final class TestBlock extends BaseEntityBlock {
        private final Kind kind;
        private final Supplier<BlockEntityType<Entity>> type;

        TestBlock(Kind kind, Supplier<BlockEntityType<Entity>> type) {
            super(BlockBehaviour.Properties.of().mapColor(MapColor.METAL).strength(1.0f));
            this.kind = kind;
            this.type = type;
        }

        @Override
        protected MapCodec<? extends BaseEntityBlock> codec() {
            return simpleCodec(p -> new TestBlock(kind, type));
        }

        @Override
        public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
            return new Entity(type.get(), pos, state);
        }

        @Override
        public <T extends BlockEntity> BlockEntityTicker<T> getTicker(Level level, BlockState state, BlockEntityType<T> t) {
            if (level.isClientSide || (kind != Kind.MACHINE && kind != Kind.STORE))
                return null;
            return (l, p, s, be) -> {};
        }

        @Override
        protected RenderShape getRenderShape(BlockState state) {
            return RenderShape.MODEL;
        }
    }

    private static final class FurnaceEntity extends net.minecraft.world.level.block.entity.AbstractFurnaceBlockEntity {
        FurnaceEntity(BlockPos pos, BlockState state) {
            super(FURNACE_TYPE.get(), pos, state, net.minecraft.world.item.crafting.RecipeType.SMELTING);
        }

        @Override
        protected net.minecraft.network.chat.Component getDefaultName() {
            return net.minecraft.network.chat.Component.literal("test furnace");
        }

        @Override
        protected net.minecraft.world.inventory.AbstractContainerMenu createMenu(int id, net.minecraft.world.entity.player.Inventory inventory) {
            return null;
        }
    }

    private static final class FurnaceBlock extends BaseEntityBlock {
        FurnaceBlock() {
            super(BlockBehaviour.Properties.of().mapColor(MapColor.STONE).strength(1.0f));
        }

        @Override
        protected MapCodec<? extends BaseEntityBlock> codec() {
            return simpleCodec(p -> new FurnaceBlock());
        }

        @Override
        public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
            return new FurnaceEntity(pos, state);
        }

        @Override
        public <T extends BlockEntity> BlockEntityTicker<T> getTicker(Level level, BlockState state, BlockEntityType<T> t) {
            return level.isClientSide ? null : (l, p, s, be) -> {};
        }

        @Override
        protected RenderShape getRenderShape(BlockState state) {
            return RenderShape.MODEL;
        }
    }

    public static DeferredBlock<Block> MACHINE, INERT, STORE, MARKER, FURNACE;
    public static DeferredHolder<BlockEntityType<?>, BlockEntityType<FurnaceEntity>> FURNACE_TYPE;
    public static DeferredHolder<BlockEntityType<?>, BlockEntityType<Entity>> MACHINE_TYPE, INERT_TYPE, STORE_TYPE, MARKER_TYPE;

    /** Called from the mod constructor: registers the blocks when this is the core's GameTest server. */
    public static void register(IEventBus modBus) {
        if (!Boolean.getBoolean("mic_rtsim.gametest"))
            return;
        MACHINE_TYPE = type("test_machine", () -> MACHINE.get());
        INERT_TYPE = type("test_inert", () -> INERT.get());
        STORE_TYPE = type("test_store", () -> STORE.get());
        MARKER_TYPE = type("test_marker", () -> MARKER.get());
        MACHINE = BLOCKS.register("test_machine", () -> new TestBlock(Kind.MACHINE, MACHINE_TYPE));
        INERT = BLOCKS.register("test_inert", () -> new TestBlock(Kind.INERT, INERT_TYPE));
        STORE = BLOCKS.register("test_store", () -> new TestBlock(Kind.STORE, STORE_TYPE));
        MARKER = BLOCKS.register("test_marker", () -> new TestBlock(Kind.MARKER, MARKER_TYPE));
        FURNACE = BLOCKS.register("test_furnace", FurnaceBlock::new);
        FURNACE_TYPE = TYPES.register("test_furnace", () -> BlockEntityType.Builder.of(FurnaceEntity::new, FURNACE.get()).build(null));
        BLOCKS.register(modBus);
        TYPES.register(modBus);
        modBus.addListener(TestMachines::capabilities);
    }

    private static DeferredHolder<BlockEntityType<?>, BlockEntityType<Entity>> type(String name, Supplier<Block> block) {
        DeferredHolder<BlockEntityType<?>, BlockEntityType<Entity>>[] self = new DeferredHolder[1];
        self[0] = TYPES.register(name, () -> BlockEntityType.Builder.<Entity>of((pos, state) -> new Entity(self[0].get(), pos, state), block.get()).build(null));
        return self[0];
    }

    private static void capabilities(RegisterCapabilitiesEvent event) {
        event.registerBlockEntity(Capabilities.EnergyStorage.BLOCK, MACHINE_TYPE.get(), (be, side) -> be.energy);
        event.registerBlockEntity(Capabilities.EnergyStorage.BLOCK, INERT_TYPE.get(), (be, side) -> be.energy);
        event.registerBlockEntity(Capabilities.ItemHandler.BLOCK, STORE_TYPE.get(), (be, side) -> be.items);
        event.registerBlockEntity(Capabilities.ItemHandler.BLOCK, FURNACE_TYPE.get(), (be, side) -> new net.neoforged.neoforge.items.wrapper.InvWrapper(be));
    }
}
