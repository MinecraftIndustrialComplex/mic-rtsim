package com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest;

import com.bobmowzie.mowziesmobs.server.entity.EntityHandler;
import com.bobmowzie.mowziesmobs.server.entity.frostmaw.EntityFrostmaw;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthana;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaMinion;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthanaRaptor;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.EntityUmvuthi;
import com.bobmowzie.mowziesmobs.server.entity.umvuthana.MaskType;
import com.bobmowzie.mowziesmobs.server.world.feature.structure.FrostmawPieces;
import com.bobmowzie.mowziesmobs.server.world.feature.structure.UmvuthanaGrovePieces;
import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CampLine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit.CensusBody;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PendingRecords;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.DenKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.GroveKind;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MicRtsimMowziesMobs;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.MowzieCamps;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Den;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Far;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Grove;
import com.minecraftindustrialcomplex.mic_rtsim_mowziesmobs.gametest.MowzieKit.Markers;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Marker;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.levelgen.structure.pieces.PiecesContainer;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.io;
import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.record;

/**
 * The camp reconciler on the jar's own bodies (MOWZIE.md §4.9, §8; docs/choices-mowzie-build.md, M5): a
 * Frostmaw's den and an Umvuthana grove, found, counted, kept home and played with the engine's
 * CAMP_RECONCILE lines, with Mowzie's Mobs itself on the GameTest server. Every test works in a far place
 * of its own (the world is reused) and declares its camps under a name no culture claims (MZB-227), so
 * the engine plays only the lines the test scripts. One batch each: the reconciler's counters are global.
 */
@GameTestHolder(MicRtsimMowziesMobs.MODID)
@PrefixGameTestTemplate(false)
public final class MowzieGameTests {
    private MowzieGameTests() {}

    private static final String ARENA = RtsimTests.TEMPLATE;
    private static final ResourceLocation FROSTMAW_TEMPLATE = ResourceLocation.fromNamespaceAndPath("mowziesmobs", "frostmaw_spawn");

    private static final int DONE = ConflictRecords.CampBodies.DONE, NOTHING = ConflictRecords.CampBodies.NOTHING,
            STALE = ConflictRecords.CampBodies.STALE;
    private static final int SPAWN = ConflictRecords.CampReconcile.SPAWN, ADOPT = ConflictRecords.CampReconcile.ADOPT,
            TAKE = ConflictRecords.CampReconcile.TAKE, SET_OUT = ConflictRecords.CampReconcile.SET_OUT,
            HAND_BACK = ConflictRecords.CampReconcile.HAND_BACK, REMOVE = ConflictRecords.CampReconcile.REMOVE;
    private static final int CHIEF = ConflictRecords.CampReconcile.LINE_CHIEF, CRYSTAL = ConflictRecords.CampReconcile.LINE_CRYSTAL;

    // ---- 1: a den ---------------------------------------------------------------------------

