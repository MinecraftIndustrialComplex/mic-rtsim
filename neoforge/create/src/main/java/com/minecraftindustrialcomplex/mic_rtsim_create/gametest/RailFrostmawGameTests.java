package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_create.CreateIds;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.minecraftindustrialcomplex.mic_rtsim_create.TrainWatch;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.Create;
import com.simibubi.create.content.contraptions.AbstractContraptionEntity;
import com.simibubi.create.content.contraptions.glue.SuperGlueEntity;
import com.simibubi.create.content.trains.entity.Carriage;
import com.simibubi.create.content.trains.entity.Train;
import com.simibubi.create.content.trains.graph.TrackGraph;
import com.simibubi.create.content.trains.graph.TrackNodeLocation;
import com.simibubi.create.content.trains.station.GlobalStation;
import com.simibubi.create.content.trains.station.StationBlockEntity;
import com.simibubi.create.content.trains.track.TrackBlock;
import com.simibubi.create.content.trains.track.TrackShape;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.game.ClientboundSoundPacket;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.util.Mth;
import net.minecraft.world.Clearable;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.DyeColor;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import net.neoforged.neoforge.items.IItemHandler;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.record;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.YARD;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.paced;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.chest;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.fill;

/**
 * A Create train and a Frostmaw (MOWZIE.md sections 3.1 and 4.4, docs/choices-mowzie-build.md MZB-328, 329, 370 and
 * 373): four tests on real track, real assembled trains and a real Frostmaw.
 *
 * <ul>
 * <li>Test 10: a train is driven through a Frostmaw standing on the rail. Create's contraption collision moves only
 * entities whose push reaction lets a contraption push them, and the Frostmaw's is IGNORE, so the carriage's box
 * overlaps the boss and nothing happens to either. A zombie, the control, is moved.
 * <li>Test 8: a Frostmaw's ice ball against a rider seated in a carriage. Static form: the train stands, the
 * Frostmaw 20 blocks off along the rail, and the ball freezes the rider, which takes it off the seat (the frozen
 * controller becomes its one vehicle). Moving form: the same rider on a train driven past at 0.5, 1.0, 1.4 and
 * 2.0 blocks a tick, four passes each, the Frostmaw 18 blocks off the rail: the hits are counted and logged, a
 * measurement (a separate test, its own batch).
 * <li>Test 9: a lurk. The engine side is scripted (AMBUSH_SPOT, BAND_MATERIALIZE, TRAIN_ROBBED, IND_CARRIAGE_DELTA,
 * AMBUSH_SPOT removed). The Frostmaw stands off the rail at its spot, swipes a seated zombie as a carriage passes,
 * and the train that was robbed has its station's roar.
 * </ul>
 *
 * <p>This class has no {@code @GameTestHolder}: {@link MicRtsimCreate} registers it from
 * {@code RegisterGameTestsEvent} only when Mowzie's Mobs is loaded ({@code -PwithMowzie}), and its templates
 * name this mod's namespace. It names the Frostmaw, its ice ball and its frozen controller by registry id and no
 * Mowzie class. The tests wait with plain {@code thenWaitUntil(Runnable)} steps, one purpose each, and a hard
 * failure is {@link Line#stop} (a wait swallows the failures of its check until the timeout).
 *
 * <p>The track is {@value #LENGTH} blocks of real track along x at a far base (its chunks forced, a stone floor
 * under it and the air cleared above, all taken out again at the end so Create's saved graph holds nothing of
 * it), a station in the middle on the south side, and the train built beside the station as
 * {@code RailFixtureGameTests} builds its own: two small bogeys, controls on the first, a chest on the second,
 * each glued (and a seat on the first's bogey, for the tests with a rider). Create's own tick drives the train
 * ({@code manualTick}, reset before every tick, and {@code speed}); which way positive speed goes is found by
 * running it three ticks.
 *
 * <p>A step of a test's sequence runs before Create's tick in the same server tick, so a distance check counts
 * observations, not game time: between two of this test's ticks Create ticks the train once.
 */
@PrefixGameTestTemplate(false)
public final class RailFrostmawGameTests {
    private RailFrostmawGameTests() {}

    public static final String FROSTMAW = "mowziesmobs:frostmaw", ZOMBIE = "minecraft:zombie";
    static final String ICE_BALL = "mowziesmobs:ice_ball", FROZEN_CONTROLLER = "mowziesmobs:frozen_controller", ROAR = "mowziesmobs:frostmaw.roar";
    static final String CAMP = "mic_rtsim:conflict/test_unclaimed_camp";
    static final double SPEED = 1.0;
    static final int LENGTH = 150, STATION_AT = 75, AHEAD = 32, PAST = 20;
    /** Observations before test 10's entity start is taken (it settles on the rail; the train is still far). */
    static final int SETTLE = 3;
    /** The cleared box: blocks past each end of the track, blocks of floor below the rail, blocks of air from the rail up. */
    static final int MARGIN = 3, BELOW = 1, ABOVE = 8;

    /** Test 8, static: the Frostmaw stands this far from the rider along the rail, and a ball must come within this many ticks. */
    static final int ICE_DISTANCE = 20, ICE_TICKS = 400;
    /** A freeze that has not let go of the rider this many ticks after it began fails the test (the jar's is 50). */
    static final int FREEZE_LIMIT = 200;
    /** Test 8, moving: the Frostmaw's distance off the rail, the speeds, the passes each, the window it is given its target in, the quiet between passes. */
    static final int ICE_OFF = 18, PASSES = 4, NEAR_X = 40, GAP = 220;
    static final double[] ICE_SPEEDS = {0.5, 1.0, 1.4, 2.0};

    /**
     * Test 9: the lurk point is this far ahead of the train, the spot this far off the rail (the engine's own:
     * {@code SPOT_BLOCKS} of rail.rs, 5 whole blocks from the track point toward the den, here due north), the
     * second station this far from the first.
     */
    static final int LURK_AHEAD = 40, LATERAL = 5, B_AWAY = 60, HEARER_BEHIND = 20, HOLD_TICKS = 200;
    static final float REACH = 8.0f;

    @GameTest(templateNamespace = MicRtsimCreate.MODID, template = YARD, batch = "rail_frostmaw_pass", timeoutTicks = 2400)
    public static void aTrainPassesThroughAFrostmawWithoutTouchingIt(GameTestHelper helper) {
        drive(helper, FROSTMAW, true);
    }

    // Its own batch: a batch's tests run together, and each releases the far chunks the other has forced
    // (ConflictKit.releaseFarChunks), which stops the other's train
    @GameTest(templateNamespace = MicRtsimCreate.MODID, template = YARD, batch = "rail_frostmaw_control", timeoutTicks = 2400)
    public static void theControlZombieOnTheRailIsMoved(GameTestHelper helper) {
        drive(helper, ZOMBIE, false);
    }

    // ---- the shared fixture ---------------------------------------------------------------------------------

    /** A block box: its blocks before the test built in it, and what the test builds (a floor at its bottom layer, air above). */
    private static final class Patch {
        final int xa, xb, ya, yb, za, zb;
        BlockState[] before;

        Patch(int xa, int xb, int ya, int yb, int za, int zb) {
            this.xa = xa;
            this.xb = xb;
            this.ya = ya;
            this.yb = yb;
            this.za = za;
            this.zb = zb;
        }

