package com.minecraftindustrialcomplex.mic_rtsim_create.bench;

import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.Create;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.CarriageBogey;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.entity.TravellingPoint;
import com.simibubi.create.content.trains.graph.TrackEdge;
import com.simibubi.create.content.trains.graph.TrackGraph;
import com.simibubi.create.content.trains.graph.TrackNode;
import com.simibubi.create.content.trains.graph.TrackNodeLocation;
import com.simibubi.create.content.trains.track.TrackMaterial;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.UUID;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.server.ServerStartedEvent;
import net.neoforged.neoforge.event.server.ServerStoppingEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;

/**
 * The far-view bench's trains (docs/choices-dh-trains.md): synthetic Create trains running on two 16-sided rings
 * of track built in code, so a dev client has many real trains to draw far away without anyone building them.
 * Only with {@code -Dmic_rtsim.farbench=true} (mic_rtsim_dh's {@code runClient -PfarBench=<trains>}); a pack never
 * sets it.
 *
 * <p>The track graphs and trains go straight into {@code Create.RAILWAYS} when the server starts, the way the
 * mic-fixes train GameTests build theirs, and come out again when it stops, before Create saves (a train with no
 * contraption behind its carriages is nothing Create can save). Create's own railway tick moves them: each tick
 * this holds every train at {@link #SPEED} as a driver would ({@code manualTick}). Create's train map sync sends
 * them to the client like any other train.
 *
 * <p>The rings lie 500 to 1500 blocks east of spawn, far from the chunks around a player who stays there: a
 * carriage in an entity-ticking chunk would try to spawn its contraption entity, find none, and drop its train.
 * {@code mic_rtsim.farbench.bandTrains} more run on a ring of radius {@link #BAND_RADIUS} around spawn, inside
 * the client's loaded chunks and outside its entity-ticking ones (the bench sets the simulation distance to 4),
 * which is the band mic_rtsim_dh's band renderer draws (docs/choices-dh-feed.md).
 */
public final class FarTrainBench {
    public static final boolean ENABLED = Boolean.getBoolean("mic_rtsim.farbench");
    static final int TRAINS = Math.max(0, Integer.getInteger("mic_rtsim.farbench.trains", 0));
    static final int CARRIAGES = Math.max(1, Integer.getInteger("mic_rtsim.farbench.carriages", 4));
    static final int BAND_TRAINS = Math.max(0, Integer.getInteger("mic_rtsim.farbench.bandTrains", 0));
    /** The band ring: 8.75 chunks, between a simulation distance of 4 and a render distance of 12. */
    static final double BAND_RADIUS = 140.0;

    static final double CENTRE_X = 1000.0, CENTRE_Z = 0.0;
    static final double[] RADII = {300.0, 500.0};
    /** Sixteen sides turn 22.5 degrees at a node; Create lets a train onto the next straight edge below 29. */
    static final int SIDES = 16;
    /** Rail height on a default flat world (its grass top is at -61). */
    static final double Y = -60.0;
    /** Blocks a tick (12 m/s), a slow freight train. */
    static final double SPEED = 0.6;
    /** A carriage's wheel points behind its front: leading bogey at 0 and 2, trailing at 6 and 8 (mic-fixes). */
    private static final double[] OFFSETS = {0.0, 2.0, 6.0, 8.0};
    private static final double PITCH = 10.0;
    /** Bogey anchor to bogey anchor between carriages: 7 behind one front to 11. */
    private static final int SPACING = 4;
    private static final int LOG_EVERY = 200;

    private static final List<Ring> RINGS = new ArrayList<>();
    private static final List<Ring> BAND = new ArrayList<>();
    private static final List<Train> BENCH = new ArrayList<>();
    private static int ticks;

    private FarTrainBench() {}

    public static void register() {
        NeoForge.EVENT_BUS.addListener(FarTrainBench::started);
        NeoForge.EVENT_BUS.addListener(FarTrainBench::tick);
        NeoForge.EVENT_BUS.addListener(FarTrainBench::stopping);
    }

    /** A closed loop of straight edges, with each point's distance along it. */
    private static final class Ring {
        final TrackGraph graph = new TrackGraph();
        final TrackNode[] nodes = new TrackNode[SIDES];
        final TrackEdge[] forward = new TrackEdge[SIDES];
        final double[] at = new double[SIDES + 1];

        Ring(ResourceKey<Level> dimension, double cx, double cz, double radius) {
            for (int i = 0; i < SIDES; i++) {
                double a = 2.0 * Math.PI * i / SIDES;
                TrackNodeLocation location = new TrackNodeLocation(cx + radius * Math.cos(a), Y,
                        cz + radius * Math.sin(a)).in(dimension);
                nodes[i] = new TrackNode(location, TrackGraph.nextNodeId(), new Vec3(0.0, 1.0, 0.0));
                graph.addNode(nodes[i]);
            }
            for (int i = 0; i < SIDES; i++) {
                TrackNode a = nodes[i], b = nodes[(i + 1) % SIDES];
                TrackEdge ab = new TrackEdge(a, b, null, TrackMaterial.ANDESITE);
                TrackEdge ba = new TrackEdge(b, a, null, TrackMaterial.ANDESITE);
                graph.putConnection(a, b, ab);
                graph.putConnection(b, a, ba);
                forward[i] = ab;
                at[i + 1] = at[i] + ab.getLength();
            }
        }

        double length() {
            return at[SIDES];
        }

