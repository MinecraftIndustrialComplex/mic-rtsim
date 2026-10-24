package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainSampler;
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
import com.simibubi.create.content.trains.track.BezierConnection;
import com.simibubi.create.content.trains.track.TrackMaterial;
import net.createmod.catnip.data.Couple;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.record;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.YARD;

/**
 * M7's train sampler on a real track (docs/choices-mowzie-build.md MZB-320 to 327, 329, test 11): synthetic Create
 * trains, held at 1.4 and 2.0 blocks a tick (28 and 40 blocks between samples), run over code-built bends of
 * radius 100 under {@link TrainSampler}, and the RAIL_PASS records the engine received are read back.
 *
 * <p>Four runs share one test, each on its own graph, 250 blocks apart in z at a far base (no chunk of it is
 * loaded, so no carriage entity is looked for: a synthetic train in an entity-ticking chunk is dropped).
 * Two are a 90 degree bend, two an S-bend of two bends of opposite hands (a left after a right), each with 60
 * blocks of straight before and after, the train held at 1.4 or 2.0. A run's circles, all of radius 15, are
 * placed from the bend's own points (the dense rail, not a formula), and the bend's chord is the straight line
 * between its two ends (the S-bend's, from first end to last):
 * <ul>
 *   <li>(a) and (c): 10 blocks outside the first bend's midpoint, so the rail enters the circle and the chord
 *       does not. The sampler must send one pass.
 *   <li>(b) and (d): on the chord, 29 blocks inside the arc, so the chord crosses the circle and the rail is
 *       out of it. No pass.
 *   <li>(e): as (a), with a mask without the rail's bit. No pass.
 * </ul>
 * Every pass is for the train ({@code CreateIds.of(id)}) with its owner (or none, the run without one), a point
 * on the rail (within 2.5 blocks of it; a sample is on it) at the rail's height plus Create's one block of
 * normal, and an entry before its exit.
 *
 * <p>Territories are scripted as TERRITORY (the engine's own way in) and removed at the end, with the trains
 * and graphs. MZB-383's case, a station inside the circle, is not covered: the synthetic graph holds no
 * station (see MZB-387).
 */
@GameTestHolder(MicRtsimCreate.MODID)
@PrefixGameTestTemplate(false)
public final class TrainSamplerGameTests {
    private TrainSamplerGameTests() {}

    private static final String DIM = "minecraft:overworld";
    /** Rail height; Create adds one block of normal to a train's point (TravellingPoint.getPosition). */
    private static final double Y0 = 64.0;
    private static final int R = 100, LEAD = 60, CIRCLE = 15;
    private static final int RAIL_MASK = 1 << ConflictRecords.SIGNAL_RAIL, OTHER_MASK = 1 << ConflictRecords.SIGNAL_BUILDING;
    /** A carriage's wheel points behind its front, as FarTrainBench lays them. */
    private static final double[] OFFSETS = {0.0, 2.0, 6.0, 8.0};
    private static final double HEAD0 = 20.0;
    private static final double REACH = 2.5;

    /** A circle, and whether the sampler must send a pass for it. {@code enters}: the rail runs through it. */
    private record Circle(String label, long faction, int mask, int x, int z, boolean enters, boolean passes) {}

    /** One run: a graph of straight and bent edges, a train on it, and its circles. */
    private static final class Run {
        final String name;
        final double speed;
        final UUID owner;
        final TrackGraph graph = new TrackGraph();
        final List<TrackNode> nodes = new ArrayList<>();
        final List<TrackEdge> forward = new ArrayList<>();
        final List<BezierConnection> bends = new ArrayList<>();
        /** The rail's (x, z), every quarter block of it. */
        final List<double[]> rail = new ArrayList<>();
        final List<Circle> circles = new ArrayList<>();
        double[] at;
        Vec3 chordFrom, chordTo;
        Train train;
        long added;
        boolean registered;