        int size() {
            return (xb - xa + 1) * (yb - ya + 1) * (zb - za + 1);
        }

        BlockPos at(int i) {
            int h = yb - ya + 1, w = zb - za + 1, rest = i % (h * w);
            return new BlockPos(xa + i / (h * w), ya + rest % h, za + rest / h);
        }

        void snapshot(ServerLevel level) {
            before = new BlockState[size()];
            for (int i = 0; i < before.length; i++)
                before[i] = level.getBlockState(at(i));
        }

        void build(ServerLevel level, BlockState floor, BlockState air) {
            for (int x = xa; x <= xb; x++)
                for (int z = za; z <= zb; z++) {
                    level.setBlock(new BlockPos(x, ya, z), floor, 2);
                    for (int yy = ya + 1; yy <= yb; yy++)
                        level.setBlock(new BlockPos(x, yy, z), air, 2);
                }
        }

        /** Every block as the snapshot has it, with no neighbour or shape updates. */
        void restore(ServerLevel level) {
            if (before == null)
                return;
            for (int i = 0; i < before.length; i++)
                level.setBlock(at(i), before[i], Block.UPDATE_CLIENTS | Block.UPDATE_KNOWN_SHAPE);
        }
    }

    /** What a run on the rail knows: the track, its stations, the train, and how to take them all out again. */
    private static class Line {
        final GameTestHelper helper;
        final ServerLevel level;
        /** Blocks cleared each side of the rail, and the chunk rows forced north of the rail's (the south one is always forced). */
        final int side, northRows;
        /** Extra cleared boxes, as {dxFrom, dxTo, dzFrom, dzTo} against the track's start and rail. */
        final List<int[]> pads = new ArrayList<>();
        /** The track offsets of the stations (their blocks stand on the rail's south side). */
        final List<Integer> stations = new ArrayList<>();
        final List<Patch> patches = new ArrayList<>();
        /** Run first when the test ends (the test's own entities). */
        final List<Runnable> atEnd = new ArrayList<>();
        final int x0, z0;
        int y;
        Train train;
        Direction dir;
        double sign, leadAtFirstHold;
        long firstHold = -1;
        BlockPos chest;
        boolean cleaned;

        Line(GameTestHelper helper, int side, int northRows) {
            this.helper = helper;
            this.level = helper.getLevel();
            this.side = side;
            this.northRows = northRows;
            int[] base = ConflictKit.farBase();
            x0 = base[0];
            z0 = base[1] + 8; // the middle of a chunk, with the station's block beside the track in it
            stations.add(STATION_AT);
        }

        BlockPos track(int dx) {
            return new BlockPos(x0 + dx, y, z0);
        }

        double lead() {
            return train.carriages.get(0).getLeadingPoint().getPosition(train.graph).x;
        }

        /** The y of a carriage's rail point, as the sampler reads it. */
        double railY() {
            return train.carriages.get(0).getLeadingPoint().getPosition(train.graph).y;
        }

        void forceChunks(boolean on) {
            int cx0 = x0 >> 4, cx1 = (x0 + LENGTH) >> 4, cz = z0 >> 4;
            for (int cx = cx0; cx <= cx1; cx++)
                for (int z = cz - 1 - northRows + 1; z <= cz + 1; z++) {
                    level.setChunkForced(cx, z, on);
                    if (on)
                        level.getChunk(cx, z);
                }
        }

        /** The chunks, the box and the track. */
        void build() {
            ConflictKit.releaseFarChunks(level);
            forceChunks(true);
            y = level.getHeight(Heightmap.Types.WORLD_SURFACE, x0 + STATION_AT, z0);
            record("railFrostmaw", "track height", y);
            patches.add(new Patch(x0 - MARGIN, x0 + LENGTH + MARGIN - 1, y - BELOW, y + ABOVE - 1, z0 - side, z0 + side));
            for (int[] p : pads)
                patches.add(new Patch(x0 + p[0], x0 + p[1], y - BELOW, y + ABOVE - 1, z0 + p[2], z0 + p[3]));
            for (Patch p : patches)
                p.snapshot(level);
            BlockState air = Blocks.AIR.defaultBlockState(), floor = Blocks.STONE.defaultBlockState();
            BlockState xo = AllBlocks.TRACK.getDefaultState().setValue(TrackBlock.SHAPE, TrackShape.XO);
            // Whatever the terrain: a floor one below the rail and clear air above (a Frostmaw is 4 wide and 4 tall)
            for (Patch p : patches)
                p.build(level, floor, air);
            for (int dx = 0; dx < LENGTH; dx++)
                level.setBlock(track(dx), xo, 3);
        }

        boolean graphed() {
            for (TrackGraph g : Create.RAILWAYS.trackNetworks.values())
                for (TrackNodeLocation loc : g.getNodes()) {
                    Vec3 p = loc.getLocation();
                    if (p.x >= x0 - 1 && p.x <= x0 + LENGTH + 1 && Math.abs(p.z - (z0 + 0.5)) < 1.5)
                        return true;
                }
            return false;
        }

        BlockPos stationPos(int dx) {
            return track(dx).south();
        }

        StationBlockEntity stationAt(int dx) {
            return RailFixtureGameTests.station(level, stationPos(dx));
        }

        /** A station block on the rail's south side, aimed at the rail. */
        void placeStation(int dx) {
            RailFixtureGameTests.point(level, stationPos(dx), AllBlocks.TRACK_STATION.getDefaultState(), new BlockPos(0, 0, -1));
        }

        /**
         * Puts the box back as the snapshots have it: the track and the stations out first, as blocks, so Create takes
         * them (and their graph) out; the chest emptied so it drops nothing; the glue; then every block of the boxes.
         * Block entities of the original terrain are not restored (a generated far chunk has none the test could reach).
         */
        void restore() {
            if (patches.isEmpty() || patches.get(0).before == null)
                return;
            for (int dx = 0; dx < LENGTH; dx++)
                level.setBlock(track(dx), Blocks.AIR.defaultBlockState(), 3);
            for (int dx : stations)
                level.setBlock(stationPos(dx), Blocks.AIR.defaultBlockState(), 3);
            if (chest != null && level.getBlockEntity(chest) instanceof Clearable c)
                c.clearContent();
            for (SuperGlueEntity glue : level.getEntitiesOfClass(SuperGlueEntity.class, AABB.encapsulatingFullBlocks(track(0), track(LENGTH))
                    .inflate(side, ABOVE, side)))
                glue.discard();
            for (Patch p : patches)
                p.restore(level);
        }

        /** The end of the test, passed or failed. */
        void end() {
            if (cleaned)
                return;
            cleaned = true;
            for (Runnable r : atEnd)
                r.run();
            if (train != null) {
                for (Carriage c : train.carriages) {
                    Entity e = c.anyAvailableEntity();
                    if (e != null)
                        e.discard();
                }
                TrainWatch.forget(train);
                Create.RAILWAYS.removeTrain(train.id);
            }
            restore();
            forceChunks(false);
        }

        /** Ends the test at once (a waiting step would only retry until its timeout). */
        void stop(String why) {
            GameTestAssertException e = new GameTestAssertException(why);
            helper.testInfo.fail(e);
            throw e;
        }

