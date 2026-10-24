package com.minecraftindustrialcomplex.mic_rtsim_create.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.Create;
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
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.record;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.IndustryKit.YARD;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.chest;
import static com.minecraftindustrialcomplex.mic_rtsim_create.gametest.glass.GlassCorpusKit.fill;

/**
 * A Create train through a Frostmaw (MOWZIE.md section 3.1, docs/choices-mowzie-build.md MZB-328 and 329, test
 * 10): a real assembled train is driven down a straight of real track, past a Frostmaw standing on the rail,
 * and runs through it. Create's contraption collision moves only entities whose push reaction lets a contraption
 * push them, and the Frostmaw's is IGNORE, so the carriage's box overlaps the boss and nothing happens to
 * either. A zombie, the control, is moved.
 *
 * <p>This class has no {@code @GameTestHolder}: {@link MicRtsimCreate} registers it from
 * {@code RegisterGameTestsEvent} only when Mowzie's Mobs is loaded ({@code -PwithMowzie}), and its templates
 * name this mod's namespace. It names the Frostmaw by its registry id and no Mowzie class. M8's tests 8 and 9
 * (the ice ball, the lurk) go in this class too.
 *
 * <p>The track is {@value #LENGTH} blocks of real track along x at a far base (its chunks forced, a stone floor
 * under it and the air cleared above, all taken out again at the end so Create's saved graph holds nothing of
 * it), a station in the middle on the south side, and the train built beside the station as
 * {@code RailFixtureGameTests} builds its own: two small bogeys, controls on the first, a chest on the second,
 * each glued. Create's own tick drives the train at {@value #SPEED} blocks a tick ({@code manualTick}, reset
 * before every tick, and {@code speed}); which way it goes is found by running it three ticks; the entity stands
 * on the rail {@value #AHEAD} blocks ahead of the leading point. The run ends when the train is {@value #PAST}
 * blocks past it (the control: when it has moved the zombie half a block).
 *
 * <p>The distance check counts observations, not game time: between two of this test's ticks Create ticks the
 * train once, whichever of the two runs first in a server tick, so the leading point has moved
 * {@code SPEED} blocks per observation after the first.
 */
@PrefixGameTestTemplate(false)
public final class RailFrostmawGameTests {
    private RailFrostmawGameTests() {}

    public static final String FROSTMAW = "mowziesmobs:frostmaw", ZOMBIE = "minecraft:zombie";
    static final double SPEED = 1.0;
    static final int LENGTH = 150, STATION_AT = 75, AHEAD = 32, PAST = 20;
    /** Observations before the entity's start is taken (it settles on the rail; the train is still far). */
    static final int SETTLE = 3;

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

    /** What one run knows. */
    private static final class Drive {
        final GameTestHelper helper;
        final String entityId;
        final boolean boss;
        int x0, z0, y;
        Train train;
        Entity target;
        Vec3 targetStart;
        float health0;
        double sign, leadAtFirstHold;
        long firstHold = -1;
        /** Observations since the entity was placed, and the leading point at the placing tick and at the last one. */
        int observed;
        double runFrom, runTo;
        boolean overlapped, hurt, healthChanged;
        double drift;

        Drive(GameTestHelper helper, String entityId, boolean boss) {
            this.helper = helper;
            this.entityId = entityId;
            this.boss = boss;
        }

        BlockPos track(int dx) {
            return new BlockPos(x0 + dx, y, z0);
        }

        double lead() {
            return train.carriages.get(0).getLeadingPoint().getPosition(train.graph).x;
        }

        /** Ends the test at once (a waiting step would only retry until its timeout). */
        void stop(String why) {
            GameTestAssertException e = new GameTestAssertException(why);
            helper.testInfo.fail(e);
            throw e;
        }

