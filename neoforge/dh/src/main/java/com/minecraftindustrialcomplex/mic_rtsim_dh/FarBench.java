package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import java.util.Locale;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameRules;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.common.NeoForge;

/**
 * The far-view bench, client side (docs/choices-dh-trains.md): only with {@code -Dmic_rtsim.farbench=true}, which
 * {@code ./gradlew :dh:runClient -PfarBench=<trains>} sets. It opens (or creates) the flat world {@value #WORLD},
 * parks a spectator at spawn looking east at the rings of synthetic trains mic_rtsim_create's bench lays out
 * (500 to 1500 blocks away, past vanilla's view and inside DH's), waits for the view to settle, takes a
 * screenshot, records vanilla's 10-second client profile (the one F3+L takes, with DH's "Custom Objects" section
 * in it), logs this controller's own cost over the same window and quits.
 *
 * <p>Every line it logs starts with {@code far bench}.
 */
final class FarBench {
    static final boolean ENABLED = Boolean.getBoolean("mic_rtsim.farbench");
    static final String WORLD = "farbench";
    private static final String LABEL = System.getProperty("mic_rtsim.farbench.label", "bench");
    /** The tier the bench draws at, over the config ({@code -PfarBenchDetail=HIGH}); null keeps the config's. */
    static final FarDetail DETAIL = ENABLED ? parse(System.getProperty("mic_rtsim.farbench.detail")) : null;

    /** Client ticks in the world before the profile starts: DH builds its flat LODs and every train is placed. */
    private static final int WARMUP_TICKS = 900;
    /** Client ticks the profile window lasts; vanilla's profiler stops itself after 10 seconds. */
    private static final int PROFILE_TICKS = 260;
    private static final int LOG_EVERY = 100;

    private enum Phase { START, OPENING, WARMUP, PROFILING, DONE }

    private static Phase phase = Phase.START;
    private static int ticks;
    private static long windowFrame0, windowNanos0;

    private FarBench() {}

    static void register() {
        NeoForge.EVENT_BUS.addListener(FarBench::onClientTick);
        MicRtsimDh.LOGGER.info("far bench: on ({}), detail {}", LABEL, DETAIL == null ? "from config" : DETAIL);
    }

    private static FarDetail parse(String s) {
        if (s == null || s.isBlank())
            return null;
        return FarDetail.valueOf(s.trim().toUpperCase(Locale.ROOT));
    }