        /** Before Create's tick, every tick the train moves: driven at {@code speed} (signed, blocks a tick). */
        void hold(double speed) {
            if (Create.RAILWAYS.trains.get(train.id) != train)
                stop("the train is no longer Create's");
            if (train.derailed || train.invalid)
                stop("the train derailed (derailed " + train.derailed + ", invalid " + train.invalid + ")");
            train.manualTick = true;
            train.speed = speed;
        }

        void hold() {
            hold(SPEED);
        }

        /** After the probe: which way positive speed takes the train's front along x. */
        void settle() {
            sign = Math.signum(lead() - leadAtFirstHold);
        }

        /** The carriage's contraption entity, or null when it is not loaded. */
        AbstractContraptionEntity entityOf(int carriage) {
            return train.carriages.get(carriage).anyAvailableEntity() instanceof AbstractContraptionEntity e ? e : null;
        }

        /** The blocks of the train's contraptions, summed. */
        int blocks() {
            int n = 0;
            for (int i = 0; i < train.carriages.size(); i++) {
                AbstractContraptionEntity e = entityOf(i);
                n += e == null ? -1000 : e.getContraption().getBlocks().size();
            }
            return n;
        }

        /** Adds {@code rider} to the level, on the seat of a carriage's contraption. */
        void seat(Entity rider, int carriage) {
            AbstractContraptionEntity e = entityOf(carriage);
            check("carriage " + carriage + " has an entity", e != null);
            check("carriage " + carriage + " has a seat", !e.getContraption().getSeats().isEmpty());
            rider.moveTo(e.getX(), e.getY(), e.getZ(), 0.0f, 0.0f);
            if (!rider.isAddedToLevel())
                check("the rider is added", level.addFreshEntity(rider));
            e.addSittingPassenger(rider, 0);
            check("the rider rides the carriage", rider.getVehicle() == e && e.getContraption().getSeatMapping().containsKey(rider.getUUID()));
        }
    }

    private static String idOf(Entity e) {
        return e == null ? "none" : BuiltInRegistries.ENTITY_TYPE.getKey(e.getType()).toString();
    }

    private static EntityType<?> typeOf(String id) {
        return BuiltInRegistries.ENTITY_TYPE.getOptional(ResourceLocation.parse(id))
                .orElseThrow(() -> new GameTestAssertException("no entity type " + id));
    }

    /**
     * The sequence every test starts with: the chunks, the box and the track, a station beside the rail, the train
     * built and assembled there (two carriages, with a seat in the first when {@code seat}), up to the carriages'
     * entities standing.
     */
    private static GameTestSequence laid(Line l, boolean seat) {
        ServerLevel level = l.level;
        UUID assembler = UUID.nameUUIDFromBytes(("mic-rtsim frostmaw " + System.nanoTime()).getBytes());
        ConflictKit.onEnd(l.helper, l::end);
        return l.helper.startSequence()
                .thenExecute(l::build)
                .thenWaitUntil(() -> check("the track has a graph", l.graphed()))
                .thenExecute(() -> l.placeStation(STATION_AT))
                .thenWaitUntil(() -> check("the station is on the graph", l.stationAt(STATION_AT).getStation() != null))
                .thenExecute(() -> {
                    StationBlockEntity sbe = l.stationAt(STATION_AT);
                    sbe.updateName("Frostmaw Pass");
                    l.dir = sbe.getAssemblyDirection();
                    check("an assembly direction along the track", l.dir != null && l.dir.getAxis() == Direction.Axis.X);
                    l.chest = cars(l.helper, l.track(STATION_AT), l.dir, seat);
                    Create.RAILWAYS.markTracksDirty();
                })
                .thenIdle(5)
                .thenExecute(() -> check("assembly mode", l.stationAt(STATION_AT).enterAssemblyMode(null)))
                .thenIdle(5)
                .thenExecute(() -> {
                    StationBlockEntity sbe = l.stationAt(STATION_AT);
                    sbe.assemble(assembler);
                    GlobalStation st = sbe.getStation();
                    l.train = st == null ? null : st.getPresentTrain();
                    if (l.train == null)
                        throw new GameTestAssertException("no train assembled: " + sbe.saveWithFullMetadata(level.registryAccess()).get("LastException"));
                    l.train.name = Component.literal("Frostmaw Pass Express");
                    sbe.exitAssemblyMode();
                    Create.RAILWAYS.markTracksDirty();
                })
                .thenWaitUntil(() -> {
                    check("the train is on the rail", l.train.graph != null && !l.train.derailed);
                    for (Carriage c : l.train.carriages)
                        check("a carriage entity stands", c.anyAvailableEntity() != null);
                });
    }

    /** Three ticks of drive tell which way the train's front goes ({@link Line#settle} reads it in the step after). */
    private static GameTestSequence probed(Line l, GameTestSequence s) {
        return s.thenWaitUntil(() -> {
            long now = l.level.getGameTime();
            if (l.firstHold < 0) {
                l.firstHold = now;
                l.leadAtFirstHold = l.lead();
            }
            l.hold();
            check("three ticks of drive (tick " + (now - l.firstHold) + ")", now - l.firstHold >= 3);
            check("the train moves along x under manual drive (" + (l.lead() - l.leadAtFirstHold) + ")",
                    Math.abs(l.lead() - l.leadAtFirstHold) > 0.5);
        });
    }

