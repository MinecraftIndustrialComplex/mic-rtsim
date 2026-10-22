package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.client.FarFeedClient;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import java.io.File;
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
 * <p>Phase 2 (docs/choices-dh-feed.md) adds made-up dragons ({@link FarDragonBench}), a band ring of trains
 * inside the loaded chunks, the simulation distance ({@code mic_rtsim.farbench.sim}, 5 when anything is in
 * the band, vanilla's least, else 12) and the far feed's meter; {@code mic_rtsim.farbench.feed=false} leaves the feed off, so
 * the trains come by Create's own map sync as in phase 1.
 *
 * <p>Every line it logs starts with {@code far bench}.
 */
final class FarBench {
    static final boolean ENABLED = Boolean.getBoolean("mic_rtsim.farbench");
    static final String WORLD = "farbench";
    private static final String LABEL = System.getProperty("mic_rtsim.farbench.label", "bench");
    /** The tier the bench draws at, over the config ({@code -PfarBenchDetail=HIGH}); null keeps the config's. */
    static final FarDetail DETAIL = ENABLED ? parse(System.getProperty("mic_rtsim.farbench.detail")) : null;
    /** The bench without the far feed: no subscription, so Create's map sync carries the trains. */
    static final boolean NO_FEED = ENABLED && "false".equals(System.getProperty("mic_rtsim.farbench.feed"));
    private static final boolean BAND = Integer.getInteger("mic_rtsim.farbench.bandTrains", 0) > 0
            || Integer.getInteger("mic_rtsim.farbench.bandDragons", 0) > 0;
    private static final int SIMULATION = Integer.getInteger("mic_rtsim.farbench.sim", BAND ? 5 : 12);

    /** Client ticks in the world before the profile starts: DH builds its flat LODs and every train is placed. */
    private static final int WARMUP_TICKS = 900;
    /** Client ticks the profile window lasts; vanilla's profiler stops itself after 10 seconds. */
    private static final int PROFILE_TICKS = 260;
    private static final int LOG_EVERY = 100;
    /** Client ticks the bench waits after the window for vanilla to write the profile's zip. */
    private static final int SAVE_TICKS = 400;

    private enum Phase { START, OPENING, WARMUP, PROFILING, SAVING, DONE }

    private static Phase phase = Phase.START;
    private static int ticks;
    private static long windowFrame0, windowNanos0, windowMillis0;
    private static long[] feed0;

    private FarBench() {}

    static void register() {
        NeoForge.EVENT_BUS.addListener(FarBench::onClientTick);
        FarDragonBench.register();
        MicRtsimDh.LOGGER.info("far bench: on ({}), detail {}, far feed {}, simulation distance {}", LABEL,
                DETAIL == null ? "from config" : DETAIL, NO_FEED ? "off" : "on", SIMULATION);
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
                mc.options.renderDistance().set(12);
                mc.options.simulationDistance().set(SIMULATION);
                if (mc.options.simulationDistance().get() != SIMULATION) {
                    // Vanilla refuses a value outside 5 to 32 and keeps the old one: a band run would then tick its ring
                    fail("simulation distance " + SIMULATION + " refused (vanilla allows 5 to 32)");
                    return;
                }
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
                FarDragonView.recuts = FarDragonView.confirms = FarDragonView.flips = 0;
                FarBandRenderer.nanos = 0;
                FarBandRenderer.frames = 0;
                feed0 = FarFeedClient.meter();
                windowFrame0 = FarTrainView.frame();
                windowNanos0 = System.nanoTime();
                windowMillis0 = System.currentTimeMillis();
                ticks = 0;
                phase = Phase.PROFILING;
            }
            case PROFILING -> {
                if (++ticks < PROFILE_TICKS)
                    return;
                double seconds = (System.nanoTime() - windowNanos0) / 1e9;
                long frames = FarTrainView.frame() - windowFrame0;
                long[] feed1 = FarFeedClient.meter();
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
                MicRtsimDh.LOGGER.info(String.format(Locale.ROOT,
                        "far bench result %s dragons: drawn %d, groups %d, boxes %d, registered %d, hidden %d; "
                                + "recuts %d, confirms %d, wing flips %d in the window; band: %d movers, %d boxes, "
                                + "%.1f us/frame over %d frames",
                        LABEL, FarDragonView.lastDrawn, FarDragonView.lastGroups, FarDragonView.lastBoxes,
                        FarDragonView.lastRegistered, FarDragonView.lastHidden, FarDragonView.recuts,
                        FarDragonView.confirms, FarDragonView.flips, FarBandRenderer.lastDrawn,
                        FarBandRenderer.lastBoxes,
                        FarBandRenderer.frames == 0 ? 0.0 : FarBandRenderer.nanos / 1e3 / FarBandRenderer.frames,
                        FarBandRenderer.frames));
                MicRtsimDh.LOGGER.info(String.format(Locale.ROOT,
                        "far bench result %s feed: %s; %d messages, %d bytes, %d records in %.2f s: %.1f messages/s, "
                                + "%.0f bytes/s, %.1f records/s; holding %d objects; %d did not decode",
                        LABEL, FarFeedClient.active() ? "active" : "not active", feed1[0] - feed0[0],
                        feed1[1] - feed0[1], feed1[2] - feed0[2], seconds, (feed1[0] - feed0[0]) / seconds,
                        (feed1[1] - feed0[1]) / seconds, (feed1[2] - feed0[2]) / seconds, FarFeedClient.held(),
                        feed1[3]));
                ticks = 0;
                phase = Phase.SAVING;
            }
            case SAVING -> {
                // Vanilla zips the profile once the server's half is in; quitting first loses it
                File[] zips = new File(mc.gameDirectory, "debug/profiling").listFiles(
                        f -> f.getName().endsWith(".zip") && f.lastModified() >= windowMillis0);
                boolean saved = zips != null && zips.length > 0;
                if (!saved && ++ticks < SAVE_TICKS)
                    return;
                MicRtsimDh.LOGGER.info("far bench: profile {}", saved ? "saved as " + zips[0].getName()
                        : "not saved after " + SAVE_TICKS + " ticks");
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
                        + "controller %.1f us/frame; dragons drawn %d, hidden %d; band %d; feed %s, %d held, "
                        + "%d bytes in",
                LABEL, when, ticks, mc.getFps(), FarGates.state(), FarObjects.movers().size(),
                FarTrainView.lastCandidates, FarTrainView.lastActive, FarTrainView.lastBoxes,
                FarTrainView.lastRegistered, FarTrainView.lastHidden, FarTrainView.recuts, FarTrainView.confirms,
                FarTrainView.creations, FarTrainView.releases,
                FarTrainView.controllerFrames == 0 ? 0.0
                        : FarTrainView.controllerNanos / 1e3 / FarTrainView.controllerFrames,
                FarDragonView.lastDrawn, FarDragonView.lastHidden, FarBandRenderer.lastDrawn,
                FarFeedClient.active() ? "active" : "not active", FarFeedClient.held(), FarFeedClient.meter()[1]));
    }

    private static void fail(String why) {
        MicRtsimDh.LOGGER.error("far bench: {}; quitting", why);
        phase = Phase.DONE;
        Minecraft.getInstance().stop();
    }
}
