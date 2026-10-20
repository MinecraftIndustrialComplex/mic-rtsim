package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.MinecraftServer;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;

/** {@code /rtsim perf} and {@code /rtsim stub caravan} run through the server's own command dispatcher. */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class CommandGameTests {
    private CommandGameTests() {}

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200)
    public static void rtsimCommandsRun(GameTestHelper helper) {
        MinecraftServer server = helper.getLevel().getServer();
        CommandSourceStack source = server.createCommandSourceStack()
                .withLevel(helper.getLevel())
                .withPosition(RtsimTests.centre(helper).add(-4, 0, 0))
                .withSuppressedOutput();
        long[] agent = {-1};
        server.getCommands().performPrefixedCommand(source, "rtsim perf");
        server.getCommands().performPrefixedCommand(source, "rtsim stub caravan");
        helper.startSequence()
                .thenWaitUntil(() -> {
                    for (PuppetGroup g : RtsimTests.session().registry().groups())
                        if (g.agent >= (1L << 40))
                            agent[0] = g.agent;
                    check("the stub caravan the command asked for is embodied", agent[0] >= 0);
                })
                .thenExecute(() -> RtsimTests.stub().despawnGroup(agent[0], 1))
                .thenWaitUntil(() -> check("and despawned again", RtsimTests.group(agent[0]) == null))
                .thenSucceed();
    }
}
