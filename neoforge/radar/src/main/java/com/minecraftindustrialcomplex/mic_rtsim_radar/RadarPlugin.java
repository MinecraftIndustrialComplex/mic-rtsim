package com.minecraftindustrialcomplex.mic_rtsim_radar;

import com.happysg.radar.block.behavior.networks.NetworkData;
import com.happysg.radar.block.behavior.networks.config.TargetingConfig;
import com.happysg.radar.block.radar.behavior.IRadar;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryApi;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.simibubi.create.content.contraptions.Contraption;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import rbasamoyai.createbigcannons.cannon_control.cannon_mount.CannonMountBlockEntity;
import rbasamoyai.createbigcannons.cannon_control.contraption.AbstractMountedCannonContraption;
import rbasamoyai.createbigcannons.cannon_control.contraption.PitchOrientedContraptionEntity;
import rbasamoyai.createbigcannons.cannons.autocannon.breech.AbstractAutocannonBreechBlockEntity;
import rbasamoyai.createbigcannons.munitions.autocannon.ammo_container.AutocannonAmmoContainerItem;

/**
 * Radar auto-turrets for the defence survey (I21, INDUSTRY.md §13.5: "Create Radars networks whose
 * TargetingConfig has mob, autoTarget and autoFire set, with Create Big Cannons weapons; their ammunition
 * is stock and a consumed good"). A Create Big Cannons mount in a district counts as one turret when all of
 * these hold (choice Q17):
 * <ol>
 *   <li>a Create Radars network ({@code NetworkData}, per level) drives it: the mount is one of the
 *       group's {@code usedWeaponMounts}, or the block a {@code weaponEndpoints} pitch controller faces;</li>
 *   <li>the group's {@code TargetingConfig} has {@code mob}, {@code autoTarget} and {@code autoFire};</li>
 *   <li>its radar is a loaded {@code IRadar} that {@code isRunning()};</li>
 *   <li>the mount carries an assembled cannon ({@code isRunning()} and a live contraption);</li>
 *   <li>the cannon is an autocannon with rounds in its breech: the input buffer plus the magazine's
 *       {@code getTotalAmmoCount}. Big cannons' loaded state is UNVERIFIED, so they are left out.</li>
 * </ol>
 * Each counts 1.0 against every threat class (radars target {@code Enemy} mobs, flyers included): the unit
 * is "one turret", not calibrated; the engine's untested weight and observed fights calibrate it (I22).
 * The rounds' good is the magazine's main ammunition, else the first queued round. Create Radars never
 * checks ammunition itself; CBC Modern Warfare's compact mount is not driven by Create Radars at all.
 */
public final class RadarPlugin implements IndustryPlugin {

    /** What the plugin saw at a mount, for the GameTests and debugging. */
    public record Facts(boolean networked, boolean configured, boolean radarRunning, boolean assembled, long rounds, String ammoGood) {
        public boolean counts() {
            return networked && configured && radarRunning && assembled && rounds > 0;
        }
    }

    @Override
    public void turret(BlockEntity be, TurretSink out) {
        if (!(be instanceof CannonMountBlockEntity) || !(be.getLevel() instanceof ServerLevel level))
            return;
        Facts f = facts(level, be.getBlockPos());
        if (!f.counts())
            return;
        float[] s = new float[IndRecords.ThreatClass.COUNT];
        java.util.Arrays.fill(s, 1f);
        out.add(be.getBlockPos(), s, f.ammoGood, f.rounds);
    }

    /** Reads everything the rule needs at a mount position. */
    public static Facts facts(ServerLevel level, BlockPos mountPos) {
        NetworkData.Group group = null;
        for (NetworkData.Group g : NetworkData.get(level).getGroups().values()) {
            if (g.key.dim() != level.dimension())
                continue;
            if (g.usedWeaponMounts.contains(mountPos) || drivenThroughEndpoint(level, g, mountPos)) {
                group = g;
                break;
            }
        }
        if (group == null)
            return new Facts(false, false, false, false, 0, null);
        TargetingConfig cfg = group.targetingTag == null ? TargetingConfig.DEFAULT : TargetingConfig.fromTag(group.targetingTag);
        boolean configured = cfg.mob() && cfg.autoTarget() && cfg.autoFire();
        boolean radar = group.radarPos != null && level.isLoaded(group.radarPos)
                && level.getBlockEntity(group.radarPos) instanceof IRadar r && r.isRunning();
        boolean assembled = false;
        long rounds = 0;
        String ammo = null;
        if (level.getBlockEntity(mountPos) instanceof CannonMountBlockEntity mount && mount.isRunning() && mount.getContraption() != null) {
            PitchOrientedContraptionEntity e = mount.getContraption();
            assembled = e.isAlive();
            Contraption c = e.getContraption();
            if (assembled && c instanceof AbstractMountedCannonContraption cannon
                    && cannon.presentBlockEntities.get(cannon.getStartPos()) instanceof AbstractAutocannonBreechBlockEntity breech) {
                ItemStack first = null;
                for (ItemStack st : breech.getInputBuffer()) {
                    if (st.isEmpty())
                        continue;
                    rounds += st.getCount();
                    if (first == null)
                        first = st;
                }
                ItemStack mag = breech.getMagazine();
                if (mag != null && !mag.isEmpty()) {
                    rounds += AutocannonAmmoContainerItem.getTotalAmmoCount(mag);
                    ItemStack main = AutocannonAmmoContainerItem.getMainAmmoStack(mag);
                    if (main != null && !main.isEmpty())
                        first = main;
                }
                Industry ind = IndustryApi.current();
                if (first != null && ind != null)
                    ammo = ind.goods().name(first);
            }
        }
        return new Facts(true, configured, radar, assembled, rounds, ammo);
    }

    private static boolean drivenThroughEndpoint(ServerLevel level, NetworkData.Group g, BlockPos mountPos) {
        for (BlockPos ep : g.weaponEndpoints) {
            if (!level.isLoaded(ep))
                continue;
            var state = level.getBlockState(ep);
            Direction facing = state.hasProperty(BlockStateProperties.FACING) ? state.getValue(BlockStateProperties.FACING)
                    : state.hasProperty(BlockStateProperties.HORIZONTAL_FACING) ? state.getValue(BlockStateProperties.HORIZONTAL_FACING) : null;
            if (facing != null && ep.relative(facing).equals(mountPos))
                return true;
        }
        return false;
    }
}
