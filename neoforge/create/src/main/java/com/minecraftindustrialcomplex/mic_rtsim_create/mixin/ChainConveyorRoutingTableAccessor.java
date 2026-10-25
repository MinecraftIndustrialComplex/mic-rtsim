package com.minecraftindustrialcomplex.mic_rtsim_create.mixin;

import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorRoutingTable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/**
 * {@code ChainConveyorRoutingTable.insert} (private): Create's own insert of a route entry, which refreshes an
 * identical entry's timeout, keeps a nearer one of the same port, and else adds it in distance order. The
 * frontier inserts its kept routes through it (SECTIONS.md §2.5, choice SC135; PATCHES.md row 79).
 */
@Mixin(value = ChainConveyorRoutingTable.class, remap = false)
public interface ChainConveyorRoutingTableAccessor {
    @Invoker("insert")
    void mic_rtsim$insert(ChainConveyorRoutingTable.RoutingTableEntry entry);
}