        /** Before Create's tick, every tick the train moves: driven at {@link #SPEED}. */
        void hold() {
            if (Create.RAILWAYS.trains.get(train.id) != train)
                stop("the train is no longer Create's");
            if (train.derailed || train.invalid)
                stop("the train derailed (derailed " + train.derailed + ", invalid " + train.invalid + ")");
            train.manualTick = true;
            train.speed = SPEED;
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
        int[] base = ConflictKit.farBase();
        d.x0 = base[0];
        d.z0 = base[1] + 8; // the middle of a chunk, with the station's block beside the track in it
        int cx0 = d.x0 >> 4, cx1 = (d.x0 + LENGTH) >> 4, cz = d.z0 >> 4;
        UUID assembler = UUID.nameUUIDFromBytes(("mic-rtsim frostmaw pass " + System.nanoTime()).getBytes());
        Direction[] dir = {null};

        ConflictKit.onEnd(helper, () -> {
            if (d.train != null) {
                for (Carriage c : d.train.carriages) {
                    Entity e = c.anyAvailableEntity();
                    if (e != null)
                        e.discard();
                }
                Create.RAILWAYS.removeTrain(d.train.id);
            }
            if (d.target != null)
                d.target.discard();
            // The track and the station out again (they take their graph with them), then the chunks
            if (d.y != 0) {
                for (int dx = 0; dx < LENGTH; dx++)
                    level.setBlock(d.track(dx), Blocks.AIR.defaultBlockState(), 3);
                level.setBlock(d.track(STATION_AT).south(), Blocks.AIR.defaultBlockState(), 3);
            }
            for (int cx = cx0; cx <= cx1; cx++)
                for (int z = cz - 1; z <= cz + 1; z++)
                    level.setChunkForced(cx, z, false);
        });

        helper.startSequence()
                .thenExecute(() -> {
                    ConflictKit.releaseFarChunks(level);
                    for (int cx = cx0; cx <= cx1; cx++)
                        for (int z = cz - 1; z <= cz + 1; z++) {
                            level.setChunkForced(cx, z, true);
                            level.getChunk(cx, z);
                        }
                    d.y = level.getHeight(Heightmap.Types.WORLD_SURFACE, d.x0 + STATION_AT, d.z0);
                    record("railFrostmaw", "track height", d.y);
                    BlockState air = Blocks.AIR.defaultBlockState(), floor = Blocks.STONE.defaultBlockState();
                    BlockState xo = AllBlocks.TRACK.getDefaultState().setValue(TrackBlock.SHAPE, TrackShape.XO);
                    // Whatever the terrain: a floor one below the rail and clear air above (a Frostmaw is 4 wide and 4 tall)
                    for (int dx = -3; dx < LENGTH + 3; dx++)
                        for (int dz = -4; dz <= 4; dz++) {
                            level.setBlock(new BlockPos(d.x0 + dx, d.y - 1, d.z0 + dz), floor, 2);
                            for (int dy = 0; dy < 8; dy++)
                                level.setBlock(new BlockPos(d.x0 + dx, d.y + dy, d.z0 + dz), air, 2);
                        }
                    for (int dx = 0; dx < LENGTH; dx++)
                        level.setBlock(d.track(dx), xo, 3);
                })
                .thenWaitUntil(() -> {
                    boolean found = false;
                    for (TrackGraph g : Create.RAILWAYS.trackNetworks.values())
                        for (TrackNodeLocation loc : g.getNodes()) {
                            Vec3 p = loc.getLocation();
                            if (p.x >= d.x0 - 1 && p.x <= d.x0 + LENGTH + 1 && Math.abs(p.z - (d.z0 + 0.5)) < 1.5)
                                found = true;
                        }
                    check("the track has a graph", found);
                })
                .thenExecute(() -> RailFixtureGameTests.point(level, d.track(STATION_AT).south(), AllBlocks.TRACK_STATION.getDefaultState(),
                        new BlockPos(0, 0, -1)))
                .thenWaitUntil(() -> check("the station is on the graph",
                        RailFixtureGameTests.station(level, d.track(STATION_AT).south()).getStation() != null))
                .thenExecute(() -> {
                    StationBlockEntity sbe = RailFixtureGameTests.station(level, d.track(STATION_AT).south());
                    sbe.updateName("Frostmaw Pass");
                    dir[0] = sbe.getAssemblyDirection();
                    check("an assembly direction along the track", dir[0] != null && dir[0].getAxis() == Direction.Axis.X);
                    cars(helper, d.track(STATION_AT), dir[0]);
                    Create.RAILWAYS.markTracksDirty();
                })
                .thenIdle(5)
                .thenExecute(() -> check("assembly mode", RailFixtureGameTests.station(level, d.track(STATION_AT).south()).enterAssemblyMode(null)))
                .thenIdle(5)
                .thenExecute(() -> {
                    StationBlockEntity sbe = RailFixtureGameTests.station(level, d.track(STATION_AT).south());
                    sbe.assemble(assembler);
                    GlobalStation st = sbe.getStation();
                    d.train = st == null ? null : st.getPresentTrain();
                    if (d.train == null)
                        throw new GameTestAssertException("no train assembled: " + sbe.saveWithFullMetadata(level.registryAccess()).get("LastException"));
                    d.train.name = Component.literal("Frostmaw Pass Express");
                    sbe.exitAssemblyMode();
                    Create.RAILWAYS.markTracksDirty();
                })
                .thenWaitUntil(() -> {
                    check("the train is on the rail", d.train.graph != null && !d.train.derailed);
                    for (Carriage c : d.train.carriages)
                        check("a carriage entity stands", c.anyAvailableEntity() != null);
                })
                // Three ticks of drive tell which way the train's front goes
                .thenWaitUntil(() -> {
                    long now = level.getGameTime();
                    if (d.firstHold < 0) {
                        d.firstHold = now;
                        d.leadAtFirstHold = d.lead();
                    }
                    d.hold();
                    check("three ticks of drive (tick " + (now - d.firstHold) + ")", now - d.firstHold >= 3);
                    check("the train moves along x under manual drive (" + (d.lead() - d.leadAtFirstHold) + ")",
                            Math.abs(d.lead() - d.leadAtFirstHold) > 0.5);
                })
                .thenExecute(() -> {
                    d.hold();
                    d.sign = Math.signum(d.lead() - d.leadAtFirstHold);
                    double at = d.lead() + d.sign * AHEAD;
                    EntityType<?> type = BuiltInRegistries.ENTITY_TYPE.getOptional(ResourceLocation.parse(d.entityId))
                            .orElseThrow(() -> new GameTestAssertException("no entity type " + d.entityId));
                    d.target = type.create(level);
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

    /**
     * Two single-bogey carriages on the station's track, as a player builds them ({@code RailFixtureGameTests.cars}):
     * a small bogey at the first block past the station and one four further, controls facing the assembly
     * direction on the first, a chest with coal on the second, each glued.
     */
    private static void cars(GameTestHelper h, BlockPos stationTrack, Direction d) {
        ServerLevel level = h.getLevel();
        BlockState bogey = AllBlocks.SMALL_BOGEY.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_AXIS, Direction.Axis.X);
        BlockPos b1 = stationTrack.relative(d, 1).above(), b2 = stationTrack.relative(d, 5).above();
        level.setBlock(b1, bogey, 3);
        level.setBlock(b2, bogey, 3);
        level.setBlock(b1.above(), AllBlocks.TRAIN_CONTROLS.getDefaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, d), 3);
        chest(h, b2.above());
        fill(h, b2.above(), Items.COAL, 32);
        level.addFreshEntity(new SuperGlueEntity(level, SuperGlueEntity.span(b1, b1.above())));
        level.addFreshEntity(new SuperGlueEntity(level, SuperGlueEntity.span(b2, b2.above())));
    }
}