    /**
     * Two single-bogey carriages on the station's track, as a player builds them ({@code RailFixtureGameTests.cars}):
     * a small bogey at the first block past the station and one four further, controls facing the assembly
     * direction on the first, a chest with coal on the second, each glued. With {@code seat}, a seat sits on the
     * first's bogey (the rider's feet are then 2 blocks above the rail, the least a seat can be, which keeps the
     * swipe's three-dimensional reach short) and the controls go above it. The chest's position.
     */
    private static BlockPos cars(GameTestHelper h, BlockPos stationTrack, Direction d, boolean seat) {
        ServerLevel level = h.getLevel();
        BlockState bogey = AllBlocks.SMALL_BOGEY.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_AXIS, Direction.Axis.X);
        BlockPos b1 = stationTrack.relative(d, 1).above(), b2 = stationTrack.relative(d, 5).above();
        level.setBlock(b1, bogey, 3);
        level.setBlock(b2, bogey, 3);
        BlockPos controls = seat ? b1.above(2) : b1.above();
        if (seat)
            level.setBlock(b1.above(), AllBlocks.SEATS.get(DyeColor.RED).getDefaultState(), 3);
        level.setBlock(controls, AllBlocks.TRAIN_CONTROLS.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, d), 3);
        chest(h, b2.above());
        fill(h, b2.above(), Items.COAL, 32);
        level.addFreshEntity(new SuperGlueEntity(level, SuperGlueEntity.span(b1, controls)));
        level.addFreshEntity(new SuperGlueEntity(level, SuperGlueEntity.span(b2, b2.above())));
        return b2.above();
    }

    /** A Frostmaw on its feet at (x, y, z): awake for good, with its crystal, kept to its place (radius 2). */
    private static Mob frostmawAt(ServerLevel level, double x, double y, double z) {
        if (!(typeOf(FROSTMAW).create(level) instanceof Mob m))
            throw new GameTestAssertException("could not create a " + FROSTMAW);
        CompoundTag tag = m.saveWithoutId(new CompoundTag());
        tag.putBoolean("active", true);
        tag.putBoolean("alwaysActive", true);
        tag.putBoolean("has_crystal", true);
        m.load(tag);
        m.moveTo(x, y, z, 0.0f, 0.0f);
        m.setPersistenceRequired();
        m.restrictTo(BlockPos.containing(x, y, z), 2);
        check("the Frostmaw is added", level.addFreshEntity(m));
        return m;
    }

    /** Health the rider will not lose in a test: a swipe or a freeze takes a few points, never all. */
    private static void sturdy(LivingEntity e) {
        e.getAttribute(Attributes.MAX_HEALTH).setBaseValue(1000.0);
        e.setHealth(1000.0f);
    }

    // ---- test 10: a train through a Frostmaw -----------------------------------------------------------------

    /** What one run of test 10 knows. */
    private static final class Drive extends Line {
        final String entityId;
        final boolean boss;
        Entity target;
        Vec3 targetStart;
        float health0;
        /** Observations since the entity was placed, and the leading point at the placing tick and at the last one. */
        int observed;
        double runFrom, runTo;
        boolean overlapped, hurt, healthChanged;
        double drift;

        Drive(GameTestHelper helper, String entityId, boolean boss) {
            super(helper, 4, 1);
            this.entityId = entityId;
            this.boss = boss;
            atEnd.add(() -> {
                if (target != null)
                    target.discard();
            });
        }

        /** The entity as it stands this tick, against the carriages. It records; the run is judged at its end. */
        void observe() {
            observed++;
            runTo = lead();
            if (!target.isAlive())
                stop("the " + entityId + " is gone from the world");
            if (observed == SETTLE)
                targetStart = target.position();
            if (target instanceof LivingEntity l) {
                if (l.hurtTime > 0)
                    hurt = true;
                if (l.getHealth() != health0)
                    healthChanged = true;
            }
            if (targetStart != null)
                drift = Math.max(drift, target.position().distanceTo(targetStart));
            for (Carriage c : train.carriages) {
                Entity e = c.anyAvailableEntity();
                if (e != null && e.getBoundingBox().intersects(target.getBoundingBox()))
                    overlapped = true;
            }
        }
    }

    private static void drive(GameTestHelper helper, String entityId, boolean boss) {
        ServerLevel level = helper.getLevel();
        Drive d = new Drive(helper, entityId, boss);

        probed(d, laid(d, false))
                .thenExecute(() -> {
                    d.hold();
                    d.settle();
                    double at = d.lead() + d.sign * AHEAD;
                    d.target = typeOf(d.entityId).create(level);
                    if (d.target == null)
                        throw new GameTestAssertException("could not create a " + d.entityId);
                    d.target.moveTo(at, d.y + 0.25, d.z0 + 0.5, 0.0f, 0.0f);
                    if (d.target instanceof Mob m) {
                        m.setNoAi(true);
                        m.setPersistenceRequired();
                    }
                    if (d.target instanceof LivingEntity l) // a zombie in daylight would burn and flinch
                        l.addEffect(new MobEffectInstance(MobEffects.FIRE_RESISTANCE, 1_000_000, 0, false, false));
                    level.addFreshEntity(d.target);
                    d.health0 = d.target instanceof LivingEntity l ? l.getHealth() : 0.0f;
                    d.runFrom = d.lead();
                    record("railFrostmaw", d.entityId + " placed", "x " + at + ", the train goes " + (d.sign > 0 ? "east" : "west") + ", health " + d.health0
                            + ", box " + d.target.getBoundingBox());
                })
                .thenWaitUntil(() -> {
                    d.hold();
                    d.observe();
                    double beyond = (d.lead() - d.target.getX()) * d.sign;
                    check("the train is " + PAST + " blocks past the " + d.entityId + " (" + beyond + ")" + (boss ? "" : ", or has moved it"),
                            beyond >= PAST || !boss && d.drift >= 0.5);
                })
                .thenExecute(() -> {
                    // A GameTest sequence runs its next step in the same tick: observation 1 is in the tick that set runFrom,
                    // before Create moves the train, so k observations span k - 1 ticks of travel
                    double travelled = Math.abs(d.runTo - d.runFrom), expected = SPEED * (d.observed - 1);
                    record("railFrostmaw", d.entityId + " run", "observations " + d.observed + ", travelled " + travelled + ", expected " + expected
                            + ", overlapped " + d.overlapped + ", moved " + d.drift + ", hurt " + d.hurt + ", health changed " + d.healthChanged);
                    check("the train is not derailed", !d.train.derailed && !d.train.invalid);
                    if (boss) {
                        check("a carriage's box overlapped the Frostmaw's on some tick", d.overlapped);
                        check("the Frostmaw's health stayed " + d.health0, !d.healthChanged);
                        check("the Frostmaw's hurtTime stayed 0", !d.hurt);
                        check("the Frostmaw moved under 0.1 blocks (" + d.drift + ")", d.drift < 0.1);
                        check("the train kept its speed: travelled " + travelled + " of " + expected, Math.abs(travelled - expected) <= 0.01 * expected);
                    } else {
                        check("the zombie was moved at least 0.5 blocks (" + d.drift + ")", d.drift >= 0.5);
                    }
                })
                .thenSucceed();
    }

    // ---- test 8: the ice ball ----------------------------------------------------------------------------------

    /** What a run of the ice ball tests knows: a Frostmaw, a rider, and the ice balls seen. */
    private static final class Ice extends Line {
        Mob frost;
        Player rider;
        long placedAt, lastBall, frozenAt = -1, lastFreeze = -1;
        final Set<Integer> seen = new HashSet<>();
        int balls;
        /** Moving form: this pass's balls, whether the rider was frozen in it, and the speeds' tallies. */
        int passBalls;
        boolean frozenInPass;
        final int[] hits = new int[ICE_SPEEDS.length], thrown = new int[ICE_SPEEDS.length];

        Ice(GameTestHelper helper, int side, int northRows) {
            super(helper, side, northRows);
            atEnd.add(() -> {
                if (frost != null)
                    frost.discard();
                if (rider != null)
                    rider.discard();
            });
        }

        /** The rider: a mock player, which the ice ball's entity search sees (a FakePlayers player is not in the entity manager). */
        void mountRider(int carriage) {
            rider = helper.makeMockPlayer(GameType.SURVIVAL);
            sturdy(rider);
            seat(rider, carriage);
        }

        /** Every poll of a wait: the balls in the level, and that the rider is never on two vehicles. */
        void pulse() {
            long now = level.getGameTime();
            AABB around = new AABB(x0 - 10, y - 3, z0 - 40, x0 + LENGTH + 10, y + 16, z0 + 12);
            for (Entity e : level.getEntities((Entity) null, around, x -> true))
                if (ICE_BALL.equals(idOf(e)) && seen.add(e.getId())) {
                    balls++;
                    passBalls++;
                    lastBall = now;
                }
            int vehicles = 0;
            for (Entity e : level.getEntities((Entity) null, rider.getBoundingBox().inflate(16.0), x -> true))
                if (e.getPassengers().contains(rider))
                    vehicles++;
            if (vehicles > 1)
                stop("the rider is on " + vehicles + " vehicles at once");
            if (frozen()) {
                if (frozenAt < 0)
                    frozenAt = now;
                if (now - frozenAt > FREEZE_LIMIT)
                    stop("the freeze has held the rider " + (now - frozenAt) + " ticks (the jar's is 50)");
            } else if (frozenAt >= 0) {
                lastFreeze = now - frozenAt;
                frozenAt = -1;
            }
        }

        boolean frozen() {
            return FROZEN_CONTROLLER.equals(idOf(rider.getVehicle()));
        }
    }

    /**
     * Static form (MZB-370, 373): the train stands at its station with the rider seated; a Frostmaw stands
     * {@value #ICE_DISTANCE} blocks off along the rail, awake with its crystal, kept to its place, its target the
     * rider (set again at every poll, so no goal's reset loses it). Within {@value #ICE_TICKS} ticks an
     * {@code ice_ball} is in the level and the rider's vehicle is the {@code frozen_controller}; then the carriage's
     * seat mapping and passengers no longer hold the rider; and when the freeze ends the rider has no vehicle. The
     * rider is never on two vehicles. (The ball ignoring the carriage and the dismount having no veto are read from
     * the jar; this settles both in the running game.)
     */
    @GameTest(templateNamespace = MicRtsimCreate.MODID, template = YARD, batch = "rail_frostmaw_ice", timeoutTicks = 2000)
    public static void anIceBallUnseatsARiderFromACarriage(GameTestHelper helper) {
        Ice t = new Ice(helper, 4, 1);
        laid(t, true)
                .thenExecute(() -> {
                    t.mountRider(0);
                    double fx = t.rider.getX() - t.dir.getStepX() * ICE_DISTANCE;
                    t.frost = frostmawAt(t.level, fx, t.y + 0.25, t.z0 + 0.5);
                    t.frost.setTarget(t.rider);
                    t.placedAt = t.level.getGameTime();
                    record("railFrostmaw", "ice static", "rider at " + t.rider.position() + ", Frostmaw at " + t.frost.position());
                })
                .thenWaitUntil(() -> {
                    t.frost.setTarget(t.rider);
                    t.pulse();
                    if (t.balls == 0 && t.level.getGameTime() - t.placedAt > ICE_TICKS)
                        t.stop("no ice ball in " + ICE_TICKS + " ticks");
                    check("an ice ball was in the level", t.balls > 0);
                })
                .thenWaitUntil(() -> {
                    t.frost.setTarget(t.rider);
                    t.pulse();
                    if (!t.frozen() && t.level.getGameTime() - t.placedAt > ICE_TICKS)
                        t.stop("the rider was not frozen in " + ICE_TICKS + " ticks (its vehicle: " + idOf(t.rider.getVehicle()) + ")");
                    check("the rider's vehicle is the frozen controller (" + idOf(t.rider.getVehicle()) + ")", t.frozen());
                })
                .thenExecute(() -> {
                    AbstractContraptionEntity carriage = t.entityOf(0);
                    check("carriage 0 still has an entity", carriage != null);
                    check("the carriage's seat mapping no longer holds the rider", !carriage.getContraption().getSeatMapping().containsKey(t.rider.getUUID()));
                    check("the carriage's passengers do not include the rider", !carriage.getPassengers().contains(t.rider));
                    record("railFrostmaw", "ice static", "frozen " + (t.level.getGameTime() - t.placedAt) + " ticks after the Frostmaw stood, balls " + t.balls);
                })
                .thenWaitUntil(() -> {
                    t.pulse();
                    check("the rider has no vehicle (" + idOf(t.rider.getVehicle()) + ")", t.rider.getVehicle() == null);
                })
                .thenExecute(() -> record("railFrostmaw", "ice static", "the freeze let go " + t.lastFreeze + " ticks after it began"))
                .thenSucceed();
    }

    /**
     * Moving form (MZB-373): the same rider on a train held at 0.5, 1.0, 1.4 and 2.0 blocks a tick, {@value #PASSES}
     * passes each, back and forth along the line; the Frostmaw {@value #ICE_OFF} blocks off the rail, abreast of the
     * station. Its target is the rider while the train's front is within {@value #NEAR_X} blocks of abreast (so a
     * ball is thrown at the rider when it comes by), and none otherwise. A pass is a hit when the rider is frozen
     * during it. Between passes the train stands until the rider is free again and {@value #GAP} ticks have gone
     * since the last ball (the jar's cooldown is 200). Logs {@code [gametest] ice ball v=<v>: <hits>/4}. The rates
     * are a measurement, not a gate: the test fails only when the rider is on two vehicles.
     */
    @GameTest(templateNamespace = MicRtsimCreate.MODID, template = YARD, batch = "rail_frostmaw_ice_rates", timeoutTicks = 16000)
    public static void theIceBallHitRatesAgainstAMovingTrain(GameTestHelper helper) {
        Ice t = new Ice(helper, 4, 1);
        // Clear the whole way from the Frostmaw to the rail (a ball explodes on a block): 17 blocks of air, the Frostmaw's pad included
        t.pads.add(new int[] {STATION_AT - 50, STATION_AT + 50, -ICE_OFF - 5, -5});
        double[] bounds = new double[2]; // the ends the passes run between (x)
        double[] frostX = new double[1];

        GameTestSequence s = probed(t, laid(t, true))
                .thenExecute(() -> {
                    t.hold();
                    t.settle();
                    t.train.speed = 0.0;
                    t.mountRider(0);
                    bounds[0] = t.x0 + 15;
                    bounds[1] = t.x0 + LENGTH - 15;
                    frostX[0] = t.x0 + STATION_AT + 0.5;
                    t.frost = frostmawAt(t.level, frostX[0], t.y + 0.25, t.z0 + 0.5 - ICE_OFF);
                    t.lastBall = t.level.getGameTime() - GAP; // ready now
                    record("railFrostmaw", "ice moving", "the train goes " + (t.sign > 0 ? "east" : "west") + " under positive speed, Frostmaw at " + t.frost.position());
                })
                // An approach to the west end, at a speed that costs little time; the first pass runs east
                .thenWaitUntil(() -> {
                    t.hold(-t.sign * 2.0);
                    t.frost.setTarget(null);
                    t.pulse();
                    check("the front is at the west end (" + t.lead() + ")", t.lead() <= bounds[0]);
                });
        int pass = 0;
        for (int v = 0; v < ICE_SPEEDS.length; v++)
            for (int k = 0; k < PASSES; k++, pass++) {
                final int vi = v;
                final double speed = ICE_SPEEDS[v];
                final boolean east = pass % 2 == 0;
                s = s
                        // Rider free and the Frostmaw's cooldown over, the train standing
                        .thenWaitUntil(() -> {
                            t.hold(0.0);
                            t.frost.setTarget(null);
                            t.pulse();
                            long quiet = t.level.getGameTime() - t.lastBall;
                            Entity vehicle = t.rider.getVehicle();
                            check("the rider is free of the freeze (" + idOf(vehicle) + ") and " + quiet + " of " + GAP + " quiet ticks have gone",
                                    !t.frozen() && quiet >= GAP);
                        })
                        .thenExecute(() -> {
                            AbstractContraptionEntity carriage = t.entityOf(0);
                            if (t.rider.getVehicle() != carriage) {
                                t.rider.stopRiding();
                                t.seat(t.rider, 0);
                            }
                            sturdy(t.rider);
                            t.passBalls = 0;
                            t.frozenInPass = false;
                        })
                        .thenWaitUntil(() -> {
                            // t.sign is known only when the probe has run: the signed speed is taken here, not when the sequence is built
                            t.hold((east ? 1.0 : -1.0) * t.sign * speed);
                            t.pulse();
                            boolean near = Math.abs(t.lead() - frostX[0]) <= NEAR_X;
                            t.frost.setTarget(near ? t.rider : null);
                            if (t.frozen())
                                t.frozenInPass = true;
                            check("the front reached the " + (east ? "east" : "west") + " end (" + t.lead() + ")",
                                    east ? t.lead() >= bounds[1] : t.lead() <= bounds[0]);
                        })
                        .thenExecute(() -> {
                            t.hold(0.0);
                            t.frost.setTarget(null);
                            if (t.frozenInPass)
                                t.hits[vi]++;
                            t.thrown[vi] += t.passBalls;
                        });
            }
        s.thenWaitUntil(() -> {
                    t.hold(0.0);
                    t.pulse();
                    check("the rider is free of the last freeze", !t.frozen());
                })
                .thenExecute(() -> {
                    for (int v = 0; v < ICE_SPEEDS.length; v++)
                        MicRtsim.LOGGER.info("[gametest] ice ball v={}: {}/{} (balls thrown at the rider: {})", ICE_SPEEDS[v], t.hits[v], PASSES, t.thrown[v]);
                })
                .thenSucceed();
    }

    // ---- test 9: a lurking Frostmaw ----------------------------------------------------------------------------

    /** What a run of the lurk test knows. */
    private static final class LurkRun extends Line {
        long agent, faction;
        double lx, trackY;
        Vec3 track, spot;
        int dxB;
        String dim;
        Mob puppet;
        ServerPlayer bait;
        FakePlayers.Hearer hearer;
        LivingEntity rider;
        float health0;
        int blocks0;
        long strikes0, holdFrom = -1, passes0;
        double nearest = Double.MAX_VALUE, stray, clearance = Double.MAX_VALUE, hitDeviation = -1, hitBearing;
        boolean targeted, hit, overlapped;
        long op;
        long trainId;
        boolean lurkEnded;

        /** The train's speed in this run (blocks a tick). */
        final double speed;

        LurkRun(GameTestHelper helper, double speed) {
            super(helper, 10, 2);
            this.speed = speed;
            atEnd.add(() -> {
                if (!lurkEnded && agent != 0) {
                    ConflictKit.ambushRemoved(agent, 1, faction, dim);
                    ConflictKit.recall(agent, 1, com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords.RECALL_DONE, 0, x0, z0 + 200);
                }
                for (ServerPlayer p : new ServerPlayer[] {bait, hearer})
                    if (p != null)
                        FakePlayers.remove(p);
                if (rider != null)
                    rider.discard();
                if (puppet != null && !puppet.isRemoved())
                    puppet.discard();
            });
        }

        /** Driven at this run's speed. */
        @Override
        void hold() {
            hold(speed);
        }

        /** Each poll of the hold phase: the puppet near its spot, off the rail, awake. */
        void sampleHold(long now) {
            puppet = RtsimApi.lurkBody(agent);
            if (puppet == null)
                stop("the lurk puppet is gone");
            double dx = puppet.getX() - spot.x, dz = puppet.getZ() - spot.z;
            stray = Math.max(stray, Math.hypot(dx, dz));
            if (stray > 2.0)
                stop("the puppet strayed " + stray + " blocks from its spot (2 allowed)");
            // The Frostmaw is 4 wide: its near edge is its distance to the rail's centre line less 2 (2.0 at the spot,
            // and it stands within 1.5 of the spot, away from the rail while it has the bait as its target)
            double edge = Math.abs(puppet.getZ() - (z0 + 0.5)) - 2.0;
            clearance = Math.min(clearance, edge);
            if (edge < 0.5)
                stop("the puppet's near edge is " + edge + " blocks from the rail's centre line (0.5 needed)");
            if (puppet.getTarget() != null)
                targeted = true;
        }

        /** Each poll of the pass: the swipe's first hit on the rider, and a carriage touching the puppet. */
        void sampleSwipe() {
            if (!hit && rider.getHealth() < health0) {
                hit = true;
                hitBearing = Math.atan2(rider.getZ() - puppet.getZ(), rider.getX() - puppet.getX()) * 57.29577951308232 - 90.0;
                hitDeviation = Math.abs(Mth.wrapDegrees(puppet.yBodyRot - hitBearing));
            }
            for (Carriage c : train.carriages) {
                Entity e = c.anyAvailableEntity();
                if (e != null && e.getBoundingBox().intersects(puppet.getBoundingBox()))
                    overlapped = true;
            }
        }
    }

    private static long soundsOf(FakePlayers.Hearer h) {
        return h.sounds.size();
    }

    /**
     * A lurking Frostmaw stands off the rail and swipes a passing carriage (MZB-359 to 370, 373). A 150-block
     * straight with two stations (the second {@value #B_AWAY} blocks from the first, on the side the train goes), an
     * assembled train with a seated zombie. The engine side is scripted: an AMBUSH_SPOT for a new agent (the track
     * point {@value #LURK_AHEAD} blocks ahead of the train, the spot {@value #LATERAL} blocks off the rail), a camp
     * with the roar as its sound (the faction key the lurk and TRAIN_ROBBED carry), then BAND_MATERIALIZE of one
     * Frostmaw and a bait player 30 blocks from the spot, set as the puppet's target and walking away.
     *
     * <ul>
     * <li>For {@value #HOLD_TICKS} ticks the puppet stays within 2 blocks of the spot, its near edge (the spot's is 2.0
     * from the rail's centre line) at least 0.5 blocks off it, and its NBT has {@code alwaysActive}.
     * <li>The bait goes, and the train passes at 1.0 blocks a tick: exactly one TRAIN_PASSED with PASSED_WATCHED and
     * a carriage in range; {@code lurkStrikes} up by one or more; at the tick the zombie's health first drops, the
     * puppet's {@code yBodyRot} within 30 degrees of the bearing to the zombie; the zombie hurt; no carriage box
     * ever touching the puppet's; the contraptions' block count unchanged; the train on the rail.
     * <li>A scripted IND_CARRIAGE_DELTA for the train lands in the second carriage's storage.
     * <li>A scripted TRAIN_ROBBED. The train's arrival at the far station, with nobody within 64 blocks, plays
     * nothing and the entry stays; then its arrival at the first, with a hearer {@value #HEARER_BEHIND} blocks off: the hearer's
     * {@code sounds} gains one packet, the roar's, and its {@code raw}, {@code overlay} and {@code titles} nothing.
     * <li>A scripted AMBUSH_SPOT removed: {@code alwaysActive} false, no restriction, the agent gone from
     * {@code RtsimApi.lurks()}.
     * </ul>
     *
     * The hearer stands {@value #HEARER_BEHIND} blocks behind the first station, away from the lurk: about 61
     * blocks from the puppet, so it is no target (50) and out of the puppet's own sounds, and 80 from the second
     * station. The train's front ends {@value #LURK_AHEAD} plus {@value #PAST} blocks past the station, inside the track.
     */
    @GameTest(templateNamespace = MicRtsimCreate.MODID, template = YARD, batch = "rail_frostmaw_lurk", timeoutTicks = 3600)
    public static void aLurkingFrostmawStandsOffTheRailAndSwipesAPassingCarriage(GameTestHelper helper) {
        lurk(helper, 1.0, true);
    }

    /** The pass of test 9 at half a block a tick, on the engine's own spot: the strike still lands (review C, item 2). */
    @GameTest(templateNamespace = MicRtsimCreate.MODID, template = YARD, batch = "rail_frostmaw_lurk_slow", timeoutTicks = 2400)
    public static void aLurkingFrostmawSwipesACarriageAtHalfABlockATick(GameTestHelper helper) {
        lurk(helper, 0.5, false);
    }

    /** The pass of test 9 at 1.4 blocks a tick (Create's default top speed is about that), on the engine's own spot. */
    @GameTest(templateNamespace = MicRtsimCreate.MODID, template = YARD, batch = "rail_frostmaw_lurk_fast", timeoutTicks = 2400)
    public static void aLurkingFrostmawSwipesACarriageAtOnePointFourBlocksATick(GameTestHelper helper) {
        lurk(helper, 1.4, false);
    }

    /**
     * Test 9's body at {@code speed}. {@code full} is the whole of it (the hold with its bait, the hearer, the
     * cargo delta, the robbery and its roar, the lurk's end); not full it is the pass alone: the puppet placed, a
     * zombie seated, the train driven past, the swipe's hit, TRAIN_PASSED.
     */
    private static void lurk(GameTestHelper helper, double speed, boolean full) {
        LurkRun t = new LurkRun(helper, speed);
        ServerLevel level = t.level;

        GameTestSequence s = probed(t, laid(t, true))
                .thenExecute(() -> {
                    t.hold();
                    t.settle();
                    t.train.speed = 0.0;
                    t.trainId = CreateIds.of(t.train.id);
                    t.lx = Math.floor(t.lead() + t.sign * LURK_AHEAD) + 0.5;
                    t.dxB = STATION_AT + (int) (t.sign * B_AWAY);
                    t.stations.add(t.dxB);
                    t.placeStation(t.dxB);
                    Create.RAILWAYS.markTracksDirty();
                    t.dim = level.dimension().location().toString();
                    record("railFrostmaw", "lurk", "the train goes " + (t.sign > 0 ? "east" : "west") + " under positive speed, lurk point x " + t.lx
                            + ", second station at offset " + t.dxB);
                })
                .thenWaitUntil(() -> check("the second station is on the graph", t.stationAt(t.dxB).getStation() != null))
                // The camp (its sound is what the roar plays), then the lurk
                .thenExecute(() -> {
                    t.agent = ConflictKit.newAgent();
                    t.trackY = t.railY();
                    t.track = new Vec3(t.lx, t.trackY, t.z0 + 0.5);
                    t.spot = new Vec3(t.lx, t.trackY, t.z0 + 0.5 - LATERAL);
                    Holder<SoundEvent> roar = BuiltInRegistries.SOUND_EVENT.getHolder(ResourceLocation.parse(ROAR))
                            .orElseThrow(() -> new GameTestAssertException("no sound " + ROAR));
                    t.faction = ConflictKit.bridge().reportCamp(level, CAMP, BlockPos.containing(t.spot), e -> false, roar, false, 0, 0);
                    check("a camp was reported", t.faction != 0);
                    ConflictKit.ambushSpot(t.agent, 1, t.faction, t.dim, new double[] {t.track.x, t.track.y, t.track.z},
                            new double[] {t.spot.x, t.spot.y, t.spot.z}, REACH);
                })
                .thenWaitUntil(paced(() -> check("the lurk is in the table", RtsimApi.lurks().containsKey(t.agent))))
                .thenExecute(() -> {
                    ConflictKit.Band b = ConflictKit.band(t.agent, t.faction, t.dim);
                    b.arcX = t.spot.x;
                    b.arcZ = t.spot.z;
                    b.targetX = t.track.x;
                    b.targetY = t.track.y;
                    b.targetZ = t.track.z;
                    b.speed = 0.0f;
                    ConflictKit.materialize(b, List.of(new ConflictKit.StackSpec(FROSTMAW, 1, false)));
                })
                .thenWaitUntil(paced(() -> {
                    t.puppet = RtsimApi.lurkBody(t.agent);
                    check("the puppet stands in the world", t.puppet != null);
                }));
        if (full) {
            s = s.thenExecute(() -> {
                    t.bait = FakePlayers.add(level, new Vec3(t.spot.x, t.spot.y, t.spot.z - 30.0), "frostmaw-bait");
                    t.holdFrom = level.getGameTime();
                    record("railFrostmaw", "lurk", "puppet at " + t.puppet.position() + ", spot " + t.spot + ", the bait at " + t.bait.position());
                })
                // The hold: the bait is the target and walks away (0.05 blocks a tick, 10 over the phase), the puppet stays
                .thenWaitUntil(() -> {
                    long now = level.getGameTime();
                    t.sampleHold(now);
                    t.puppet.setTarget(t.bait);
                    t.bait.setPos(t.spot.x, t.spot.y, t.spot.z - 30.0 - 0.05 * (now - t.holdFrom));
                    check("the hold ran " + (now - t.holdFrom) + " of " + HOLD_TICKS + " ticks", now - t.holdFrom >= HOLD_TICKS);
                })
                .thenExecute(() -> {
                    check("the puppet had a target", t.targeted);
                    check("alwaysActive is set", t.puppet.saveWithoutId(new CompoundTag()).getBoolean("alwaysActive"));
                    check("the lurk is still in the table", RtsimApi.lurks().containsKey(t.agent));
                    record("railFrostmaw", "lurk", "held " + HOLD_TICKS + " ticks: strayed at most " + t.stray + " blocks, near edge at least " + t.clearance
                            + " blocks from the rail's centre line");
                    // The bait goes: out of the list, out of view
                    FakePlayers.remove(t.bait);
                    t.bait.setPos(t.spot.x, t.spot.y, t.spot.z - 300.0);
                    t.puppet.setTarget(null);
                })
                .thenIdle(60);
        } else {
            // The sampler's next sample (once in 20 ticks) puts the train in the near set, now the puppet lives
            s = s.thenIdle(40);
        }
        s = s.thenExecute(() -> {
                    // The hearer stands behind the first station (away from the lurk), the zombie rides the first carriage
                    double apart = -1.0;
                    if (full) {
                        t.hearer = FakePlayers.hearing(level, new Vec3(t.x0 + STATION_AT + 0.5 - t.sign * HEARER_BEHIND, t.y, t.z0 + 0.5 + 3.0), "frostmaw-hearer");
                        apart = Math.hypot(t.hearer.getX() - t.puppet.getX(), t.hearer.getZ() - t.puppet.getZ());
                        check("the hearer is more than 56 blocks from the puppet (" + apart + ")", apart > 56.0);
                    }
                    if (!(typeOf(ZOMBIE).create(level) instanceof LivingEntity z))
                        throw new GameTestAssertException("could not create a zombie");
                    t.rider = z;
                    if (z instanceof Mob m) {
                        m.setNoAi(true);
                        m.setPersistenceRequired();
                    }
                    z.addEffect(new MobEffectInstance(MobEffects.FIRE_RESISTANCE, 1_000_000, 0, false, false));
                    sturdy(z);
                    t.seat(z, 0);
                    t.health0 = z.getHealth();
                    t.blocks0 = t.blocks();
                    t.strikes0 = ConflictKit.lurkStrikes();
                    t.passes0 = ConflictKit.bridge().trainPasses;
                    record("railFrostmaw", "lurk", "the zombie rides carriage 0 at " + z.position() + ", train speed " + t.speed
                            + (full ? ", hearer " + apart + " blocks from the puppet" : ""));
                })
                // The pass
                .thenWaitUntil(() -> {
                    t.hold();
                    t.sampleSwipe();
                    double beyond = (t.lead() - t.lx) * t.sign;
                    check("the train is " + PAST + " blocks past the lurk point (" + beyond + ")", beyond >= PAST);
                })
                .thenExecute(() -> {
                    t.train.speed = 0.0;
                    long strikes = ConflictKit.lurkStrikes() - t.strikes0;
                    record("railFrostmaw", "lurk", "pass at " + t.speed + ": strikes " + strikes + ", zombie health " + t.rider.getHealth() + " of " + t.health0
                            + ", yBodyRot off the bearing to the zombie by " + t.hitDeviation + " degrees, overlapped " + t.overlapped);
                    check("the train is on the rail", !t.train.derailed && !t.train.invalid);
                    check("the lurk struck at " + t.speed + " (" + strikes + ")", strikes >= 1);
                    check("the zombie was hurt at " + t.speed + " (health " + t.rider.getHealth() + " of " + t.health0 + ")", t.hit);
                    check("the puppet faced the zombie within 30 degrees at the hit (" + t.hitDeviation + ")", t.hitDeviation >= 0 && t.hitDeviation <= 30.0);
                    check("no carriage box touched the puppet's", !t.overlapped);
                    check("the contraptions' block count is unchanged (" + t.blocks() + " of " + t.blocks0 + ")", t.blocks() == t.blocks0);
                })
                .thenIdle(60)
                .thenWaitUntil(paced(() -> {
                    List<ScriptedEngine.Received> passed = ConflictKit.trainsPassed(t.agent);
                    check("exactly one TRAIN_PASSED (" + passed.size() + ")", passed.size() == 1);
                    ConflictKit.TrainPassedRecord r = ConflictKit.trainPassed(passed.get(0));
                    check("PASSED_WATCHED", r.watched());
                    check("a carriage in range (" + r.carriage() + ")", r.carriage() >= 0 && r.carriage() < t.train.carriages.size());
                    check("the train's id", r.train() == t.trainId);
                }));
        if (!full) {
            s.thenSucceed();
            return;
        }
        // IND_CARRIAGE_DELTA for the train that was struck
        s.thenExecute(() -> {
                    t.op = IndustryKit.engineOp();
                    IndustryKit.carriageDelta(t.op, t.trainId, IndRecords.CarriageDelta.EXCHANGE,
                            List.<Object[]>of(new Object[] {1, "i/64/minecraft:copper_ingot", 5L}));
                })
                .thenWaitUntil(paced(() -> {
                    check("IND_CARRIAGE_RESULT", IndustryKit.received(IndRecords.IND_CARRIAGE_RESULT, IndRecords.CarriageResult.OP_ID, t.op).size() == 1);
                    check("the delta landed in the second carriage's storage (" + copperIn(t.train, 1) + ")", copperIn(t.train, 1) == 5);
                }))
                // TRAIN_ROBBED: the train waits for its roar
                .thenExecute(() -> {
                    t.passes0 = ConflictKit.bridge().roarsTold;
                    ConflictKit.trainRobbed(t.faction, t.trainId);
                })
                .thenWaitUntil(paced(() -> check("the train waits for its roar", ConflictKit.bridge().roarPending(t.trainId))))
                .thenExecute(() -> {
                    check("the puppet was told to roar", ConflictKit.bridge().roarsTold > t.passes0);
                    // Its arrival at the far station: nobody within 64 blocks
                    FakePlayers.Hearer h = t.hearer;
                    int sounds = h.sounds.size(), raw = h.raw.size(), overlay = h.overlay.size(), titles = h.titles.size();
                    long played = ConflictKit.bridge().roarsPlayed;
                    t.train.arriveAt(t.stationAt(t.dxB).getStation());
                    check("nothing played at the far station", h.sounds.size() == sounds && h.raw.size() == raw && h.overlay.size() == overlay
                            && h.titles.size() == titles && ConflictKit.bridge().roarsPlayed == played);
                    check("the entry stays", ConflictKit.bridge().roarPending(t.trainId));
                })
                .thenExecute(() -> {
                    // Its arrival at the first station, the hearer 20 blocks off
                    FakePlayers.Hearer h = t.hearer;
                    int sounds = h.sounds.size(), raw = h.raw.size(), overlay = h.overlay.size(), titles = h.titles.size();
                    t.train.arriveAt(t.stationAt(STATION_AT).getStation());
                    check("one sound packet (" + (h.sounds.size() - sounds) + ")", h.sounds.size() - sounds == 1);
                    ClientboundSoundPacket packet = h.sounds.get(h.sounds.size() - 1);
                    check("the roar's sound (" + packet.getSound().value().getLocation() + ")", ROAR.equals(packet.getSound().value().getLocation().toString()));
                    check("no chat line, action bar line or title", h.raw.size() == raw && h.overlay.size() == overlay && h.titles.size() == titles);
                    check("the entry is gone", !ConflictKit.bridge().roarPending(t.trainId));
                })
                // The lurk ends
                .thenExecute(() -> {
                    t.lurkEnded = true;
                    ConflictKit.ambushRemoved(t.agent, 1, t.faction, t.dim);
                })
                .thenWaitUntil(paced(() -> {
                    check("the agent is gone from the lurk table", !RtsimApi.lurks().containsKey(t.agent));
                    check("alwaysActive is false", !t.puppet.saveWithoutId(new CompoundTag()).getBoolean("alwaysActive"));
                    check("no restriction", !t.puppet.hasRestriction());
                }))
                .thenExecute(() -> ConflictKit.recall(t.agent, 1, com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords.RECALL_DONE, 0,
                        t.x0, t.z0 + 200))
                .thenWaitUntil(paced(() -> check("BAND_DEMATERIALIZED",
                        !ConflictKit.about(com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords.BAND_DEMATERIALIZED, t.agent).isEmpty())))
                .thenSucceed();
    }

    /** The copper ingots in a carriage's storage. */
    private static long copperIn(Train train, int carriage) {
        IItemHandler h = train.carriages.get(carriage).storage.getAllItems();
        long n = 0;
        for (int s = 0; s < h.getSlots(); s++)
            if (h.getStackInSlot(s).is(Items.COPPER_INGOT))
                n += h.getStackInSlot(s).getCount();
        return n;
    }
}
