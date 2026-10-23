package com.minecraftindustrialcomplex.mic_rtsim_carryon.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictBridge;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.engine.ScriptedEngine;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.ConflictKit;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.FakePlayers;
import com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests;
import com.minecraftindustrialcomplex.mic_rtsim_carryon.MicRtsimCarryOn;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.level.GameType;
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
     * Each body stays in the world as an ordinary mob ({@code Entity.save} works, no name). A member
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
                .thenWaitUntil(() -> check("placed", ConflictKit.party(agent).stacks().get(0).bodies().size() == 4))
                .thenExecute(() -> {
                    counted[0] = ConflictKit.bridge().signalsCounted;
                    ConflictBridge.Stack st = ConflictKit.party(agent).stacks().get(0);
                    Mob warrior = st.slots().get(0).entity();
                    for (ConflictBridge.Slot sl : st.slots())
                        if (sl.chief)
                            chief[0] = sl.entity();
                    for (Mob m : List.of(warrior, chief[0])) {
                        check("a puppet: Entity.save refuses it", RtsimApi.isPuppet(m) && !m.save(new CompoundTag()));
                        EntityPickupEvent event = NeoForge.EVENT_BUS.post(new EntityPickupEvent(player, m));
                        check("nobody cancelled the pickup", !event.isCanceled());
                        check("no longer a puppet", !RtsimApi.isPuppet(m));
                        CompoundTag saved = new CompoundTag();
                        check("Carry On's save of it works now", m.save(saved) && saved.contains("id"));
                        check("still in the world", !m.isRemoved() && m.isAlive());
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
}