    /**
     * Four real Frostmaw den pieces, one per rotation, each placed as worldgen places it at the middle of
     * its own chunk (§7 test 1). The template's Frostmaw stands within 1.5 blocks of the den's spot
     * (MZB-217) and stays the jar's own: no puppet, untagged. The den is declared from a real structure
     * start; its FACTION_FOUND comes once with one body at home and the census flags the Frostmaw has (its
     * crystal as set), and its CAMP_CENSUS lists it as a stray chief with a health between what was set and
     * what it has now (it heals asleep). ADOPT makes it the camp's body (slot 0, generation 1) where it
     * stands: restricted 8 round the spot, still no puppet, still saved, its persistence as the jar left it
     * (MZB-210), and its saved NBT carries the camp tag, so a world without rtsim loads it as the jar's own.
     */
    @GameTest(template = ARENA, timeoutTicks = 1500, batch = "mowzie_den", skyAccess = true)
    public static void aDensFrostmawStaysVanillaAndTheCensusReadsItsCrystalAndHealth(GameTestHelper helper) {
        String test = "aDensFrostmawStaysVanillaAndTheCensusReadsItsCrystalAndHealth";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        int n = 4, cz = far.z0;
        int[] cx = new int[n];
        for (int i = 0; i < n; i++)
            cx[i] = far.k + 4 * i;
        Rotation[] rotation = Rotation.values();
        boolean[] crystal = {true, false, true, false};
        float[] set = new float[n];
        EntityFrostmaw[] frost = new EntityFrostmaw[n];
        boolean[] persistent = new boolean[n];
        long[] faction = new long[n], op = new long[n];

        helper.startSequence()
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++)
                        far.forceRect(cx[i] - 1, cz - 1, cx[i] + 1, cz + 1);
                })
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    Structure structure = level.registryAccess().registryOrThrow(Registries.STRUCTURE)
                            .get(ResourceLocation.parse(MicRtsimMowziesMobs.DEN_STRUCTURE));
                    for (int i = 0; i < n; i++) {
                        int px = cx[i] * 16 + 8, pz = cz * 16 + 8;
                        MowzieKit.platform(level, (cx[i] - 1) * 16, (cx[i] + 1) * 16 + 15, (cz - 1) * 16, (cz + 1) * 16 + 15);
                        int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, px, pz);
                        FrostmawPieces.FrostmawPiece piece = new FrostmawPieces.FrostmawPiece(level.getStructureManager(), FROSTMAW_TEMPLATE,
                                new BlockPos(px, y, pz), rotation[i]);
                        ChunkPos chunk = new ChunkPos(cx[i], cz);
                        BlockPos pivot = new BlockPos(chunk.getMiddleBlockX(), piece.getBoundingBox().minY(), chunk.getMiddleBlockZ());
                        piece.postProcess(level, level.structureManager(), level.getChunkSource().getGenerator(), level.random,
                                piece.getBoundingBox(), chunk, pivot);
                        BoundingBox box = piece.getBoundingBox();
                        List<EntityFrostmaw> fs = level.getEntitiesOfClass(EntityFrostmaw.class, AABB.of(box).inflate(3));
                        check("den " + i + " (" + rotation[i] + ") made one Frostmaw from its template (" + fs.size() + ")", fs.size() == 1);
                        EntityFrostmaw f = fs.get(0);
                        Vec3 spot = MowzieCamps.denSpot(box);
                        double off = Math.hypot(f.getX() - spot.x, f.getZ() - spot.z);
                        record(test, "den " + i + " " + rotation[i], "box " + box + ", the Frostmaw " + String.format("%.2f", off) + " blocks from the spot");
                        check("den " + i + "'s Frostmaw stands within 1.5 blocks of the spot (" + off + ")", off <= 1.5);
                        check("and is the jar's own: no puppet, untagged", !RtsimApi.isPuppet(f) && !RtsimAttachments.isTagged(f) && MowzieKit.tag(f) == null);
                        f.setHasCrystal(crystal[i]);
                        f.setHealth(Math.min(60f + 30f * i, f.getMaxHealth() - 5f));
                        set[i] = f.getHealth();
                        persistent[i] = f.isPersistenceRequired();
                        frost[i] = f;
                        StructureStart start = new StructureStart(structure, chunk, 0, new PiecesContainer(List.of(piece)));
                        faction[i] = MowzieCamps.declareDen(level, start, false, MowzieKit.TEST_DEN);
                        check("den " + i + " is declared", faction[i] != 0);
                    }
                })
                .thenWaitUntil(() -> {
                    for (int i = 0; i < n; i++)
                        check("den " + i + "'s census", MowzieKit.censusCount(faction[i]) >= 1);
                })
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++) {
                        List<ScriptedEngine.Received> found = ConflictKit.factionsFound(faction[i]);
                        check("den " + i + ": FACTION_FOUND once (" + found.size() + ")", found.size() == 1);
                        ByteBuffer fb = found.get(0).view();
                        check("home bodies 1 (" + (fb.getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) + ")",
                                (fb.getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) == 1);
                        int want = ConflictRecords.FactionFound.CENSUS_CHIEF | (crystal[i] ? ConflictRecords.FactionFound.CENSUS_CRYSTAL : 0);
                        check("den " + i + ": found with the chief" + (crystal[i] ? " and its crystal" : " and no crystal"),
                                (fb.get(ConflictRecords.FactionFound.CENSUS) & 0xFF) == want);
                        ScriptedEngine.Received c = MowzieKit.census(faction[i], 1);
                        int wantFlags = ConflictRecords.CampCensus.CHIEF | (crystal[i] ? ConflictRecords.CampCensus.CRYSTAL : 0);
                        check("den " + i + ": the census's chief flags", MowzieKit.chiefFlags(c) == wantFlags);
                        List<CensusBody> bodies = ConflictKit.censusBodies(c);
                        check("den " + i + ": one body (" + bodies.size() + ")", bodies.size() == 1);
                        CensusBody b = bodies.get(0);
                        check("it is the Frostmaw", b.body() == ConflictKit.bodyId(MowzieKit.FROSTMAW) && b.uuid().equals(frost[i].getUUID()));
                        check("a stray chief (" + b.flags() + ")",
                                b.flags() == (ConflictRecords.CampCensus.BODY_STRAY | ConflictRecords.CampCensus.BODY_CHIEF));
                        float h = MowzieKit.chiefHealth(c);
                        record(test, "den " + i + " health", "set " + set[i] + ", census " + h + ", now " + frost[i].getHealth());
                        check("its health reads between what was set and what it has now (" + h + ")",
                                h >= set[i] - 0.01f && h <= frost[i].getHealth() + 0.01f);
                        check("it is still no puppet and untagged", !RtsimApi.isPuppet(frost[i]) && MowzieKit.tag(frost[i]) == null);
                        op[i] = MowzieKit.reconcile(faction[i], MowzieKit.epoch(c), List.of(CampLine.of(0, 1, MowzieKit.FROSTMAW, ADOPT, CHIEF,
                                frost[i].getUUID())));
                    }
                })
                .thenWaitUntil(() -> {
                    for (int i = 0; i < n; i++)
                        MowzieKit.answered(faction[i], op[i], 1);
                })
                .thenExecute(() -> {
                    for (int i = 0; i < n; i++) {
                        MowzieKit.allOutcomes("den " + i + " ADOPT", ConflictKit.outcomes(faction[i], op[i]), DONE);
                        EntityFrostmaw f = frost[i];
                        Camps.Site site = MowzieKit.site(faction[i]);
                        check("den " + i + ": tagged (faction, 0, 1)", MowzieKit.tagged(f, faction[i], 0, 1));
                        check("a body a camp holds", RtsimApi.isCampBody(f));
                        check("restricted " + DenKind.RESTRICT + " round the spot", f.hasRestriction() && f.getRestrictRadius() == DenKind.RESTRICT
                                && f.getRestrictCenter().equals(BlockPos.containing(site.spot)));
                        check("still no puppet", !RtsimApi.isPuppet(f) && !RtsimAttachments.isTagged(f));
                        check("still saved with its chunk", f.shouldBeSaved());
                        check("its persistence is the jar's (" + persistent[i] + ")", f.isPersistenceRequired() == persistent[i]);
                        CompoundTag nbt = new CompoundTag();
                        f.saveWithoutId(nbt);
                        check("its NBT carries the camp tag", nbt.getCompound("neoforge:attachments").contains("mic_rtsim:camp_member"));
                        check("and the tag names this camp, slot 0, generation 1", nbt.getCompound("neoforge:attachments").getCompound("mic_rtsim:camp_member")
                                .getLong("faction") == faction[i]);
                    }
                    record(test, "adopted", "four Frostmaws, one per rotation, tagged and restricted; the tag is in their saved NBT");
                })
                .thenSucceed();
    }

    // ---- 2: a grove -------------------------------------------------------------------------

    /**
     * A real Umvuthana fire pit piece (rotation at random) at the middle of a chunk (§7 test 3): every
     * warrior it makes is restricted 25 round {@code MowzieCamps.groveCentre} (MZB-218). With a warrior
     * added inside, a raptor inside, a no-AI warrior 27 blocks out and a no-AI Umvuthi (direction 2), the
     * census lists exactly the warriors inside, all strays: no raptor, no warrior outside, no Umvuthi body;
     * its chief fields are the Umvuthi's (CHIEF and THRONE, its position, direction 2), and FACTION_FOUND
     * says how many are at home and that a chief is. ADOPT of all of them in one reconcile is DONE for each:
     * tagged by slot, restricted again. The footprint released and forced again is CAMP_OUT then a second
     * census listing every adopted body by UUID with its slot (not as a stray); FACTION_FOUND stays once.
     * The session reset that recounts a grove is MZB-228's gap.
     */
    @GameTest(template = ARENA, timeoutTicks = 2000, batch = "mowzie_grove", skyAccess = true)
    public static void aGroveIsCountedAndAResetRecountsTheWarriorsStandingInIt(GameTestHelper helper) {
        String test = "aGroveIsCountedAndAResetRecountsTheWarriorsStandingInIt";
        Far far = new Far(helper, test);
        ServerLevel level = far.level;
        Grove g = new Grove(far, far.k, far.z0);
        Rotation rotation = Rotation.getRandom(level.random);
        Set<UUID> inside = new HashSet<>();
        UUID[] adopted = new UUID[0];
        List<UUID> order = new ArrayList<>();
        Entity[] extra = new Entity[4]; // warrior inside, raptor, warrior out, Umvuthi
        BlockPos[] centre = new BlockPos[1];
        long[] op = new long[2];
        long[] e1 = new long[1];
        float[] chiefHealth = new float[1];

        helper.startSequence()
                .thenExecute(g::force)
                .thenWaitUntil(far.ready())
                .thenExecute(() -> {
                    g.lay();
                    Structure structure = level.registryAccess().registryOrThrow(Registries.STRUCTURE)
                            .get(ResourceLocation.parse(MicRtsimMowziesMobs.GROVE_STRUCTURE));
                    int px = g.cx * 16 + 8, pz = g.cz * 16 + 8;
                    int y = level.getHeight(Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, px, pz);
                    // The piece's centre is (minX + 4, maxZ - 4) of its box: shift it onto the chunk's middle.
                    UmvuthanaGrovePieces.FirepitPiece probe = new UmvuthanaGrovePieces.FirepitPiece(level.getStructureManager(), rotation,
                            new BlockPos(px, y, pz));
                    BoundingBox pb = probe.getBoundingBox();
                    int dx = px - (pb.minX() + 4), dz = pz - (pb.maxZ() - 4);
                    UmvuthanaGrovePieces.FirepitPiece piece = new UmvuthanaGrovePieces.FirepitPiece(level.getStructureManager(), rotation,
                            new BlockPos(px + dx, y, pz + dz));
                    BoundingBox box = piece.getBoundingBox();
                    check("the fire pit's centre is the chunk's middle", box.minX() + 4 == px && box.maxZ() - 4 == pz);
                    ChunkPos chunk = new ChunkPos(g.cx, g.cz);
                    BlockPos pivot = new BlockPos(chunk.getMiddleBlockX(), box.minY(), chunk.getMiddleBlockZ());
                    piece.postProcess(level, level.structureManager(), level.getChunkSource().getGenerator(), level.random, box, chunk, pivot);
                    centre[0] = MowzieCamps.groveCentre(box);
                    List<EntityUmvuthanaMinion> made = level.getEntitiesOfClass(EntityUmvuthanaMinion.class, g.area());
                    record(test, "fire pit", rotation + ", box " + piece.getBoundingBox() + ", centre " + centre[0] + ", " + made.size() + " warriors made");
                    check("the fire pit made warriors (" + made.size() + ")", made.size() >= 3 && made.size() <= 9);
                    for (EntityUmvuthanaMinion m : made) {
                        // the jar restricts to the surface block there; the centre is from the piece's box (MZB-242)
                        BlockPos rc = m.getRestrictCenter();
                        check("a warrior it made is restricted 25 round the grove's centre " + centre[0] + " (" + rc + ", "
                                + m.getRestrictRadius() + ")", m.hasRestriction() && m.getRestrictRadius() == GroveKind.RESTRICT
                                && rc.getX() == centre[0].getX() && rc.getZ() == centre[0].getZ() && Math.abs(rc.getY() - centre[0].getY()) <= 4);
                        m.setNoAi(true);
                    }
                    // the grove's spot is not declared yet: g.spot is null, so place from the centre block
                    extra[0] = MowzieKit.stray(level, EntityHandler.UMVUTHANA_MINION.get(), centre[0].getX() - 7.5, y, centre[0].getZ() + 6.5, true);
                    extra[1] = MowzieKit.stray(level, EntityHandler.UMVUTHANA_RAPTOR.get(), centre[0].getX() + 8.5, y, centre[0].getZ() - 5.5, true, false);
                    extra[2] = MowzieKit.stray(level, EntityHandler.UMVUTHANA_MINION.get(), centre[0].getX() + 27.5, y, centre[0].getZ() + 0.5, true);
                    EntityUmvuthi u = MowzieKit.stray(level, EntityHandler.UMVUTHI.get(), centre[0].getX() + 12.5, y, centre[0].getZ() + 0.5, true, false);
                    u.setDirection(2);
                    extra[3] = u;
                    chiefHealth[0] = u.getHealth();
                    StructureStart start = new StructureStart(structure, chunk, 0, new PiecesContainer(List.of(piece)));
                    g.faction = MowzieCamps.declareGrove(level, start, false, MowzieKit.TEST_GROVE);
                    check("the grove is declared from its real structure start", g.faction != 0);
                    g.spot = g.site().spot;
                    check("its spot is the centre block's middle " + g.spot, g.spot.x == centre[0].getX() + 0.5 && g.spot.z == centre[0].getZ() + 0.5);
                    check("the 27-block warrior is outside the footprint", !g.site().contains(extra[2].position()));
                })
                .thenWaitUntil(() -> check("census 1", MowzieKit.censusCount(g.faction) >= 1))
                .thenExecute(() -> {
                    Camps.Site site = g.site();
                    List<EntityUmvuthanaMinion> all = level.getEntitiesOfClass(EntityUmvuthanaMinion.class, g.area());
                    Set<UUID> want = new HashSet<>();
                    for (EntityUmvuthanaMinion m : all)
                        if (m.isAlive() && site.contains(m.position()))
                            want.add(m.getUUID());
                    check("the warrior added inside is counted among them", want.contains(extra[0].getUUID()));
                    ScriptedEngine.Received c = MowzieKit.census(g.faction, 1);
                    List<CensusBody> bodies = ConflictKit.censusBodies(c);
                    Set<UUID> got = new HashSet<>();
                    for (CensusBody b : bodies) {
                        got.add(b.uuid());
                        check("a warrior of the grove, a stray (" + b.flags() + ")", b.body() == ConflictKit.bodyId(MowzieKit.WARRIOR)
                                && b.flags() == ConflictRecords.CampCensus.BODY_STRAY);
                    }
                    record(test, "census 1", bodies.size() + " warriors of " + all.size() + " standing near; chief health " + MowzieKit.chiefHealth(c));
                    check("exactly the warriors inside: " + got.size() + " listed, " + want.size() + " inside", got.equals(want) && bodies.size() == want.size());
                    check("no raptor", !got.contains(extra[1].getUUID()));
                    check("no warrior 27 blocks out", !got.contains(extra[2].getUUID()));
                    check("no Umvuthi body", !got.contains(extra[3].getUUID()));
                    check("chief flags CHIEF and THRONE (" + MowzieKit.chiefFlags(c) + ")",
                            MowzieKit.chiefFlags(c) == (ConflictRecords.CampCensus.CHIEF | ConflictRecords.CampCensus.THRONE));
                    ByteBuffer cb = c.view();
                    check("the throne is the Umvuthi's position", cb.getDouble(ConflictRecords.CampCensus.THRONE_X) == extra[3].getX()
                            && cb.getDouble(ConflictRecords.CampCensus.THRONE_Y) == extra[3].getY()
                            && cb.getDouble(ConflictRecords.CampCensus.THRONE_Z) == extra[3].getZ());
                    check("its direction 2", cb.getFloat(ConflictRecords.CampCensus.DIRECTION) == 2f);
                    check("its health", Math.abs(MowzieKit.chiefHealth(c) - chiefHealth[0]) < 0.01f);
                    List<ScriptedEngine.Received> found = ConflictKit.factionsFound(g.faction);
                    check("FACTION_FOUND once (" + found.size() + ")", found.size() == 1);
                    ByteBuffer fb = found.get(0).view();
                    check("home " + want.size() + " (" + (fb.getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) + ")",
                            (fb.getShort(ConflictRecords.FactionFound.HOME_BODIES) & 0xFFFF) == want.size());
                    check("found with the chief, no crystal", (fb.get(ConflictRecords.FactionFound.CENSUS) & 0xFF) == ConflictRecords.FactionFound.CENSUS_CHIEF);
                    e1[0] = MowzieKit.epoch(c);
                    List<CampLine> lines = new ArrayList<>();
                    for (CensusBody b : bodies) {
                        lines.add(CampLine.of(order.size(), 1, MowzieKit.WARRIOR, ADOPT, 0, b.uuid()));
                        order.add(b.uuid());
                    }
                    op[0] = MowzieKit.reconcile(g.faction, e1[0], lines);
                })
                .thenWaitUntil(() -> MowzieKit.answered(g.faction, op[0], order.size()))
                .thenExecute(() -> {
                    MowzieKit.allOutcomes("ADOPT", ConflictKit.outcomes(g.faction, op[0]), DONE);
                    BlockPos spotBlock = BlockPos.containing(g.spot);
                    for (int i = 0; i < order.size(); i++) {
                        Entity e = level.getEntity(order.get(i));
                        check("warrior " + i + " is in the level", e instanceof EntityUmvuthanaMinion);
                        EntityUmvuthanaMinion m = (EntityUmvuthanaMinion) e;
                        check("tagged (faction, " + i + ", 1)", MowzieKit.tagged(m, g.faction, i, 1));
                        check("restricted 25 round the spot", m.hasRestriction() && m.getRestrictRadius() == GroveKind.RESTRICT
                                && m.getRestrictCenter().equals(spotBlock));
                        check("no puppet", !RtsimApi.isPuppet(m));
                    }
                    // the footprint out and in
                    far.releaseRect(g.cx - 2, g.cz - 2, g.cx + 2, g.cz + 2);
                })
                .thenWaitUntil(() -> check("CAMP_OUT", ConflictKit.campOuts(g.faction).size() == 1))
                .thenWaitUntil(io(() -> {
                    for (UUID id : order)
                        check("warrior " + id + " is unloaded", level.getEntity(id) == null);
                }))
                .thenExecute(() -> {
                    check("one census so far", MowzieKit.censusCount(g.faction) == 1);
                    g.force();
                })
                .thenWaitUntil(far.ready())
                .thenWaitUntil(() -> check("census 2", MowzieKit.censusCount(g.faction) >= 2))
                .thenExecute(() -> {
                    ScriptedEngine.Received c = MowzieKit.census(g.faction, 2);
                    check("a new epoch", MowzieKit.epoch(c) != e1[0]);
                    List<CensusBody> bodies = ConflictKit.censusBodies(c);
                    check("every adopted warrior is listed and no other (" + bodies.size() + " of " + order.size() + ")", bodies.size() == order.size());
                    for (int i = 0; i < order.size(); i++) {
                        CensusBody b = MowzieKit.bodyOf(c, order.get(i));
                        check("warrior " + i + " is listed", b != null);
                        check("by its slot " + i + ", generation 1 (" + b.slot() + ", " + b.generation() + ")", b.slot() == i && b.generation() == 1);
                        check("not as a stray or a chief (" + b.flags() + ")",
                                (b.flags() & (ConflictRecords.CampCensus.BODY_STRAY | ConflictRecords.CampCensus.BODY_CHIEF)) == 0);
                    }
                    check("FACTION_FOUND is still once", ConflictKit.factionsFound(g.faction).size() == 1);
                    record(test, "out and in", "census 2 lists the " + order.size() + " adopted warriors by slot; the session reset itself has no GameTest (MZB-228)");
                })
                .thenSucceed();
    }

    // @@NEXT@@
}