    private static void onClientTick(ClientTickEvent.Post event) {
        Minecraft mc = Minecraft.getInstance();
        switch (phase) {
            case START -> {
                // Wait for the loading overlay; then any screen (onboarding or title) will do
                if (mc.getOverlay() != null || mc.screen == null || mc.level != null)
                    return;
                if (++ticks < 20)
                    return;
                ticks = 0;
                mc.options.onboardAccessibility = false;
                mc.options.pauseOnLostFocus = false;
                mc.options.hideGui = true;
                mc.options.enableVsync().set(false);
                mc.options.framerateLimit().set(260);
                mc.options.save();
                phase = Phase.OPENING;
                if (mc.getLevelSource().levelExists(WORLD)) {
                    MicRtsimDh.LOGGER.info("far bench: opening {}", WORLD);
                    mc.createWorldOpenFlows().openWorld(WORLD, () -> fail("could not open " + WORLD));
                } else {
                    MicRtsimDh.LOGGER.info("far bench: creating the flat world {}", WORLD);
                    LevelSettings settings = new LevelSettings(WORLD, GameType.CREATIVE, false, Difficulty.PEACEFUL,
                            true, new GameRules(), WorldDataConfiguration.DEFAULT);
                    mc.createWorldOpenFlows().createFreshLevel(WORLD, settings, new WorldOptions(20261022L, false, false),
                            ra -> ra.registryOrThrow(Registries.WORLD_PRESET).getHolderOrThrow(WorldPresets.FLAT).value()
                                    .createWorldDimensions(),
                            mc.screen);
                }
            }
            case OPENING -> {
                if (mc.level == null || mc.player == null || mc.player.connection == null)
                    return;
                for (String c : new String[] {"gamemode spectator", "tp @s 0 -45 0 -90 5", "time set noon",
                        "gamerule doDaylightCycle false", "weather clear", "gamerule doWeatherCycle false",
                        "gamerule doMobSpawning false"})
                    mc.player.connection.sendCommand(c);
                MicRtsimDh.LOGGER.info("far bench: in the world; render distance {}, simulation distance {}",
                        mc.options.getEffectiveRenderDistance(), mc.options.simulationDistance().get());
                ticks = 0;
                phase = Phase.WARMUP;
            }
            case WARMUP -> {
                if (mc.level == null)
                    return;
                ticks++;
                if (ticks % LOG_EVERY == 0)
                    log("warmup");
                if (ticks == WARMUP_TICKS - 20)
                    Screenshot.grab(mc.gameDirectory, "farbench-" + LABEL + ".png", mc.getMainRenderTarget(),
                            msg -> MicRtsimDh.LOGGER.info("far bench: screenshot: {}", msg.getString()));
                if (ticks < WARMUP_TICKS)
                    return;
                boolean started = mc.debugClientMetricsStart(
                        msg -> MicRtsimDh.LOGGER.info("far bench: profiler: {}", msg.getString()));
                MicRtsimDh.LOGGER.info("far bench: profile window starts (profiler started: {})", started);
                FarTrainView.controllerNanos = 0;
                FarTrainView.controllerFrames = 0;
                FarTrainView.recuts = FarTrainView.confirms = 0;
                windowFrame0 = FarTrainView.frame();
                windowNanos0 = System.nanoTime();
                ticks = 0;
                phase = Phase.PROFILING;
            }
            case PROFILING -> {
                if (++ticks < PROFILE_TICKS)
                    return;
                double seconds = (System.nanoTime() - windowNanos0) / 1e9;
                long frames = FarTrainView.frame() - windowFrame0;
                MicRtsimDh.LOGGER.info(String.format(Locale.ROOT,
                        "far bench result %s: %d frames in %.2f s (%.1f fps); controller %.1f us/frame over %d "
                                + "frames; active %d, boxes %d, registered %d, hidden %d, movers %d; recuts %d, "
                                + "confirms %d in the window",
                        LABEL, frames, seconds, frames / seconds,
                        FarTrainView.controllerFrames == 0 ? 0.0
                                : FarTrainView.controllerNanos / 1e3 / FarTrainView.controllerFrames,
                        FarTrainView.controllerFrames, FarTrainView.lastActive, FarTrainView.lastBoxes,
                        FarTrainView.lastRegistered, FarTrainView.lastHidden, FarObjects.movers().size(),
                        FarTrainView.recuts, FarTrainView.confirms));
                phase = Phase.DONE;
                mc.stop();
            }
            case DONE -> {
            }
        }
    }

    private static void log(String when) {
        Minecraft mc = Minecraft.getInstance();
        MicRtsimDh.LOGGER.info(String.format(Locale.ROOT,
                "far bench %s %s: tick %d, fps %d, gates %s, movers %d, candidates %d, active %d, boxes %d, "
                        + "registered %d, hidden %d; recuts %d, confirms %d, creations %d, releases %d; "
                        + "controller %.1f us/frame",
                LABEL, when, ticks, mc.getFps(), FarGates.state(), FarObjects.movers().size(),
                FarTrainView.lastCandidates, FarTrainView.lastActive, FarTrainView.lastBoxes,
                FarTrainView.lastRegistered, FarTrainView.lastHidden, FarTrainView.recuts, FarTrainView.confirms,
                FarTrainView.creations, FarTrainView.releases,
                FarTrainView.controllerFrames == 0 ? 0.0
                        : FarTrainView.controllerNanos / 1e3 / FarTrainView.controllerFrames));
    }

    private static void fail(String why) {
        MicRtsimDh.LOGGER.error("far bench: {}; quitting", why);
        phase = Phase.DONE;
        Minecraft.getInstance().stop();
    }
}
