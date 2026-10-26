package com.minecraftindustrialcomplex.mic_rtsim_carryon.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampKind;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.CampMember;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Camps;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.RtsimAttachments;
import com.minecraftindustrialcomplex.mic_rtsim_carryon.MicRtsimCarryOn;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Marker;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import tschipp.carryon.events.EntityPickupEvent;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;

/**
 * Carry On and the conflict bridge (MOWZIE.md §8, ledger 16), with Carry On itself on the GameTest server.
 */
@GameTestHolder(MicRtsimCarryOn.MODID)
@PrefixGameTestTemplate(false)
public final class CarryOnConflictGameTests {
    private CarryOnConflictGameTests() {}

    /**
     * A player's Carry On pickup of a raider is a casualty at that player's hands (BAND_CASUALTIES kind 1
     * with the player); the war chief picked up is CHIEF_KILLED kind 1 with the player and a Java op id.
     * Each body stays in the world as an ordinary mob ({@code Entity.save} works, no name, and no longer
     * persistence-required, so it despawns as any other would: MZB-102, UR-1). A member
     * discarded by something else is kind 3 with no killer. None of it is a killing signal, though the
     * band stands inside a territory that minds killing, of a camp whose kin is every entity. The recall
     * reports the one left.
     */
    @GameTest(template = "arena", timeoutTicks = 600, batch = "carryon_conflict", skyAccess = true)
    public static void aCarryOnPickupOfARaiderIsTheCarriersKill(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        String dim = RtsimTests.dimension(helper);
        Vec3 c = RtsimTests.centre(helper);
        long agent = ConflictKit.newAgent();
        ConflictKit.Band band = ConflictKit.band(agent, ConflictKit.newFaction(), dim);
        band.arcX = c.x + 1.5;
        band.arcZ = c.z;
        band.targetX = c.x + 100;
        band.targetZ = c.z;
        band.flags = ConflictRecords.Materialize.FLAG_HOSTILE | ConflictRecords.Materialize.FLAG_CHIEF;
        ServerPlayer player = FakePlayers.add(level, c.add(-3, 0, 0), "carryon-raider");
        player.setGameMode(GameType.CREATIVE);
        long[] camp = {0};
        long[] counted = {0};
        List<Mob> taken = new ArrayList<>();
        Mob[] chief = new Mob[1];

        helper.startSequence()
                .thenExecute(() -> {
                    camp[0] = ConflictKit.bridge().reportCamp(level, "mic_rtsim:conflict/test_unclaimed_camp", BlockPos.containing(c),
                            e -> true, null, false, 0, 0);
                    int mask = (1 << ConflictRecords.SIGNAL_KILLING) | (1 << ConflictRecords.SIGNAL_BUILDING);
                    ConflictKit.territory(camp[0], dim, mask, (int) Math.floor(c.x), (int) Math.floor(c.z), 64);
                })
                .thenWaitUntil(() -> check("the territory", ConflictKit.bridge().territories().get(camp[0]) != null))
                .thenExecute(() -> ConflictKit.materialize(band, List.of(new ConflictKit.StackSpec(ConflictKit.WARRIOR, 3, true))))
                // The column (MZB-722): the warriors stand as a row of two and a row of one, the chief alone at the rear.
                .thenWaitUntil(() -> check("placed: three warriors and the chief (" + ConflictKit.bridge().bodies(agent).size() + ")",
                        ConflictKit.party(agent).stacks().size() == 3 && ConflictKit.bridge().bodies(agent).size() == 4))
                .thenExecute(() -> {
                    counted[0] = ConflictKit.bridge().signalsCounted;
                    ConflictBridge.Stack st = ConflictKit.party(agent).stacks().get(0);
                    Mob warrior = st.slots().get(0).entity();
                    for (ConflictBridge.Stack rows : ConflictKit.party(agent).stacks())
                        for (ConflictBridge.Slot sl : rows.slots())
                            if (sl.chief)
                                chief[0] = sl.entity();
                    for (Mob m : List.of(warrior, chief[0])) {
                        check("a puppet: Entity.save refuses it", RtsimApi.isPuppet(m) && !m.save(new CompoundTag()));
                        check("held persistence-required", m.isPersistenceRequired());
                        EntityPickupEvent event = NeoForge.EVENT_BUS.post(new EntityPickupEvent(player, m));
                        check("nobody cancelled the pickup", !event.isCanceled());
                        check("no longer a puppet", !RtsimApi.isPuppet(m));
                        CompoundTag saved = new CompoundTag();
                        check("Carry On's save of it works now", m.save(saved) && saved.contains("id"));
                        check("still in the world", !m.isRemoved() && m.isAlive());
                        check("its persistence given back: an ordinary mob despawns", !m.isPersistenceRequired());
                        taken.add(m);
                    }
                    check("not the party's now", ConflictKit.bridge().slotOf(warrior) == null && ConflictKit.bridge().slotOf(chief[0]) == null);
                    st.slots().get(1).entity().discard();
                    check("the other two stay", ConflictKit.bridge().bodies(agent).size() == 1);
                })
                .thenWaitUntil(() -> check("two casualties and the chief", ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent).size() == 2
                        && ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).size() == 1))
                .thenExecute(() -> {
                    List<ScriptedEngine.Received> cs = ConflictKit.about(ConflictRecords.BAND_CASUALTIES, agent);
                    ScriptedEngine.Received byPlayer = null, byOther = null;
                    for (ScriptedEngine.Received r : cs) {
                        if (r.aux() == ConflictRecords.BY_PLAYER)
                            byPlayer = r;
                        if (r.aux() == ConflictRecords.BY_OTHER)
                            byOther = r;
                    }
                    check("a pickup is the player's (kind 1)", byPlayer != null
                            && player.getUUID().equals(ConflictKit.uuidAt(byPlayer, ConflictRecords.Casualties.KILLER_MOST)));
                    check("a warrior", byPlayer.view().getInt(ConflictRecords.Casualties.BODY) == ConflictKit.bodyId(ConflictKit.WARRIOR));
                    check("a discard is no one's (kind 3)", byOther != null
                            && ConflictKit.uuidAt(byOther, ConflictRecords.Casualties.KILLER_MOST) == null);
                    ScriptedEngine.Received ck = ConflictKit.about(ConflictRecords.CHIEF_KILLED, agent).get(0);
                    check("the chief picked up is CHIEF_KILLED by the player", ck.aux() == ConflictRecords.BY_PLAYER
                            && player.getUUID().equals(ConflictKit.uuidAt(ck, ConflictRecords.ChiefKilled.KILLER_MOST)));
                    check("with a Java op id", (ck.view().getLong(ConflictRecords.ChiefKilled.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    check("no killing signal (" + (ConflictKit.bridge().signalsCounted - counted[0]) + ")",
                            ConflictKit.bridge().signalsCounted == counted[0]);
                    check("the chief's name is gone", chief[0].getCustomName() == null);
                    RtsimTests.record("carryon_conflict", "pickup", "a warrior: BAND_CASUALTIES kind 1, the chief: CHIEF_KILLED kind 1, by "
                            + player.getUUID() + "; a discard kind 3; no killing signal");
                    FakePlayers.remove(player);
                    ConflictKit.recall(agent, 1, ConflictRecords.RECALL_DONE, 0, c.x - 100, c.z);
                })
                .thenWaitUntil(() -> check("BAND_DEMATERIALIZED", !ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent).isEmpty()))
                .thenExecute(() -> {
                    Map<String, Integer> s = ConflictKit.survivors(ConflictKit.about(ConflictRecords.BAND_DEMATERIALIZED, agent).get(0));
                    check("the one left survives: " + s, s.equals(Map.of(ConflictKit.WARRIOR, 1)));
                    for (Mob m : taken)
                        if (!m.isRemoved())
                            m.discard();
                    ConflictKit.territoryRemoved(camp[0], dim);
                })
                .thenWaitUntil(() -> check("the territory removed", ConflictKit.bridge().territories().get(camp[0]) == null))
                .thenSucceed();
    }

    // ---- a camp's body picked up (MOWZIE.md §4.9, M5; the review's missing test) -------------------

    private static final String CAMP_STRUCTURE = "mic_rtsim_carryon:test_camp";

    /**
     * A stub camp kind: a vindicator is its body (the pickup path is the core's, so any kind will do). It makes
     * no bodies and holds nothing; the test adopts its one stray by hand.
     */
    private static final CampKind STUB_KIND = new CampKind() {
        @Override
        public boolean isBody(Entity e) {
            return e.getType() == EntityType.VINDICATOR;
        }

        @Override
        public String bodyName(Entity e) {
            return ConflictKit.WARRIOR;
        }

        @Override
        public boolean isChief(Entity e) {
            return false;
        }

        @Override
        public Chief chief(ServerLevel level, Camps.Site site, List<Entity> footprint) {
            return Chief.NONE;
        }

        @Override
        public Mob make(ServerLevel level, Camps.Site site, String body, int lineFlags, double x, double y, double z, float yaw) {
            Mob m = EntityType.VINDICATOR.create(level);
            if (m != null)
                m.moveTo(x, y, z, yaw, 0f);
            return m;
        }

        @Override
        public Vec3 spawnAt(ServerLevel level, Camps.Site site, String body, int lineFlags) {
            return null;
        }

        @Override
        public boolean holdSpawn(ServerLevel level, Camps.Site site, String body, int lineFlags) {
            return false;
        }

        @Override
        public void joined(Camps.Site site, Mob body) {}

        @Override
        public void copy(Mob from, Mob to) {}
    };

    /**
     * A real Carry On pickup (the event Carry On posts before it saves the entity, the path of the test
     * above) of a body a camp holds: CAMP_DEATH with killer kind taken (aux {@code BY_TAKEN}), the player's
     * UUID, the body's slot and generation and a Java op id; a record marker where it stood; its camp tag
     * gone before Carry On saves it, so the saved copy is an ordinary stray (a copy loaded from that
     * NBT is no camp body); and the discard Carry On does next books no second death. Carry On's own
     * {@code PickupHandler} cannot run on a GameTest player (its payload send fails on the fake
     * connection), so the event is the hook, as in the test above.
     */
    @GameTest(template = "arena", timeoutTicks = 1200, batch = "carryon_camp", skyAccess = true)
    public static void aCarryOnPickupOfACampBodyIsItsTakeAndLeavesAStray(GameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Vec3 c = RtsimTests.centre(helper);
        int[] base = ConflictKit.farBase();
        int cx = base[0] >> 4, cz = base[1] >> 4;
        ServerPlayer player = FakePlayers.add(level, c.add(-3, 0, 0), "carryon-camp");
        player.setGameMode(GameType.CREATIVE);
        long[] faction = {0};
        long[] op = {0};
        Mob[] body = new Mob[1];
        Vec3[] spot = new Vec3[1];
        CompoundTag[] saved = {new CompoundTag()};

        ConflictKit.onEnd(helper, () -> {
            FakePlayers.remove(player);
            if (body[0] != null && !body[0].isRemoved())
                body[0].discard();
            level.setChunkForced(cx, cz, false);
        });

        helper.startSequence()
                .thenExecute(() -> {
                    ConflictKit.releaseFarChunks(level);
                    level.setChunkForced(cx, cz, true);
                })
                .thenWaitUntil(() -> check("the camp's chunk", level.getChunkSource().getChunkNow(cx, cz) != null))
                .thenExecute(() -> {
                    double x = cx * 16 + 8.5, z = cz * 16 + 8.5;
                    spot[0] = new Vec3(x, level.getHeight(Heightmap.Types.MOTION_BLOCKING, (int) x, (int) z), z);
                    Mob v = EntityType.VINDICATOR.create(level);
                    v.moveTo(spot[0].x, spot[0].y, spot[0].z, 0f, 0f);
                    v.setNoAi(true);
                    // far from every player, a monster that is not persistent despawns on its first tick (the census
                    // then lists nothing); a camp's own bodies are persistent
                    v.setPersistenceRequired();
                    check("added", level.addFreshEntity(v));
                    body[0] = v;
                    faction[0] = RtsimApi.declareCamp(level, new ChunkPos(cx, cz), CAMP_STRUCTURE, STUB_KIND, BlockPos.containing(spot[0]),
                            spot[0], 4.0, 0.0, false);
                    check("the camp is declared", faction[0] != 0);
                })
                .thenWaitUntil(() -> check("the first census", !ConflictKit.censuses(faction[0]).isEmpty()))
                .thenExecute(() -> {
                    ScriptedEngine.Received census = ConflictKit.censuses(faction[0]).get(0);
                    List<ConflictKit.CensusBody> bodies = ConflictKit.censusBodies(census);
                    check("the census lists the one stray (" + bodies.size() + ")", bodies.size() == 1
                            && bodies.get(0).uuid().equals(body[0].getUUID())
                            && (bodies.get(0).flags() & ConflictRecords.CampCensus.BODY_STRAY) != 0);
                    op[0] = ConflictKit.newOp();
                    ConflictKit.reconcile(faction[0], census.view().getLong(ConflictRecords.CampCensus.EPOCH), op[0],
                            List.of(ConflictKit.CampLine.of(0, 1, ConflictKit.WARRIOR, ConflictRecords.CampReconcile.ADOPT, 0, body[0].getUUID())));
                })
                .thenWaitUntil(() -> check("the answer", !ConflictKit.campBodies(faction[0], op[0]).isEmpty()))
                .thenExecute(() -> {
                    Map<Integer, Integer> out = ConflictKit.outcomes(faction[0], op[0]);
                    check("ADOPT is DONE (" + out + ")", out.equals(Map.of(0, ConflictRecords.CampBodies.DONE)));
                    Mob v = body[0];
                    CampMember m = RtsimAttachments.campMember(v);
                    check("tagged (faction, 0, 1)", m != null && m.faction() == faction[0] && m.slot() == 0 && m.generation() == 1);
                    check("a body a camp holds, no puppet", RtsimApi.isCampBody(v) && !RtsimApi.isPuppet(v));
                    check("no death yet", ConflictKit.campDeaths(faction[0]).isEmpty());

                    EntityPickupEvent event = NeoForge.EVENT_BUS.post(new EntityPickupEvent(player, v));
                    check("nobody cancelled the pickup", !event.isCanceled());
                    check("no longer a camp body", !RtsimApi.isCampBody(v));
                    CampMember after = RtsimAttachments.campMember(v);
                    check("its tag is gone", after == null || after.isNone());
                    check("Carry On's save of it works", v.save(saved[0]) && saved[0].contains("id"));
                    Entity copy = EntityType.create(saved[0], level).orElse(null);
                    check("a copy loaded from the saved NBT is no camp body", copy != null && !RtsimApi.isCampBody(copy));
                    check("still in the world, alive", !v.isRemoved() && v.isAlive());
                    check("a record marker where it stood", !markers(level, v.position()).isEmpty());
                    CompoundTag mk = new CompoundTag();
                    markers(level, v.position()).get(0).saveWithoutId(mk);
                    check("carrying the records", mk.getCompound("neoforge:attachments").contains("mic_rtsim:pending"));
                })
                .thenWaitUntil(() -> check("CAMP_DEATH", ConflictKit.campDeaths(faction[0]).size() == 1))
                .thenExecute(() -> {
                    ScriptedEngine.Received d = ConflictKit.campDeaths(faction[0]).get(0);
                    check("by taken (" + d.aux() + ")", d.aux() == ConflictRecords.BY_TAKEN);
                    check("by the player", player.getUUID().equals(ConflictKit.uuidAt(d, ConflictRecords.CampDeath.KILLER_MOST)));
                    check("a warrior", d.view().getInt(ConflictRecords.CampDeath.BODY) == ConflictKit.bodyId(ConflictKit.WARRIOR));
                    check("slot 0, generation 1", d.view().getInt(ConflictRecords.CampDeath.SLOT) == 0
                            && d.view().getInt(ConflictRecords.CampDeath.GENERATION) == 1);
                    check("not the chief", (d.view().getInt(ConflictRecords.CampDeath.FLAGS) & ConflictRecords.CampDeath.FLAG_CHIEF) == 0);
                    check("with a Java op id", (d.view().getLong(ConflictRecords.CampDeath.OP_ID) & Obs.JAVA_OP_BIT) != 0);
                    RtsimTests.record("carryon_camp", "pickup", "CAMP_DEATH aux BY_TAKEN by " + player.getUUID()
                            + ", slot 0 generation 1, a Java op id; the tag gone before the save; a marker carries the records");
                    body[0].discard(); // Carry On removes what it picked up
                })
                .thenIdle(20)
                .thenExecute(() -> check("the discard booked no second death (" + ConflictKit.campDeaths(faction[0]).size() + ")",
                        ConflictKit.campDeaths(faction[0]).size() == 1))
                .thenSucceed();
    }

    private static List<Marker> markers(ServerLevel level, Vec3 at) {
        return level.getEntities(EntityType.MARKER, AABB.ofSize(at, 6, 6, 6), e -> e.getTags().contains(RtsimSession.RECORD_MARKER_TAG));
    }
}
