package com.minecraftindustrialcomplex.mic_rtsim_dh;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarItem;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarSource;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarState;
import java.util.function.Consumer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;

/**
 * The far-view bench's dragons (docs/choices-dh-feed.md): a far feed source on the single-player server that
 * makes up dragons the way the engine and Ice and Fire would give them, so the bench needs neither. Only with
 * {@code -Dmic_rtsim.farbench=true}; inert otherwise.
 *
 * <ul>
 *   <li><b>Far dragons</b> ({@code mic_rtsim.farbench.dragons}) in the rings' sector, 500 to 1,400 blocks east of
 *       spawn, at every stage and colour: a third fly legs (30 s back and forth, as the engine's FAR_LEG gives
 *       them), a third hunt (circling), and a third are sampled every second the way Ice and Fire's embodied
 *       dragons are, flying circles, with no entity on this client (as a dragon embodied for another
 *       player).</li>
 *   <li><b>Band dragons</b> ({@code mic_rtsim.farbench.bandDragons}) 110 to 160 blocks east, inside the
 *       client's loaded chunks with no entity, so {@link FarBandRenderer} draws them.</li>
 * </ul>
 */
final class FarDragonBench implements FarSource {
    static final int DRAGONS = Math.max(0, Integer.getInteger("mic_rtsim.farbench.dragons", 0));
    static final int BAND_DRAGONS = Math.max(0, Integer.getInteger("mic_rtsim.farbench.bandDragons", 0));
    /** Ids like the engine's dragons (agent 0x44 << 56), from a block of handles no engine gives out. */
    static final long ID0 = 0x44L << 56 | 0xBE_0000L;
    /** A leg lasts this long, in game ticks. */
    static final long LEG_TICKS = 600;
    /** Each species' four colours, as mic_rtsim_iceandfire's palette has them. */
    static final int[][] PALETTE = {
            {0x8E1B1B, 0x2E5E2A, 0x8C6A2F, 0x5E5E5E},
            {0x6FA8C8, 0xD8E2E8, 0x2A4E9C, 0x9AA3AA},
            {0x2B3A8C, 0x7A4A9C, 0xA0603A, 0x222226}};

    private FarDragonBench() {}

    static void register() {
        if (DRAGONS + BAND_DRAGONS == 0)
            return;
        RtsimApi.registerFarSource(new FarDragonBench());
        MicRtsimDh.LOGGER.info("far bench: {} far dragons and {} band dragons", DRAGONS, BAND_DRAGONS);
    }

    @Override
    public String name() {
        return MicRtsimDh.MODID + ":bench_dragons";
    }

    @Override
    public void collect(ServerLevel level, long gameTime, Consumer<FarItem> out) {
        if (level.dimension() != Level.OVERWORLD)
            return;
        for (int i = 0; i < DRAGONS; i++) {
            // A golden-angle spiral over the sector east of spawn
            double a = Math.toRadians(-35.0 + (i * 137.508) % 70.0);
            double d = 500.0 + (i * 211.0) % 900.0;
            out.accept(dragon(i, Math.cos(a) * d, Math.sin(a) * d, -20.0 + (i * 37) % 80, i % 3, 60.0, gameTime));
        }
        for (int i = 0; i < BAND_DRAGONS; i++) {
            double a = Math.toRadians(-30.0 + (BAND_DRAGONS == 1 ? 30.0 : 60.0 * i / (BAND_DRAGONS - 1)));
            double d = 120.0 + (i * 13) % 30;
            out.accept(dragon(DRAGONS + i, Math.cos(a) * d, Math.sin(a) * d, -35.0 + (i * 7) % 20, i % 2, 24.0,
                    gameTime));
        }
    }

    /** Dragon {@code i} around (cx, cz): mode 0 flies legs, 1 hunts, 2 is sampled flying a circle. */
    private static FarItem dragon(int i, double cx, double cz, double y, int mode, double reach, long gameTime) {
        int style = i % 3, variant = (i / 3) % 4, stage = 1 + (i % 5);
        int rgb = PALETTE[style][variant];
        long id = ID0 + i;
        switch (mode) {
            case 0 -> {
                // Back and forth along a line through the centre, a leg each LEG_TICKS
                long k = Math.floorDiv(gameTime + i * 97L, LEG_TICKS);
                long t0 = k * LEG_TICKS - i * 97L;
                double dir = Math.toRadians((i * 53) % 180);
                double ex = Math.cos(dir) * reach * 2.5, ez = Math.sin(dir) * reach * 2.5;
                int s = (k & 1) == 0 ? 1 : -1;
                FarItem.Leg leg = new FarItem.Leg((int) y, (int) Math.floor(cx - s * ex), (int) Math.floor(cz - s * ez),
                        (int) Math.floor(cx + s * ex), (int) Math.floor(cz + s * ez), t0, t0 + LEG_TICKS, 0);
                return new FarItem(id, FarKind.DRAGON, style, variant, stage, rgb, FarState.FLY, null, leg, -1, 0);
            }
            case 1 -> {
                FarItem.Leg leg = new FarItem.Leg((int) y, (int) Math.floor(cx), (int) Math.floor(cz),
                        (int) Math.floor(cx), (int) Math.floor(cz), 0, Long.MAX_VALUE / 4, (int) reach);
                return new FarItem(id, FarKind.DRAGON, style, variant, stage, rgb, FarState.HUNT, null, leg, -1, 0);
            }
            default -> {
                // 0.5 blocks a tick round a circle, as a dragon in the game would fly it
                double w = 0.5 / reach * ((i & 1) == 0 ? 1 : -1);
                double ph = gameTime * w + i;
                double x = cx + Math.cos(ph) * reach, z = cz + Math.sin(ph) * reach;
                double hx = -Math.sin(ph) * Math.signum(w), hz = Math.cos(ph) * Math.signum(w);
                double[] seg = {x + hx * 0.5, y, z + hz * 0.5, x - hx * 0.5, y, z - hz * 0.5};
                return new FarItem(id, FarKind.DRAGON, style, variant, stage, rgb, FarState.FLY, seg, null, -1, 10);
            }
        }
    }
}