        Run(String name, double speed, UUID owner, boolean sBend, int bx, int bz) {
            this.name = name;
            this.speed = speed;
            this.owner = owner;
            Vec3 start = pt(bx, bz), a = pt(bx + LEAD, bz), b = pt(bx + LEAD + R, bz + R);
            Vec3 c = pt(bx + LEAD + 2 * R, bz + 2 * R);
            List<Vec3> points = new ArrayList<>(List.of(start, a, b));
            if (sBend) {
                points.add(c);
                points.add(pt(bx + LEAD + 2 * R + LEAD, bz + 2 * R));
            } else {
                points.add(pt(bx + LEAD + R, bz + R + LEAD));
            }
            for (Vec3 p : points) {
                TrackNode n = new TrackNode(new TrackNodeLocation(p.x, p.y, p.z).in(Level.OVERWORLD), TrackGraph.nextNodeId(),
                        new Vec3(0.0, 1.0, 0.0));
                graph.addNode(n);
                nodes.add(n);
            }
            // East, then a right turn to south; the S-bend turns left to east again
            connect(0, 1, null);
            connect(1, 2, bend(a, new Vec3(1, 0, 0), b, new Vec3(0, 0, -1)));
            if (sBend) {
                connect(2, 3, bend(b, new Vec3(0, 0, 1), c, new Vec3(-1, 0, 0)));
                connect(3, 4, null);
            } else {
                connect(2, 3, null);
            }
            at = new double[forward.size() + 1];
            for (int i = 0; i < forward.size(); i++)
                at[i + 1] = at[i] + forward.get(i).getLength();
            chordFrom = a;
            chordTo = sBend ? c : b;
            place(sBend);
        }

        private static Vec3 pt(double x, double z) {
            return new Vec3(x, Y0, z);
        }

        /** A bend as TrackPlacement makes one: starts, axes (each pointing into the bend) and an up normal at both ends. */
        private static BezierConnection bend(Vec3 from, Vec3 axisFrom, Vec3 to, Vec3 axisTo) {
            Vec3 up = new Vec3(0, 1, 0);
            return new BezierConnection(Couple.create(BlockPos.containing(from), BlockPos.containing(to)), Couple.create(from, to),
                    Couple.create(axisFrom, axisTo), Couple.create(up, up), true, false, TrackMaterial.ANDESITE);
        }

        private void connect(int i, int j, BezierConnection bend) {
            TrackNode n1 = nodes.get(i), n2 = nodes.get(j);
            TrackEdge ab = new TrackEdge(n1, n2, bend, TrackMaterial.ANDESITE);
            TrackEdge ba = new TrackEdge(n2, n1, bend == null ? null : bend.secondary(), TrackMaterial.ANDESITE);
            graph.putConnection(n1, n2, ab);
            graph.putConnection(n2, n1, ba);
            forward.add(ab);
            if (bend == null) {
                Vec3 p = n1.getLocation().getLocation(), q = n2.getLocation().getLocation();
                int steps = (int) Math.ceil(p.distanceTo(q) * 4.0);
                for (int k = 0; k <= steps; k++)
                    rail.add(new double[]{p.x + (q.x - p.x) * k / steps, p.z + (q.z - p.z) * k / steps});
            } else {
                bends.add(bend);
                int steps = (int) Math.ceil(bend.getLength() * 4.0);
                for (int k = 0; k <= steps; k++) {
                    Vec3 v = bend.getPosition((double) k / steps);
                    rail.add(new double[]{v.x, v.z});
                }
            }
        }

        /** The circles, from the first bend's midpoint and the chord. */
        private void place(boolean sBend) {
            Vec3 m = bends.get(0).getPosition(0.5);
            Vec3 u = chordTo.subtract(chordFrom).normalize();
            Vec3 foot = chordFrom.add(u.scale(m.subtract(chordFrom).dot(u)));
            Vec3 out = m.subtract(foot).normalize();
            Vec3 entering = m.add(out.scale(10.0));
            circles.add(circle(sBend ? "c" : "a", RAIL_MASK, entering, true, true));
            circles.add(circle(sBend ? "d" : "b", RAIL_MASK, foot, false, false));
            if (!sBend)
                circles.add(circle("e", OTHER_MASK, entering, true, false));
        }

        private static Circle circle(String label, int mask, Vec3 centre, boolean enters, boolean passes) {
            return new Circle(label, ConflictKit.newFaction(), mask, (int) Math.round(centre.x), (int) Math.round(centre.z), enters, passes);
        }

        /** What the run's geometry must satisfy for the test to mean anything (no sampler involved). */
        void verify() {
            Vec3 m = bends.get(0).getPosition(0.5);
            Vec3 u = chordTo.subtract(chordFrom).normalize();
            Vec3 foot = chordFrom.add(u.scale(m.subtract(chordFrom).dot(u)));
            double sagitta = m.distanceTo(foot);
            check(name + ": the first bend bulges about 29 blocks off its chord (" + sagitta + ")", sagitta > 27.0 && sagitta < 31.0);
            record("trainSampler", name + " bend radius", bends.get(0).getRadius());
            for (Circle c : circles) {
                double track = distanceToRail(c.x(), c.z()), chord = distanceToChord(c.x(), c.z());
                if (c.enters())
                    check(name + " " + c.label() + ": the rail enters the circle and the chord does not (rail " + track + ", chord " + chord + ")",
                            track < CIRCLE - 3 && chord > CIRCLE + 5);
                else
                    check(name + " " + c.label() + ": the chord crosses the circle and the rail does not (rail " + track + ", chord " + chord + ")",
                            track > CIRCLE + 5 && chord < 1.5);
                record("trainSampler", name + " circle " + c.label(), "(" + c.x() + ", " + c.z() + ") rail " + track + " chord " + chord);
            }
        }