        /** Puts {@code point} {@code s} along the ring, facing the way the nodes run. */
        void locate(TravellingPoint point, double s) {
            double l = length();
            s = ((s % l) + l) % l;
            int i = 0;
            while (i < SIDES - 1 && at[i + 1] <= s)
                i++;
            point.node1 = nodes[i];
            point.node2 = nodes[(i + 1) % SIDES];
            point.edge = forward[i];
            point.position = s - at[i];
        }
    }

    private static void started(ServerStartedEvent event) {
        ResourceKey<Level> overworld = Level.OVERWORLD;
        if (BAND_TRAINS > 0) {
            Ring band = new Ring(overworld, 0.0, 0.0, BAND_RADIUS);
            Create.RAILWAYS.putGraphWithDefaultGroup(band.graph);
            for (int i = 0; i < BAND_TRAINS; i++) {
                Train train = train(band, 10_000 + i, band.length() * i / BAND_TRAINS);
                Create.RAILWAYS.addTrain(train);
                BENCH.add(train);
            }
            MicRtsimCreate.LOGGER.info("far bench: {} band trains on a ring of radius {} around spawn", BAND_TRAINS,
                    BAND_RADIUS);
            BAND.add(band);
            saveOff(event);
        }
        if (TRAINS == 0)
            return;
        for (double r : RADII) {
            Ring ring = new Ring(overworld, CENTRE_X, CENTRE_Z, r);
            Create.RAILWAYS.putGraphWithDefaultGroup(ring.graph);
            RINGS.add(ring);
        }
        // Train i on ring i % 2, the trains of a ring evenly spaced along it
        int[] perRing = new int[RINGS.size()];
        for (int i = 0; i < TRAINS; i++)
            perRing[i % RINGS.size()]++;
        for (int i = 0; i < TRAINS; i++) {
            int r = i % RINGS.size();
            Ring ring = RINGS.get(r);
            double head = ring.length() * (i / RINGS.size()) / perRing[r];
            Train train = train(ring, i, head);
            Create.RAILWAYS.addTrain(train);
            BENCH.add(train);
        }
        MicRtsimCreate.LOGGER.info("far bench: {} trains of {} carriages on rings of radius {} around ({}, {})",
                TRAINS, CARRIAGES, java.util.Arrays.toString(RADII), CENTRE_X, CENTRE_Z);
        saveOff(event);
    }

    /**
     * {@code /save-off} for the bench's run: a synthetic carriage has no contraption, so Create cannot write it,
     * and vanilla's autosave (every 6,000 ticks) crashed a longer run. The save at the stop still runs (vanilla
     * clears the flag first), after {@link #stopping} took the trains out.
     */
    private static void saveOff(ServerStartedEvent event) {
        for (net.minecraft.server.level.ServerLevel level : event.getServer().getAllLevels())
            level.noSave = true;
    }

    private static Train train(Ring ring, int index, double head) {
        List<Carriage> carriages = new ArrayList<>();
        for (int c = 0; c < CARRIAGES; c++) {
            TravellingPoint[] p = new TravellingPoint[4];
            for (int k = 0; k < 4; k++) {
                p[k] = new TravellingPoint();
                ring.locate(p[k], head - (PITCH * c + OFFSETS[k]));
            }
            CarriageBogey front = new CarriageBogey(AllBlocks.SMALL_BOGEY.get(), false, new CompoundTag(), p[0], p[1]);
            CarriageBogey back = new CarriageBogey(AllBlocks.SMALL_BOGEY.get(), false, new CompoundTag(), p[2], p[3]);
            carriages.add(new Carriage(front, back, 6));
        }
        UUID id = UUID.nameUUIDFromBytes(("mic_rtsim_create:farbench:" + index).getBytes(StandardCharsets.UTF_8));
        Train train = new Train(id, null, ring.graph, carriages,
                new ArrayList<>(Collections.nCopies(CARRIAGES - 1, SPACING)), false, 0);
        train.speed = SPEED;
        return train;
    }

    /** Before the levels tick: every bench train is driven at {@link #SPEED} this tick. */
    private static void tick(ServerTickEvent.Pre event) {
        if (BENCH.isEmpty())
            return;
        int lost = 0, derailed = 0;
        for (Train train : BENCH) {
            if (Create.RAILWAYS.trains.get(train.id) != train) {
                lost++;
                continue;
            }
            if (train.derailed)
                derailed++;
            train.manualTick = true;
            train.speed = SPEED;
        }
        if (++ticks % LOG_EVERY == 0 || lost > 0 && ticks % 20 == 0)
            MicRtsimCreate.LOGGER.info("far bench: tick {}: {} trains running, {} lost, {} derailed", ticks,
                    BENCH.size() - lost, lost, derailed);
    }

    private static void stopping(ServerStoppingEvent event) {
        for (Train train : BENCH)
            Create.RAILWAYS.removeTrain(train.id);
        for (Ring ring : RINGS)
            Create.RAILWAYS.removeGraphAndGroup(ring.graph);
        for (Ring ring : BAND)
            Create.RAILWAYS.removeGraphAndGroup(ring.graph);
        if (!BENCH.isEmpty())
            MicRtsimCreate.LOGGER.info("far bench: took {} trains and {} rings out before the save", BENCH.size(),
                    RINGS.size());
        BENCH.clear();
        RINGS.clear();
        BAND.clear();
        ticks = 0;
    }
}
