package com.minecraftindustrialcomplex.mic_rtsim_create.client;

import com.minecraftindustrialcomplex.mic_rtsim.client.FarLooks;
import com.minecraftindustrialcomplex.mic_rtsim.client.FarModel;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarClock;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarKind;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarMover;
import com.minecraftindustrialcomplex.mic_rtsim.far.FarObjects;
import com.minecraftindustrialcomplex.mic_rtsim_create.MicRtsimCreate;
import com.simibubi.create.content.trains.entity.CarriageContraptionEntity;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import org.joml.Vector3f;

/**
 * Bench only ({@code -Dmic_rtsim.farbench=true}; docs/choices-dh-feed.md DHT64): checks the far model's placement
 * against Create's own. Once a second, for every far carriage whose entity the client also has, four points of
 * the carriage (its midpoint, and a step along, up and across from it in the carriage's own frame) go through
 * the far model's turn and through the entity's {@link CarriageContraptionEntity#toGlobalVector}, and the gap is
 * logged split along the track (mostly the feed's extrapolation against the entity's interpolation), up and
 * across (the placement). Reads only; draws nothing.
 */
public final class CarriageProbe {
    private static final double[] SEG = new double[6];
    private static final FarModel MODEL = new FarModel();
    private static final float[][] STEPS = {{0, 0, 0}, {3, 0, 0}, {0, 2, 0}, {0, 0, 1}};
    private static int ticks;

    private CarriageProbe() {}

    public static void onClientTick(ClientTickEvent.Post event) {
        Minecraft mc = Minecraft.getInstance();
        ClientLevel level = mc.level;
        if (level == null || mc.isPaused() || ++ticks % 20 != 0)
            return;
        if (!(FarLooks.of(FarKind.CARRIAGE) instanceof CarriageLook look))
            return;
        String dimension = level.dimension().location().toString();
        double clock = FarClock.ticks();
        int n = 0;
        double upMax = 0, upSum = 0, sideMax = 0, alongMax = 0;
        for (FarMover m : FarObjects.movers()) {
            if (m.kind() != FarKind.CARRIAGE || !dimension.equals(m.dimension()) || m.entity() < 0)
                continue;
            if (!(level.getEntity(m.entity()) instanceof CarriageContraptionEntity cce) || cce.isRemoved())
                continue;
            float[] mid = look.midOf(m.shape());
            if (mid == null)
                continue;
            m.segmentAt(clock, SEG);
            if (!look.place(m, SEG, clock, false, MODEL.reset()))
                continue;
            double cx = (SEG[0] + SEG[3]) / 2.0, cy = (SEG[1] + SEG[4]) / 2.0, cz = (SEG[2] + SEG[5]) / 2.0;
            double ax = SEG[0] - SEG[3], ay = SEG[1] - SEG[4], az = SEG[2] - SEG[5];
            double al = Math.sqrt(ax * ax + ay * ay + az * az);
            if (al < 1e-6)
                continue;
            ax /= al;
            ay /= al;
            az /= al;
            n++;
            for (float[] s : STEPS) {
                float px = mid[0] + s[0], py = mid[1] + s[1], pz = mid[2] + s[2];
                Vector3f w = MODEL.local.transformPosition(px, py, pz, new Vector3f());
                Vec3 g = cce.toGlobalVector(new Vec3(px, py, pz), 1f);
                double dx = cx + w.x - g.x, dy = cy + w.y - g.y, dz = cz + w.z - g.z;
                double along = dx * ax + dy * ay + dz * az;
                double rx = dx - along * ax, ry = dy - along * ay, rz = dz - along * az;
                upMax = Math.max(upMax, Math.abs(ry));
                upSum += Math.abs(ry);
                sideMax = Math.max(sideMax, Math.sqrt(rx * rx + rz * rz));
                alongMax = Math.max(alongMax, Math.abs(along));
            }
        }
        if (n > 0)
            MicRtsimCreate.LOGGER.info("far probe carriages {} up max {} mean {} side max {} along max {}", n,
                    String.format("%.3f", upMax), String.format("%.3f", upSum / (n * STEPS.length)),
                    String.format("%.3f", sideMax), String.format("%.3f", alongMax));
    }
}
