package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.engine.NativeEngine;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import com.minecraftindustrialcomplex.mic_rtsim.village.Village;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestAssertException;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.animal.horse.TraderLlama;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.List;
import java.util.concurrent.ThreadLocalRandom;

/**
 * DIAGNOSIS ONLY (fix/robbery-flake): the live caravan test's scenario repeated many times in one test, with
 * what each repeat saw logged. Not to be landed.
 */
@GameTestHolder("mic_rtsim_rob")
@PrefixGameTestTemplate(false)
public final class RobberyStressGameTests {
    private RobberyStressGameTests() {}

    private static final int ROUNDS = Integer.getInteger("rob.rounds", 40);
    private static final int BOOK_TICKS = 3_000;

    @GameTest(template = RtsimTests.TEMPLATE, templateNamespace = "mic_rtsim", timeoutTicks = 3_000_000, batch = "rob_stress")
    public static void robberyStress(GameTestHelper helper) {
        if (!(RtsimTests.engine() instanceof NativeEngine e)) {
            helper.succeed();
            return;
        }
        String dim = RtsimTests.dimension(helper);
        Vec3 at = RtsimTests.centre(helper);
        ServerPlayer player = FakePlayers.add(helper.getLevel(), at, "rob_stress");
        // The state of the repeat in progress.
        int[] round = {0}, phase = {0};
        long[] tickMark = {0}, stepMark = {0}, agent = {0};
        long[][] base = {null};
        int[] asks = {0}, askTick = {0};
        List<Village> villages = new java.util.ArrayList<>();
        int[] booked = {0}, notBooked = {0};
        String[] robbed = {null};

        helper.startSequence().thenWaitUntil(() -> {
            long now = helper.getLevel().getGameTime();
            switch (phase[0]) {
                case 0 -> { // set up a repeat
                    int ox = 2_000_000 + ThreadLocalRandom.current().nextInt(0, 1_000) * 2_000;
                    int oz = -2_000_000 - ThreadLocalRandom.current().nextInt(0, 1_000) * 2_000;
                    villages.clear();
                    villages.add(RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_plains", new BlockPos(ox, 64, oz), 20, 90));
                    villages.add(RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_taiga", new BlockPos(ox + 400, 64, oz), 20, 90));
                    villages.add(RtsimTests.adoptArenaLive(helper, "human", "minecraft:village_desert", new BlockPos(ox, 64, oz + 400), 20, 90));
                    agent[0] = RtsimTests.newAgent();
                    base[0] = e.phase1Stats();
                    asks[0] = 0;
                    askTick[0] = 0;
                    robbed[0] = null;
                    phase[0] = 1;
                    tickMark[0] = now;
                    stepMark[0] = e.steps();
                    throw new GameTestAssertException("set up");
                }
                case 1 -> { // the engine knows the villages
                    for (Village v : villages)
                        if (e.phase1Sentiment(v.id, player.getUUID()) == Integer.MIN_VALUE)
                            throw new GameTestAssertException("the engine does not know the villages yet");
                    phase[0] = 2;
                    throw new GameTestAssertException("villages known");
                }
                case 2 -> { // ask until a caravan is there
                    if (RtsimTests.group(agent[0]) != null) {
                        phase[0] = 3;
                        throw new GameTestAssertException("caravan there");
                    }
                    if (askTick[0]++ % 10 == 0) {
                        if (asks[0] >= 300) {
                            MicRtsim.LOGGER.info("[gametest] robStress round {}: no caravan within 300 asks", round[0]);
                            phase[0] = 5;
                            throw new GameTestAssertException("no caravan");
                        }
                        e.phase1Advance(6_000);
                        Village a = villages.get(0);
                        e.phase1SpawnNear(agent[0], 1, dim, at.x, at.y, at.z, 2_000, a.meeting.getX() + 150, a.meeting.getZ() + 150);
                        asks[0]++;
                    }
                    throw new GameTestAssertException("no caravan yet");
                }
                case 3 -> { // rob
                    PuppetGroup g = RtsimTests.group(agent[0]);
                    for (int i = 1; i <= 2 && robbed[0] == null; i++) {
                        TraderLlama llama = (TraderLlama) RtsimTests.member(g, i);
                        for (int k = 0; k < 3 && robbed[0] == null; k++) {
                            ItemStack s = llama.getSlot(500 + k).get();
                            if (!s.isEmpty()) {
                                int n = Math.min(5, s.getCount());
                                robbed[0] = n + " " + s.getItem();
                                s.shrink(n);
                            }
                        }
                    }
                    if (robbed[0] == null) {
                        MicRtsim.LOGGER.info("[gametest] robStress round {}: nothing to rob after {} asks", round[0], asks[0]);
                        phase[0] = 5;
                        throw new GameTestAssertException("nothing to rob");
                    }
                    tickMark[0] = now;
                    stepMark[0] = e.steps();
                    phase[0] = 4;
                    throw new GameTestAssertException("robbed");
                }
                case 4 -> { // booked, or not within the budget
                    long[] s = e.phase1Stats();
                    boolean yes = s[NativeEngine.P1_ROBBERIES] > base[0][NativeEngine.P1_ROBBERIES];
                    if (yes || now - tickMark[0] >= BOOK_TICKS) {
                        PuppetGroup g = RtsimTests.group(agent[0]);
                        MicRtsim.LOGGER.info("[gametest] robStress round {}: {} after {} asks; robbed {}; {} ticks and {} steps after the robbery; robberies {} vs base {}; last step {} us; group {}",
                                round[0], yes ? "BOOKED" : "NOT BOOKED", asks[0], robbed[0], now - tickMark[0], e.steps() - stepMark[0],
                                s[NativeEngine.P1_ROBBERIES], base[0][NativeEngine.P1_ROBBERIES], e.lastStepNanos() / 1000,
                                g == null ? "gone" : "there, llama 1 at " + RtsimTests.member(g, 1).distanceTo(player));
                        if (yes)
                            booked[0]++;
                        else
                            notBooked[0]++;
                        phase[0] = 5;
                        throw new GameTestAssertException("done");
                    }
                    throw new GameTestAssertException("not booked yet");
                }
                case 5 -> { // clean up
                    if (RtsimTests.group(agent[0]) != null) {
                        e.despawnGroup(agent[0], 1);
                        throw new GameTestAssertException("despawning");
                    }
                    villages.forEach(RtsimTests::forget);
                    round[0]++;
                    phase[0] = 0;
                    if (round[0] >= ROUNDS) {
                        MicRtsim.LOGGER.info("[gametest] robStress SUMMARY: {} booked, {} not booked of {}", booked[0], notBooked[0], ROUNDS);
                        return;
                    }
                    throw new GameTestAssertException("next round");
                }
                default -> throw new IllegalStateException();
            }
        }).thenExecute(() -> FakePlayers.remove(player)).thenSucceed();
    }
}
