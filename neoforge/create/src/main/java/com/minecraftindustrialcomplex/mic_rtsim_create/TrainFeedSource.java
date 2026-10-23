package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarItem;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarSource;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import com.simibubi.create.Create;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageContraptionEntity;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.entity.TravellingPoint;
import java.util.List;
import java.util.UUID;
import java.util.function.Consumer;
import net.createmod.catnip.data.Pair;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;

/**
 * Create trains for the far feed (research/distant-horizons-2026-10-21.md §4.1, §13 item 2; docs/boundary.md
 * §12.3): every carriage in a level, once a second, read on the server the way Create's own train map sync
 * reads it ({@code TrainMapSync.createEntry}: the leading and trailing bogey points on the graph, or the
 * rotation anchors of a train off it), sent only to subscribers and only within their radius. A client with
 * the feed stops asking Create for its map sync ({@code client.TrainFarSource}).
 *
 * <p>Each carriage carries its entity's network id while the server has one, so a client that has the entity
 * hides the proxy, and one that does not (a carriage past the server's simulation distance but inside the
 * client's render distance has none) draws it. It also carries what the carriage looks like
 * ({@link CarriageShapes}), so a far renderer draws the player's build rather than a box (DHT61).
 */
public final class TrainFeedSource implements FarSource {
    /** A carriage far away: dark grey, like the shadowed side of a train (DHT choices). */
    public static final int COLOUR = 0x3A3B3F;
    /** The top byte of a carriage's far id ('T'); the engine's agents keep their own namespaces. */
    public static final long NAMESPACE = 0x54L << 56;

    /** A carriage's far id: the namespace, 48 bits of the train's id, the carriage index. */
    public static long id(UUID train, int carriage) {
        long h = train.getMostSignificantBits() * 0x9E3779B97F4A7C15L ^ train.getLeastSignificantBits();
        h ^= h >>> 29;
        h *= 0xBF58476D1CE4E5B9L;
        h ^= h >>> 32;
        return NAMESPACE | ((h & 0xFFFF_FFFF_FFFFL) << 8) | (carriage & 0xFF);
    }

    @Override
    public String name() {
        return "Create trains";
    }

    /**
     * A carriage's entity is joining a server level (Create spawns one when its carriage reaches an
     * entity-ticking chunk): the subscribers holding the carriage hear its id at once, ahead of the entity's
     * spawn packet, so the client hides the far carriage the frame the real one appears
     * (docs/choices-dh-feed.md DHT40). The next sweep's item says the same.
     */
    static void joining(EntityJoinLevelEvent event) {
        if (event.getLevel() instanceof ServerLevel level && event.getEntity() instanceof CarriageContraptionEntity cce
                && cce.trainId != null && cce.carriageIndex >= 0 && cce.carriageIndex < 256)
            com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi.farEntityJoining(level, id(cce.trainId, cce.carriageIndex),
                    cce.getId());
    }

    @Override
    public void collect(ServerLevel level, long gameTime, Consumer<FarItem> out) {
        ResourceKey<Level> dim = level.dimension();
        CarriageShapes.sweep();
        for (Train train : Create.RAILWAYS.trains.values()) {
            List<Carriage> carriages = train.carriages;
            for (int i = 0; i < carriages.size() && i < 256; i++) {
                Carriage c = carriages.get(i);
                Vec3[] ends = ends(train, c, dim);
                if (ends == null)
                    continue;
                CarriageContraptionEntity e = c.anyAvailableEntity();
                int entity = e != null && e.level() == level && !e.isRemoved() ? e.getId() : -1;
                out.accept(new FarItem(id(train.id, i), FarKind.CARRIAGE, 0, 0, 0, COLOUR, FarState.MOVING,
                        new double[]{ends[0].x, ends[0].y, ends[0].z, ends[1].x, ends[1].y, ends[1].z}, null, entity, 0,
                        CarriageShapes.of(c, level), -1));
            }
        }
    }

    /** The carriage's leading and trailing points if it is wholly in {@code dim}, as Create's map sync has them. */
    private static Vec3[] ends(Train train, Carriage c, ResourceKey<Level> dim) {
        if (train.graph == null) {
            Pair<ResourceKey<Level>, Carriage.DimensionalCarriageEntity> dc = c.anyAvailableDimensionalCarriage();
            if (dc == null || c.presentInMultipleDimensions() || !dim.equals(dc.getFirst()))
                return null;
            Vec3 l = dc.getSecond().rotationAnchors.getFirst(), t = dc.getSecond().rotationAnchors.getSecond();
            return l == null || t == null ? null : new Vec3[]{l, t};
        }
        TravellingPoint leading = c.getLeadingPoint(), trailing = c.getTrailingPoint();
        if (leading == null || trailing == null || leading.edge == null || trailing.edge == null)
            return null;
        if (leading.node1 == null || trailing.node1 == null || leading.edge.isInterDimensional()
                || trailing.edge.isInterDimensional())
            return null;
        ResourceKey<Level> ld = leading.node1.getLocation().getDimension(), td = trailing.node1.getLocation().getDimension();
        if (!dim.equals(ld) || !dim.equals(td))
            return null;
        return new Vec3[]{leading.getPosition(train.graph), trailing.getPosition(train.graph)};
    }
}
