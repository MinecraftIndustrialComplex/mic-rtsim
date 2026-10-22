package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.simibubi.create.api.behaviour.movement.MovementBehaviour;
import com.simibubi.create.content.contraptions.AbstractContraptionEntity;
import com.simibubi.create.content.contraptions.Contraption;
import com.simibubi.create.content.contraptions.ControlledContraptionEntity;
import com.simibubi.create.content.contraptions.bearing.ClockworkBearingBlockEntity;
import com.simibubi.create.content.contraptions.bearing.MechanicalBearingBlockEntity;
import com.simibubi.create.content.contraptions.gantry.GantryContraptionEntity;
import com.simibubi.create.content.contraptions.piston.LinearActuatorBlockEntity;
import com.simibubi.create.content.contraptions.pulley.PulleyBlockEntity;
import com.simibubi.create.content.kinetics.gantry.GantryShaftBlockEntity;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.EntityLeaveLevelEvent;

import java.util.List;

/**
 * DS-10 and contraptions. An outpost of a bearing with saws, a piston with a drill, a pulley with a harvester has
 * no machine block entity in the world while it is assembled: its saws and drills are the contraption's actors.
 * The anchor (a mechanical or clockwork bearing, a piston, a rope or elevator pulley, a gantry shaft the carriage
 * rides) is the only thing left, so it seeds a district while the contraption it carries has at least one
 * <i>working</i> actor ({@code Contraption.getActors()}: a block with a MovementBehaviour). Windmill sails, doors
 * and plain moving platforms have none, and stay non-seeds. A disassembled contraption's blocks are back in the
 * world, and whether its saws or drills seed follows the normal rule.
 *
 * <p>Passive actors do not count: seats, bells, train controls, redstone contacts, funnels, basins, burners,
 * campfires, stabilized bearings and sliding doors have a MovementBehaviour but make nothing. Every other
 * behaviour (drills, saws, rollers, harvesters, ploughs, deployers, dispensers, and any other mod's) does.
 *
 * <p>An anchor that is running but whose contraption entity is not attached yet (just loaded) counts as
 * seeding: dropping a district for the ticks before its entity arrives would only make it come back under a new id.
 * A cart assembler is no anchor: the cart contraption it builds is an entity that leaves on the rails, with no
 * block entity left to carry the seed.
 */
final class ContraptionAnchors {
    private ContraptionAnchors() {}

    static boolean isAnchor(BlockEntity be) {
        return be instanceof MechanicalBearingBlockEntity || be instanceof ClockworkBearingBlockEntity
                || be instanceof LinearActuatorBlockEntity || be instanceof GantryShaftBlockEntity;
    }

    /** Null: not an anchor. Else whether it seeds now. */
    static Boolean works(BlockEntity be) {
        if (be instanceof MechanicalBearingBlockEntity b)
            return b.isRunning() && attached(b.getMovedContraption());
        if (be instanceof ClockworkBearingBlockEntity c)
            return c.isRunning() && clockHands(c);
        if (be instanceof PulleyBlockEntity p) // rope and elevator pulleys; a mirror pulley shares its parent's contraption
            return p.running && attached(p.getAttachedContraption());
        if (be instanceof LinearActuatorBlockEntity a) // pistons
            return a.running && attached(a.movedContraption);
        if (be instanceof GantryShaftBlockEntity g)
            return gantry(g);
        return null;
    }

    private static boolean attached(AbstractContraptionEntity e) {
        return e == null || carriesWork(e.getContraption());
    }

    /** The clock hands ride entities at the bearing; none found while it runs is the not-attached-yet case. */
    private static boolean clockHands(ClockworkBearingBlockEntity c) {
        Level level = c.getLevel();
        if (level == null)
            return false;
        List<ControlledContraptionEntity> hands = level.getEntitiesOfClass(ControlledContraptionEntity.class,
                new AABB(c.getBlockPos()).inflate(3), c::isAttachedTo);
        if (hands.isEmpty())
            return true;
        for (ControlledContraptionEntity h : hands)
            if (carriesWork(h.getContraption()))
                return true;
        return false;
    }

    /**
     * A gantry's carriage is part of the moving contraption, so the shaft it rides is what stays in the world:
     * the shaft seeds while a gantry contraption with work is within reach of it.
     */
    private static boolean gantry(GantryShaftBlockEntity g) {
        Level level = g.getLevel();
        if (level == null)
            return false;
        for (GantryContraptionEntity e : level.getEntitiesOfClass(GantryContraptionEntity.class, new AABB(g.getBlockPos()).inflate(2.5)))
            if (carriesWork(e.getContraption()))
                return true;
        return false;
    }

    static boolean carriesWork(Contraption c) {
        if (c == null)
            return false;
        for (var actor : c.getActors()) {
            // The block's info, not its MovementContext: the contexts are made when the entity first ticks.
            MovementBehaviour b = actor.getLeft() == null ? null : MovementBehaviour.REGISTRY.get(actor.getLeft().state().getBlock());
            if (b != null && !passive(b))
                return true;
        }
        return false;
    }

    private static boolean passive(MovementBehaviour b) {
        String n = b.getClass().getSimpleName();
        return n.equals("SeatMovementBehaviour") || n.equals("BellMovementBehaviour") || n.equals("HauntedBellMovementBehaviour")
                || n.equals("ControlsMovementBehaviour") || n.equals("ContactMovementBehaviour") || n.equals("FunnelMovementBehaviour")
                || n.equals("BasinMovementBehaviour") || n.equals("BlazeBurnerMovementBehaviour") || n.equals("CampfireMovementBehaviour")
                || n.equals("StabilizedBearingMovementBehaviour") || n.equals("SlidingDoorMovementBehaviour");
    }

    // ---- events ---------------------------------------------------------------------------------

    /** A contraption entity joined or left a level: the anchors around it may seed or stop seeding, so detect again there. */
    static void changed(net.minecraft.world.entity.Entity entity, Level level) {
        if (!(entity instanceof AbstractContraptionEntity) || !(level instanceof ServerLevel sl))
            return;
        var ind = IndustryApi.current();
        if (ind == null)
            return;
        ChunkPos at = entity.chunkPosition();
        for (int dx = -1; dx <= 1; dx++)
            for (int dz = -1; dz <= 1; dz++)
                ind.districts().markDirty(sl.dimension(), ChunkPos.asLong(at.x + dx, at.z + dz));
    }

    static void joined(EntityJoinLevelEvent event) {
        changed(event.getEntity(), event.getLevel());
    }

    static void left(EntityLeaveLevelEvent event) {
        changed(event.getEntity(), event.getLevel());
    }
}