        double distanceToRail(double x, double z) {
            double best = Double.MAX_VALUE;
            for (double[] p : rail)
                best = Math.min(best, Math.hypot(p[0] - x, p[1] - z));
            return best;
        }

        private double distanceToChord(double x, double z) {
            double dx = chordTo.x - chordFrom.x, dz = chordTo.z - chordFrom.z;
            double t = Math.max(0.0, Math.min(1.0, ((x - chordFrom.x) * dx + (z - chordFrom.z) * dz) / (dx * dx + dz * dz)));
            return Math.hypot(chordFrom.x + t * dx - x, chordFrom.z + t * dz - z);
        }

        void locate(TravellingPoint point, double s) {
            int i = 0;
            while (i < forward.size() - 1 && at[i + 1] <= s)
                i++;
            TrackEdge e = forward.get(i);
            point.node1 = e.node1;
            point.node2 = e.node2;
            point.edge = e;
            point.position = s - at[i];
        }

        /** One carriage (two bogeys of two points), the front 20 blocks along the first straight, held at the run's speed. */
        void start(long now) {
            TravellingPoint[] p = new TravellingPoint[4];
            for (int k = 0; k < 4; k++) {
                p[k] = new TravellingPoint();
                locate(p[k], HEAD0 - OFFSETS[k]);
            }
            CarriageBogey front = new CarriageBogey(AllBlocks.SMALL_BOGEY.get(), false, new CompoundTag(), p[0], p[1]);
            CarriageBogey back = new CarriageBogey(AllBlocks.SMALL_BOGEY.get(), false, new CompoundTag(), p[2], p[3]);
            List<Carriage> carriages = new ArrayList<>();
            carriages.add(new Carriage(front, back, 6));
            UUID id = UUID.nameUUIDFromBytes(("mic_rtsim_create:trainsampler:" + name + ":" + System.nanoTime()).getBytes(StandardCharsets.UTF_8));
            train = new Train(id, owner, graph, carriages, new ArrayList<>(), false, 0);
            train.speed = speed;
            train.manualTick = true;
            Create.RAILWAYS.addTrain(train);
            added = now;
        }

        /** Before Create's tick: the train is driven at the run's speed. */
        void hold() {
            if (train == null)
                return;
            check(name + ": the train is still Create's", Create.RAILWAYS.trains.get(train.id) == train && !train.invalid);
            check(name + ": the train has not derailed", !train.derailed && train.graph == graph);
            train.manualTick = true;
            train.speed = speed;
        }

        /** How far along the path the first carriage's leading point is, or -1. */
        double pathPosition() {
            TravellingPoint p = train.carriages.get(0).getLeadingPoint();
            for (int i = 0; i < forward.size(); i++)
                if (forward.get(i) == p.edge)
                    return at[i] + p.position;
            return -1.0;
        }

        double length() {
            return at[forward.size()];
        }

        void remove() {
            if (train != null)
                Create.RAILWAYS.removeTrain(train.id);
            if (registered)
                Create.RAILWAYS.removeGraphAndGroup(graph);
        }
    }

