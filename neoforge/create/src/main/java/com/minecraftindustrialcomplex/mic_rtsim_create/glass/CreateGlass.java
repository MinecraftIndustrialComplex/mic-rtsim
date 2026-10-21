package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassExtractor;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GraphBuilder;
import com.simibubi.create.content.fluids.tank.FluidTankBlockEntity;
import com.simibubi.create.content.kinetics.belt.BeltBlockEntity;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.crafter.MechanicalCrafterBlockEntity;
import com.simibubi.create.content.kinetics.deployer.DeployerBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmInteractionPoint;
import com.simibubi.create.content.logistics.packagePort.PackagePortBlockEntity;
import com.simibubi.create.content.logistics.tunnel.BeltTunnelBlockEntity;
import com.simibubi.create.content.redstone.thresholdSwitch.ThresholdSwitchBlockEntity;
import com.simibubi.create.foundation.blockEntity.SmartBlockEntity;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.scrollValue.ScrollValueBehaviour;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.HopperBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;

import java.util.List;
import java.util.Set;

/**
 * The Create plugin's glass-box extractor ({@code IndustryPlugin.glass()}): the fingerprint of each
 * block entity's compile-relevant configuration (G-B3) and a {@link CreateCompile} per district compile.
 */
public final class CreateGlass implements GlassExtractor {

    public static final CreateGlass INSTANCE = new CreateGlass();

    private CreateGlass() {}

    /**
     * Block state properties that are the model's state, not its structure: redstone power, lit, a blaze
     * burner's heat, a hopper's enabled flag. A paused funnel or a burner running out is a regime the
     * engine follows from the stores and sweeps, not a new graph.
     */
    private static final Set<String> VOLATILE = Set.of("powered", "lit", "blaze", "enabled", "triggered", "open", "power",
            "waterlogged", "inverted_signal");

    @Override
    public long fingerprint(BlockEntity be) {
        String ns = net.minecraft.core.registries.BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(be.getType()) == null ? ""
                : net.minecraft.core.registries.BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(be.getType()).getNamespace();
        if (!ns.equals("create") && !(be instanceof HopperBlockEntity) && !(be instanceof net.minecraft.world.level.block.entity.AbstractFurnaceBlockEntity))
            return 0;
        long h = 0x51ED_2701L;
        BlockState s = be.getBlockState();
        for (Property<?> p : s.getProperties()) {
            if (VOLATILE.contains(p.getName()))
                continue;
            h = mix(h, p.getName().hashCode() * 31L + s.getValue(p).toString().hashCode());
        }
        if (be instanceof SmartBlockEntity sbe)
            for (BlockEntityBehaviour b : sbe.getAllBehaviours()) {
                if (b instanceof FilteringBehaviour f) {
                    h = mix(h, item(f.getFilter()));
                    h = mix(h, f.count * 2L + (f.upTo ? 1 : 0));
                } else if (b instanceof ScrollValueBehaviour v) {
                    h = mix(h, v.getValue());
                }
            }
        // which kinetic network a block is in (its source's position) is structure: a machine joins its
        // network a tick after it is placed, and a compile from before has it outside any domain
        if (be instanceof com.simibubi.create.content.kinetics.base.KineticBlockEntity k)
            h = mix(h, k.network == null ? 0 : k.network);
        if (be instanceof ArmBlockEntity a) {
            h = points(h, CreateGlassAccess.armInputs(a));
            h = mix(h, 0xA5);
            h = points(h, CreateGlassAccess.armOutputs(a));
        }
        if (be instanceof DeployerBlockEntity d) {
            String m = CreateGlassAccess.deployerMode(d);
            h = mix(h, m == null ? -1 : m.hashCode());
            h = mix(h, d.getPlayer() == null ? 0 : item(d.getPlayer().getMainHandItem()));
        }
        if (be instanceof ThresholdSwitchBlockEntity t) {
            h = mix(h, t.onWhenAbove);
            h = mix(h, t.offWhenBelow);
            h = mix(h, t.isInverted() ? 1 : 0);
            h = mix(h, t.inStacks ? 1 : 0);
        }
        if (be instanceof BeltBlockEntity b) {
            h = mix(h, b.beltLength);
            h = mix(h, b.getController() == null ? 0 : b.getController().asLong());
        }
        if (be instanceof BeltTunnelBlockEntity t) {
            for (Direction d : Direction.values())
                h = mix(h, (t.sides.contains(d) ? 1 : 0) + (t.flaps.containsKey(d) ? 2 : 0));
        }
        if (be instanceof FluidTankBlockEntity t) {
            h = mix(h, t.getController() == null ? 0 : t.getController().asLong());
            h = mix(h, t.getTotalTankSize());
            FluidTankBlockEntity c = t.getControllerBE();
            h = mix(h, c == null || c.boiler == null ? 0 : c.boiler.attachedEngines);
        }
        if (be instanceof MechanicalCrafterBlockEntity m)
            h = mix(h, m.craftingItemOrCoverPresent() && !m.craftingItemPresent() ? 1 : 0);
        if (be instanceof PackagePortBlockEntity p) {
            h = mix(h, p.addressFilter == null ? 0 : p.addressFilter.hashCode());
            h = mix(h, p.target == null ? 0 : p.target.relativePos.asLong());
        }
        if (be instanceof ChainConveyorBlockEntity c) {
            long sum = 0;
            for (BlockPos rel : c.connections)
                sum += mix(rel.asLong(), 0x3C);
            h = mix(h, sum);
        }
        return h;
    }

    private static long points(long h, List<ArmInteractionPoint> points) {
        if (points == null)
            return mix(h, -1);
        for (ArmInteractionPoint p : points)
            h = mix(h, p.getPos().asLong());
        return h;
    }

    /** An item and its components, without the count. */
    private static long item(ItemStack s) {
        if (s.isEmpty())
            return 0;
        return ItemStack.hashItemAndComponents(s);
    }

    private static long mix(long h, long v) {
        long z = h * 0x9E3779B97F4A7C15L + v;
        z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
        z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
        return z ^ (z >>> 31);
    }

    @Override
    public Compile begin(GraphBuilder graph) {
        return new CreateCompile(graph);
    }
}
