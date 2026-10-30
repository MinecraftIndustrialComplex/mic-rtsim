package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.client.particle.ParticleHandler;
import com.bobmowzie.mowziesmobs.server.entity.effects.EntityCameraShake;
import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.bobmowzie.mowziesmobs.server.sound.MMSounds;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ChunkWait;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.BossArrival;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Den;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import io.netty.buffer.Unpooled;
import net.minecraft.core.Holder;
import net.minecraft.core.particles.ParticleType;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundLevelParticlesPacket;
import net.minecraft.network.protocol.game.ClientboundSoundPacket;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * A boss that is due is placed in front of a player who stays, and arrives with an effect (slice S9, FB-S9-3; Ben, 2026-10-30:
 * "just spawn anyway, bonus points if you have some cool effects"; MZB-1272 to MZB-1279): the Frostmaw at its den and the Umvuthi on
 * its throne, as a SPAWN and as an heir, with a player standing beside them. The scripted engine sends the lines, the real kinds
 * play them. A hearer ({@link FakePlayers#overhearing}) stands 6 blocks off and keeps every packet it is sent: the effect is the
 * particles, sounds and camera shake {@link BossArrival} sends, read as packets; every one of them is also sent through the wire
 * codecs, so a type the jar registered with no codec that works would fail here, not on a client. A second hearer 150 blocks off is
 * sent nothing. A grove's warrior is placed quietly, as before.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class BossArrivalGameTests {
    private BossArrivalGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;

    private static final int DONE = ConflictRecords.CampBodies.DONE;
    private static final int SPAWN = ConflictRecords.CampReconcile.SPAWN, ADOPT = ConflictRecords.CampReconcile.ADOPT,
            MASK = ConflictRecords.CampReconcile.MASK, SEAT_HEIR = ConflictRecords.CampReconcile.SEAT_HEIR;
    private static final int CHIEF_SLOT = ConflictRecords.CampReconcile.CHIEF_SLOT, CHIEF = ConflictRecords.CampReconcile.LINE_CHIEF,
            CRYSTAL = ConflictRecords.CampReconcile.LINE_CRYSTAL;

    /** The most particle packets an arrival may send a player: one burst, not a stream. */
    private static final int SHORT = 100;

    /** What a hearer was sent since a mark: its particle packets and its sounds. */
    private record Sent(List<ClientboundLevelParticlesPacket> particles, List<ClientboundSoundPacket> sounds) {
        static Sent since(FakePlayers.Hearer h, int packetMark, int soundMark) {
            List<ClientboundLevelParticlesPacket> ps = new ArrayList<>();
            for (var p : h.packets.subList(packetMark, h.packets.size()))
                if (p instanceof ClientboundLevelParticlesPacket q)
                    ps.add(q);
            return new Sent(ps, new ArrayList<>(h.sounds.subList(soundMark, h.sounds.size())));
        }

        long of(ParticleType<?> type) {
            return particles.stream().filter(p -> p.getParticle().getType() == type).count();
        }

        boolean heard(Holder<SoundEvent> sound) {
            return sounds.stream().anyMatch(p -> p.getSound().value() == sound.value());
        }
    }

    /** Every packet of {@code sent} survives the wire: encoded and decoded by vanilla's own codecs, the same type, count and sound. */
    private static void survivesTheWire(ServerLevel level, Sent sent) {
        RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), level.registryAccess());
        for (ClientboundLevelParticlesPacket p : sent.particles) {
            buf.clear();
            ClientboundLevelParticlesPacket.STREAM_CODEC.encode(buf, p);
            ClientboundLevelParticlesPacket back = ClientboundLevelParticlesPacket.STREAM_CODEC.decode(buf);
            check("a particle packet of " + p.getParticle().getType() + " survives the wire",
                    back.getParticle().getType() == p.getParticle().getType() && back.getCount() == p.getCount() && buf.readableBytes() == 0);
        }
        for (ClientboundSoundPacket p : sent.sounds) {
            buf.clear();
            ClientboundSoundPacket.STREAM_CODEC.encode(buf, p);
            ClientboundSoundPacket back = ClientboundSoundPacket.STREAM_CODEC.decode(buf);
            check("a sound packet of " + p.getSound().value().getLocation() + " survives the wire",
                    back.getSound().value().getLocation().equals(p.getSound().value().getLocation()) && buf.readableBytes() == 0);
        }
    }

    private static long shakes(ServerLevel level, Vec3 at) {
        return level.getEntitiesOfClass(EntityCameraShake.class, new AABB(at.add(-48, -48, -48), at.add(48, 48, 48))).size();
    }

    /**
     * A Frostmaw SPAWN with a player 6 blocks from the den, another 150 blocks off, and a den that stays asleep. The line is
     * DONE at once (placed within 200 ticks of being sent, where it was held for up to 6,000), nothing is held, the Frostmaw
     * stands at the spot asleep with its crystal, tagged, and the arrival is played once: the near hearer is sent between 30 and
     * 100 particle packets (the jar's roar ring of 30 clouds in two rings, its snowflakes, vanilla's snow, cloud and ice shards)
     * and the jar's frozen crash, landing thump and roar, and one camera shake stands at the spot; all survive the wire. The far
     * hearer is sent no particle and no sound.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_arrival_frostmaw", skyAccess = true)
    public static void aDueFrostmawIsPlacedAtOnceBesideAPlayerWithItsArrival(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 3000);
        String test = "aDueFrostmawIsPlacedAtOnceBesideAPlayerWithItsArrival";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Den den = new Den(far, far.k, far.z0);
        FakePlayers.Hearer[] hearer = new FakePlayers.Hearer[2];
        int[] mark = new int[4];
        long[] was = new long[4], op = new long[1], sent = new long[2];
        Vec3[] at = new Vec3[1];

        MowzieKit.seq(helper)
                .thenExecute(den::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    den.lay().declare();
                    hearer[0] = FakePlayers.overhearing(level, den.spot.add(6, 1, 0), "mowzie-arrival-near");
                    hearer[1] = FakePlayers.overhearing(level, den.spot.add(150, 1, 0), "mowzie-arrival-far");
                    ConflictKit.onEnd(helper, () -> {
                        FakePlayers.remove(hearer[0]);
                        FakePlayers.remove(hearer[1]);
                    });
                    MowzieKit.everyTick(helper, () -> {
                        if (sent[1] == 0 && !MowzieKit.held(den.faction, 0, 1).isEmpty())
                            sent[1] = level.getGameTime();
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("census 1", MowzieKit.censusCount(den.faction) >= 1)))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    was[0] = camps.heldSpawns;
                    was[1] = BossArrival.FROSTMAWS.get();
                    was[2] = camps.arrivedBodies;
                    mark[0] = hearer[0].packets.size();
                    mark[1] = hearer[0].sounds.size();
                    mark[2] = hearer[1].packets.size();
                    mark[3] = hearer[1].sounds.size();
                    sent[0] = level.getGameTime();
                    op[0] = MowzieKit.reconcile(den.faction, MowzieKit.epoch(MowzieKit.census(den.faction, 1)),
                            List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, SPAWN, CHIEF | CRYSTAL)));
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(den.faction, op[0], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the SPAWN with a player 6 blocks from the den", ConflictKit.outcomes(den.faction, op[0]), DONE);
                    long after = sent[1] - sent[0];
                    check("placed at once, not held (" + after + " ticks after it was sent)", sent[1] != 0 && after <= 200);
                    check("nothing was held (" + (MowzieKit.camps().heldSpawns - was[0]) + ")",
                            MowzieKit.camps().heldSpawns == was[0] && MowzieKit.camps().heldLines() == 0);
                    List<Entity> h = MowzieKit.held(den.faction, 0, 1);
                    check("a Frostmaw is the den's (" + h.size() + ")", h.size() == 1 && h.get(0) instanceof EntityFrostmaw);
                    EntityFrostmaw f = (EntityFrostmaw) h.get(0);
                    at[0] = f.position();
                    check("at the spot, asleep with its crystal (" + f.position().distanceTo(den.spot) + ")",
                            f.position().distanceTo(den.spot) < 1.5 && f.getHasCrystal() && !f.getActive());
                    check("the player stood within 160 blocks all along", hearer[0].distanceToSqr(den.spot) < 160 * 160);
                    check("the arrival was played once (" + (BossArrival.FROSTMAWS.get() - was[1]) + ")", BossArrival.FROSTMAWS.get() == was[1] + 1);
                    check("and the camp counted the body's arrival (" + (MowzieKit.camps().arrivedBodies - was[2]) + ")",
                            MowzieKit.camps().arrivedBodies == was[2] + 1);

                    Sent near = Sent.since(hearer[0], mark[0], mark[1]);
                    check("the jar's roar ring: 30 clouds (" + near.of(ParticleHandler.CLOUD.get()) + ")", near.of(ParticleHandler.CLOUD.get()) == 30);
                    check("the jar's snowflakes (" + near.of(ParticleHandler.SNOWFLAKE.get()) + ")", near.of(ParticleHandler.SNOWFLAKE.get()) >= 1);
                    check("vanilla's snow, cloud and ice shards (" + near.of(ParticleTypes.SNOWFLAKE) + ", " + near.of(ParticleTypes.CLOUD) + ", "
                            + near.of(ParticleTypes.BLOCK) + ")", near.of(ParticleTypes.SNOWFLAKE) >= 1 && near.of(ParticleTypes.CLOUD) >= 1
                            && near.of(ParticleTypes.BLOCK) >= 1);
                    check("one burst, not a stream (" + near.particles.size() + " particle packets)", near.particles.size() >= 30 && near.particles.size() <= SHORT);
                    check("the jar's frozen crash, landing thump and roar (" + near.sounds.size() + " sounds)",
                            near.heard(MMSounds.ENTITY_FROSTMAW_FROZEN_CRASH) && near.heard(MMSounds.ENTITY_FROSTMAW_LAND) && near.heard(MMSounds.ENTITY_FROSTMAW_ROAR)
                                    && near.sounds.size() == 3);
                    check("one camera shake stands at the spot (" + shakes(level, at[0]) + ")", shakes(level, at[0]) == 1);
                    survivesTheWire(level, near);

                    Sent far150 = Sent.since(hearer[1], mark[2], mark[3]);
                    check("the player 150 blocks off was sent nothing (" + far150.particles.size() + " particles, " + far150.sounds.size() + " sounds)",
                            far150.particles.isEmpty() && far150.sounds.isEmpty());
                    record(test, "arrival", "the SPAWN was DONE " + after + " ticks after it was sent with a player 6 blocks off; " + near.particles.size()
                            + " particle packets (" + near.of(ParticleHandler.CLOUD.get()) + " jar clouds), " + near.sounds.size() + " sounds, 1 camera shake; the player 150 blocks off nothing");
                })
                .thenSucceed();
    }

    /**
     * An Umvuthi SPAWN with a player 6 blocks from the throne and one 150 blocks off: DONE at once with nothing held, the
     * Umvuthi on its throne facing the direction the line names, and the arrival played once: the near hearer is sent the jar's
     * orbs (24), vanilla's flames, smoke and ash, the jar's belly drum and roar, and one camera shake stands there; all survive
     * the wire; the far hearer is sent nothing. A warrior SPAWN after it, with the same player beside the grove, is placed with
     * no effect: no arrival is counted for it and the hearer is sent no particle and no sound.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_arrival_umvuthi", skyAccess = true)
    public static void aDueUmvuthiIsSeatedAtOnceBesideAPlayerWithItsArrival(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 3000);
        String test = "aDueUmvuthiIsSeatedAtOnceBesideAPlayerWithItsArrival";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        FakePlayers.Hearer[] hearer = new FakePlayers.Hearer[2];
        int[] mark = new int[6];
        long[] was = new long[4], op = new long[2], sent = new long[2];
        Vec3[] throne = new Vec3[1];
        Entity[] chief = new Entity[1];

        MowzieKit.seq(helper)
                .thenExecute(g::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    g.lay().declare();
                    throne[0] = g.at(6, 0);
                    hearer[0] = FakePlayers.overhearing(level, throne[0].add(6, 1, 0), "mowzie-arrival-near");
                    hearer[1] = FakePlayers.overhearing(level, throne[0].add(0, 1, 150), "mowzie-arrival-far");
                    ConflictKit.onEnd(helper, () -> {
                        FakePlayers.remove(hearer[0]);
                        FakePlayers.remove(hearer[1]);
                    });
                    MowzieKit.everyTick(helper, () -> {
                        if (sent[1] == 0 && !MowzieKit.held(g.faction, CHIEF_SLOT, 1).isEmpty())
                            sent[1] = level.getGameTime();
                    });
                })
                .thenWaitUntil(waits.logic(() -> check("census 1", MowzieKit.censusCount(g.faction) >= 1)))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    was[0] = camps.heldSpawns;
                    was[1] = BossArrival.UMVUTHIS.get();
                    was[2] = camps.arrivedBodies;
                    mark[0] = hearer[0].packets.size();
                    mark[1] = hearer[0].sounds.size();
                    mark[2] = hearer[1].packets.size();
                    mark[3] = hearer[1].sounds.size();
                    sent[0] = level.getGameTime();
                    op[0] = MowzieKit.reconcile(g.faction, g.site().epoch(), throne[0], 3f, List.of(CampLine.of(CHIEF_SLOT, 1, MowzieKit.UMVUTHI, SPAWN, CHIEF)));
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g.faction, op[0], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the chief SPAWN with a player 6 blocks from the throne", ConflictKit.outcomes(g.faction, op[0]), DONE);
                    long after = sent[1] - sent[0];
                    check("placed at once, not held (" + after + " ticks after it was sent)", sent[1] != 0 && after <= 200);
                    check("nothing was held (" + (MowzieKit.camps().heldSpawns - was[0]) + ")",
                            MowzieKit.camps().heldSpawns == was[0] && MowzieKit.camps().heldLines() == 0);
                    List<Entity> h = MowzieKit.held(g.faction, CHIEF_SLOT, 1);
                    check("an Umvuthi, tagged (faction, -1, 1) (" + h.size() + ")", h.size() == 1 && h.get(0) instanceof EntityUmvuthi);
                    EntityUmvuthi u = (EntityUmvuthi) h.get(0);
                    chief[0] = u;
                    check("at the throne facing 3 (" + u.position().distanceTo(throne[0]) + " blocks off, " + u.getDirectionData() + ")",
                            u.position().distanceTo(throne[0]) < 5 && u.getDirectionData() == 3);
                    check("the arrival was played once (" + (BossArrival.UMVUTHIS.get() - was[1]) + ")", BossArrival.UMVUTHIS.get() == was[1] + 1);

                    Sent near = Sent.since(hearer[0], mark[0], mark[1]);
                    check("the jar's orbs: 24 (" + near.of(ParticleHandler.ORB.get()) + ")", near.of(ParticleHandler.ORB.get()) == 24);
                    check("vanilla's flames, smoke and ash (" + near.of(ParticleTypes.FLAME) + ", " + near.of(ParticleTypes.CAMPFIRE_SIGNAL_SMOKE) + ", "
                            + near.of(ParticleTypes.ASH) + ")", near.of(ParticleTypes.FLAME) == 20 && near.of(ParticleTypes.CAMPFIRE_SIGNAL_SMOKE) == 5
                            && near.of(ParticleTypes.ASH) == 1);
                    check("one burst, not a stream (" + near.particles.size() + " particle packets)", near.particles.size() >= 30 && near.particles.size() <= SHORT);
                    check("the jar's belly drum and roar (" + near.sounds.size() + " sounds)",
                            near.heard(MMSounds.ENTITY_UMVUTHI_BELLY) && near.heard(MMSounds.ENTITY_UMVUTHI_ROAR) && near.sounds.size() == 2);
                    check("one camera shake stands at the throne (" + shakes(level, throne[0]) + ")", shakes(level, throne[0]) == 1);
                    survivesTheWire(level, near);
                    Sent far150 = Sent.since(hearer[1], mark[2], mark[3]);
                    check("the player 150 blocks off was sent nothing (" + far150.particles.size() + " particles, " + far150.sounds.size() + " sounds)",
                            far150.particles.isEmpty() && far150.sounds.isEmpty());

                    // a warrior comes back as it always did: quietly, with the same player beside the grove
                    was[3] = MowzieKit.camps().arrivedBodies;
                    mark[4] = hearer[0].packets.size();
                    mark[5] = hearer[0].sounds.size();
                    op[1] = MowzieKit.reconcile(g.faction, g.site().epoch(), List.of(CampLine.of(0, 1, MowzieKit.WARRIOR, SPAWN, 0)));
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g.faction, op[1], 1)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the warrior's SPAWN", ConflictKit.outcomes(g.faction, op[1]), DONE);
                    List<Entity> w = MowzieKit.held(g.faction, 0, 1);
                    check("a warrior stands (" + w.size() + ")", w.size() == 1 && w.get(0) instanceof EntityUmvuthanaMinion);
                    check("no arrival was played for it (" + (BossArrival.UMVUTHIS.get() - was[1]) + ")", BossArrival.UMVUTHIS.get() == was[1] + 1);
                    Sent quiet = Sent.since(hearer[0], mark[4], mark[5]);
                    check("the player beside the grove was sent no particle and no sound (" + quiet.particles.size() + ", " + quiet.sounds.size() + ")",
                            quiet.particles.isEmpty() && quiet.sounds.isEmpty());
                    check("the camp counted the body's arrival hook (" + (MowzieKit.camps().arrivedBodies - was[3]) + ")", MowzieKit.camps().arrivedBodies == was[3] + 1);
                    record(test, "arrival", "the chief SPAWN was DONE " + (sent[1] - sent[0]) + " ticks after it was sent with a player 6 blocks off; "
                            + "the warrior's SPAWN made no sound and no particle");
                })
                .thenSucceed();
    }

    /**
     * An Umvuthi heir with a player 6 blocks from the throne: an adopted warrior, then the pair (MASK, SEAT_HEIR). The pair is
     * played at once (no held pair, no held line), the warrior is taken, the heir stands on the throne tagged (faction, -1, 2),
     * and its arrival is played as the first Umvuthi's is: the near hearer is sent the jar's orbs and the belly drum.
     */
    @GameTest(template = ARENA, timeoutTicks = ChunkWait.NO_TICK_TIMEOUT, batch = "mowzie_arrival_heir", skyAccess = true)
    public static void anUmvuthiHeirIsSeatedAtOnceBesideAPlayerWithItsArrival(GameTestHelper helper) {
        ChunkWait waits = ChunkWait.of(helper, 3000);
        String test = "anUmvuthiHeirIsSeatedAtOnceBesideAPlayerWithItsArrival";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        FakePlayers.Hearer[] hearer = new FakePlayers.Hearer[1];
        EntityUmvuthanaMinion[] w = new EntityUmvuthanaMinion[1];
        int[] mark = new int[2];
        long[] was = new long[4], op = new long[2];
        Vec3[] throne = new Vec3[1];

        MowzieKit.seq(helper)
                .thenExecute(g::force)
                .thenWaitUntil(waits.chunks(far.ready()))
                .thenExecute(() -> {
                    g.lay().declare();
                    throne[0] = g.at(0, 10);
                    w[0] = MowzieKit.warrior(level, g.at(-10, 0));
                    hearer[0] = FakePlayers.overhearing(level, throne[0].add(6, 1, 0), "mowzie-arrival-heir");
                    ConflictKit.onEnd(helper, () -> FakePlayers.remove(hearer[0]));
                })
                .thenWaitUntil(waits.logic(() -> check("census 1", MowzieKit.censusCount(g.faction) >= 1)))
                .thenExecute(() -> op[0] = MowzieKit.reconcile(g.faction, g.site().epoch(),
                        List.of(CampLine.of(0, 1, MowzieKit.WARRIOR, ADOPT, 0, w[0].getUUID()))))
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g.faction, op[0], 1)))
                .thenExecute(() -> {
                    Camps camps = MowzieKit.camps();
                    was[0] = camps.heldPairs;
                    was[1] = BossArrival.UMVUTHIS.get();
                    was[2] = camps.heirs;
                    mark[0] = hearer[0].packets.size();
                    mark[1] = hearer[0].sounds.size();
                    op[1] = MowzieKit.reconcile(g.faction, g.site().epoch(), throne[0], 3f, List.of(
                            CampLine.of(0, 0, MowzieKit.WARRIOR, MASK, 0), CampLine.of(CHIEF_SLOT, 2, MowzieKit.UMVUTHI, SEAT_HEIR, CHIEF)));
                })
                .thenWaitUntil(waits.logic(() -> MowzieKit.answered(g.faction, op[1], 2)))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("the pair with a player 6 blocks from the throne", ConflictKit.outcomes(g.faction, op[1]), DONE);
                    Camps camps = MowzieKit.camps();
                    check("the pair was not held (" + (camps.heldPairs - was[0]) + ")", camps.heldPairs == was[0] && camps.heldLines() == 0);
                    check("an heir was seated (" + (camps.heirs - was[2]) + ") and the warrior is gone", camps.heirs == was[2] + 1 && w[0].isRemoved());
                    List<Entity> heir = MowzieKit.held(g.faction, CHIEF_SLOT, 2);
                    check("the heir stands, tagged (faction, -1, 2) (" + heir.size() + ")", heir.size() == 1 && heir.get(0) instanceof EntityUmvuthi
                            && heir.get(0).position().distanceTo(throne[0]) < 5);
                    check("the arrival was played once (" + (BossArrival.UMVUTHIS.get() - was[1]) + ")", BossArrival.UMVUTHIS.get() == was[1] + 1);
                    Sent near = Sent.since(hearer[0], mark[0], mark[1]);
                    check("the jar's orbs: 24 (" + near.of(ParticleHandler.ORB.get()) + ")", near.of(ParticleHandler.ORB.get()) == 24);
                    check("the jar's belly drum and roar (" + near.sounds.size() + " sounds)", near.heard(MMSounds.ENTITY_UMVUTHI_BELLY) && near.heard(MMSounds.ENTITY_UMVUTHI_ROAR));
                    survivesTheWire(level, near);
                    record(test, "heir", "the pair was DONE at once with a player 6 blocks from the throne; " + near.particles.size() + " particle packets, "
                            + near.sounds.size() + " sounds");
                })
                .thenSucceed();
    }
}