    @GameTest(template = YARD, batch = "rail_sampler", timeoutTicks = 4000)
    public static void theSamplerReportsPassesOnTheRealTrack(GameTestHelper helper) {
        var level = helper.getLevel();
        int[] base = ConflictKit.farBase();
        UUID owner = UUID.nameUUIDFromBytes(("mic_rtsim_create:trainsampler owner " + System.nanoTime()).getBytes(StandardCharsets.UTF_8));
        List<Run> runs = new ArrayList<>();
        // Cleanup first, so a run that fails half way through leaves nothing in Create's lists or the territories
        ConflictKit.onEnd(helper, () -> {
            for (Run r : runs) {
                r.remove();
                for (Circle c : r.circles)
                    ConflictKit.territoryRemoved(c.faction(), DIM);
            }
        });
        runs.add(new Run("curve at 1.4", 1.4, owner, false, base[0], base[1]));
        runs.add(new Run("curve at 2.0 with no owner", 2.0, null, false, base[0], base[1] + 250));
        runs.add(new Run("S-bend at 1.4", 1.4, owner, true, base[0], base[1] + 500));
        runs.add(new Run("S-bend at 2.0", 2.0, owner, true, base[0], base[1] + 750));
        long[] before = new long[2];
        int passing = 0;
        for (Run r : runs)
            for (Circle c : r.circles)
                if (c.passes())
                    passing++;
        int expected = passing;

        helper.startSequence()
                .thenExecute(() -> {
                    before[0] = TrainSampler.samples;
                    before[1] = TrainSampler.passes;
                    for (Run r : runs) {
                        r.verify();
                        Create.RAILWAYS.putGraphWithDefaultGroup(r.graph);
                        r.registered = true;
                    }
                    ConflictKit.together(() -> {
                        for (Run r : runs)
                            for (Circle c : r.circles)
                                ConflictKit.territory(c.faction(), DIM, c.mask(), c.x(), c.z(), CIRCLE);
                    });
                })
                .thenWaitUntil(() -> {
                    check("the conflict engine is up", RtsimApi.conflictUp());
                    for (Run r : runs)
                        for (Circle c : r.circles)
                            check(r.name + " " + c.label() + ": the territory is applied", RtsimApi.territories().get(c.faction()) != null);
                })
                .thenExecute(() -> {
                    for (Run r : runs)
                        r.start(level.getGameTime());
                })
                .thenWaitUntil(() -> {
                    for (Run r : runs)
                        r.hold();
                    for (Run r : runs)
                        check(r.name + ": the train has run the track (" + r.pathPosition() + " of " + r.length() + ")",
                                r.pathPosition() >= r.length() - 1.0);
                })
                .thenWaitUntil(() -> {
                    for (Run r : runs)
                        for (Circle c : r.circles)
                            if (c.passes())
                                check(r.name + " " + c.label() + ": the pass has reached the engine", !ConflictKit.railPasses(c.faction()).isEmpty());
                })
                .thenIdle(60)
                .thenExecute(() -> {
                    long now = level.getGameTime();
                    int dimension = RtsimTests.session().dimId(Level.OVERWORLD);
                    for (Run r : runs) {
                        long id = CreateIds.of(r.train.id);
                        for (Circle c : r.circles) {
                            var got = ConflictKit.railPasses(c.faction());
                            check(r.name + " " + c.label() + ": " + (c.passes() ? "exactly one pass" : "no pass") + ", got " + got.size(),
                                    got.size() == (c.passes() ? 1 : 0));
                            if (!c.passes())
                                continue;
                            ConflictKit.RailPassRecord p = ConflictKit.railPass(got.get(0));
                            check(r.name + " " + c.label() + ": the train is CreateIds.of(id)", p.train() == id);
                            check(r.name + " " + c.label() + ": the owner is " + r.owner + ", got " + p.owner(), Objects.equals(p.owner(), r.owner));
                            check(r.name + " " + c.label() + ": the overworld's id " + dimension + ", got " + p.dimension(), p.dimension() == dimension);
                            double off = r.distanceToRail(p.x(), p.z());
                            check(r.name + " " + c.label() + ": the point is on the rail (" + off + " blocks off)", off <= REACH);
                            check(r.name + " " + c.label() + ": the point is at the rail's height plus Create's block (y " + p.y() + ")",
                                    Math.abs(p.y() - (Y0 + 1.0)) <= 0.1);
                            check(r.name + " " + c.label() + ": entry " + p.entry() + " is before exit " + p.exit(), p.entry() < p.exit());
                            check(r.name + " " + c.label() + ": the train had been added (" + r.added + ") and the pass is over (" + now + ")",
                                    p.entry() >= r.added && p.exit() <= now);
                            check(r.name + " " + c.label() + ": at most two samples long (" + (p.exit() - p.entry()) + " ticks)",
                                    p.exit() - p.entry() <= 2L * TrainSampler.PERIOD);
                            double toCentre = Math.hypot(p.x() - c.x(), p.z() - c.z());
                            record("trainSampler", r.name + " " + c.label() + " pass",
                                    "point (" + p.x() + ", " + p.y() + ", " + p.z() + ") " + off + " off the rail, " + toCentre
                                            + " from the centre, ticks " + p.entry() + " to " + p.exit());
                        }
                    }
                    check("the sampler took samples (" + (TrainSampler.samples - before[0]) + ")", TrainSampler.samples > before[0]);
                    check("the sampler sent " + expected + " passes, not " + (TrainSampler.passes - before[1]),
                            TrainSampler.passes - before[1] == expected);
                    record("trainSampler", "samples and passes", (TrainSampler.samples - before[0]) + " and " + (TrainSampler.passes - before[1]));
                })
                .thenSucceed();
    }
}
