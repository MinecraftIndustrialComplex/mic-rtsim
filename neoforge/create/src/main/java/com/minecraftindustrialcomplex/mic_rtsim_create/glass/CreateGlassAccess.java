package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import com.simibubi.create.content.kinetics.KineticNetwork;
import com.simibubi.create.content.kinetics.TorquePropagator;
import com.simibubi.create.content.kinetics.deployer.DeployerBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmInteractionPoint;
import net.minecraft.world.level.LevelAccessor;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.List;
import java.util.Map;

/**
 * The glass box's only reads of Create's non-public members (P7, one accessor class for Create; PATCHES.md
 * rows 30 to 33). Read only, at compile time on the server thread, through VarHandles resolved once. A
 * member that is gone (a Create update) makes its getter return null; the compile then models that block
 * entity as a black box instead of failing.
 */
public final class CreateGlassAccess {
    private CreateGlassAccess() {}

    private static final VarHandle NETWORKS = find(TorquePropagator.class, "networks", Map.class, true);
    private static final VarHandle DEPLOYER_MODE = find(DeployerBlockEntity.class, "mode", modeClass(), false);
    private static final VarHandle ARM_INPUTS = find(ArmBlockEntity.class, "inputs", List.class, false);
    private static final VarHandle ARM_OUTPUTS = find(ArmBlockEntity.class, "outputs", List.class, false);

    /** {@code DeployerBlockEntity.Mode} is package-private: its class by name. */
    private static Class<?> modeClass() {
        try {
            return Class.forName("com.simibubi.create.content.kinetics.deployer.DeployerBlockEntity$Mode");
        } catch (ClassNotFoundException | LinkageError e) {
            return null;
        }
    }

    private static VarHandle find(Class<?> owner, String name, Class<?> type, boolean isStatic) {
        if (type == null)
            return null;
        try {
            MethodHandles.Lookup lookup = MethodHandles.privateLookupIn(owner, MethodHandles.lookup());
            return isStatic ? lookup.findStaticVarHandle(owner, name, type) : lookup.findVarHandle(owner, name, type);
        } catch (ReflectiveOperationException | RuntimeException e) {
            return null;
        }
    }

    /** True when every member resolved (the GameTests assert it, so a Create update fails loudly there). */
    public static boolean allResolved() {
        return NETWORKS != null && DEPLOYER_MODE != null && ARM_INPUTS != null && ARM_OUTPUTS != null;
    }

    /**
     * {@code TorquePropagator.networks} (package-private static) for one level, by network id, without
     * {@code getOrCreateNetworkFor}'s side effect of creating an empty network. Row 30.
     */
    @SuppressWarnings("unchecked")
    public static KineticNetwork network(LevelAccessor level, Long id) {
        if (NETWORKS == null || id == null)
            return null;
        Map<LevelAccessor, Map<Long, KineticNetwork>> all = (Map<LevelAccessor, Map<Long, KineticNetwork>>) NETWORKS.get();
        Map<Long, KineticNetwork> m = all == null ? null : all.get(level);
        return m == null ? null : m.get(id);
    }

    /**
     * {@code DeployerBlockEntity.mode} (protected, of a package-private enum): its constant's name, USE or
     * PUNCH, or null. Row 31.
     */
    public static String deployerMode(DeployerBlockEntity be) {
        if (DEPLOYER_MODE == null)
            return null;
        Object m = (Object) DEPLOYER_MODE.get(be);
        return m instanceof Enum<?> e ? e.name() : null;
    }

    /** {@code ArmBlockEntity.inputs} (package-private; null until the arm's first tick). Row 32. */
    @SuppressWarnings("unchecked")
    public static List<ArmInteractionPoint> armInputs(ArmBlockEntity be) {
        return ARM_INPUTS == null ? null : (List<ArmInteractionPoint>) ARM_INPUTS.get(be);
    }

    /** {@code ArmBlockEntity.outputs} (package-private). Row 33. */
    @SuppressWarnings("unchecked")
    public static List<ArmInteractionPoint> armOutputs(ArmBlockEntity be) {
        return ARM_OUTPUTS == null ? null : (List<ArmInteractionPoint>) ARM_OUTPUTS.get(be);
    }
}
