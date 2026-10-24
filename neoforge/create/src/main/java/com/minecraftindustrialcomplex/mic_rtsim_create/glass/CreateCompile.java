package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassExtractor;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GlassRecords;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.GraphBuilder;
import com.minecraftindustrialcomplex.mic_rtsim.industry.glass.Rat;
import com.minecraftindustrialcomplex.mic_rtsim_create.Loaded;
import com.simibubi.create.content.contraptions.AbstractContraptionEntity;
import com.simibubi.create.content.contraptions.actors.psi.PortableStorageInterfaceBlockEntity;
import com.simibubi.create.content.fluids.FluidTransportBehaviour;
import com.simibubi.create.content.fluids.drain.ItemDrainBlockEntity;
import com.simibubi.create.content.fluids.pump.PumpBlock;
import com.simibubi.create.content.fluids.pump.PumpBlockEntity;
import com.simibubi.create.content.fluids.spout.SpoutBlockEntity;
import com.simibubi.create.content.fluids.tank.CreativeFluidTankBlockEntity;
import com.simibubi.create.content.fluids.tank.FluidTankBlockEntity;
import com.simibubi.create.content.kinetics.base.IRotate;
import com.simibubi.create.content.kinetics.base.KineticBlockEntity;
import com.simibubi.create.content.kinetics.belt.BeltBlockEntity;
import com.simibubi.create.content.kinetics.belt.BeltHelper;
import com.simibubi.create.content.kinetics.belt.behaviour.DirectBeltInputBehaviour;
import com.simibubi.create.content.kinetics.belt.behaviour.TransportedItemStackHandlerBehaviour;
import com.simibubi.create.content.kinetics.chainConveyor.ChainConveyorBlockEntity;
import com.simibubi.create.content.kinetics.crafter.MechanicalCrafterBlock;
import com.simibubi.create.content.kinetics.crafter.MechanicalCrafterBlockEntity;
import com.simibubi.create.content.kinetics.crafter.RecipeGridHandler;
import com.simibubi.create.content.kinetics.crusher.CrushingWheelBlockEntity;
import com.simibubi.create.content.kinetics.crusher.CrushingWheelControllerBlock;
import com.simibubi.create.content.kinetics.crusher.CrushingWheelControllerBlockEntity;
import com.simibubi.create.content.kinetics.deployer.DeployerBlockEntity;
import com.simibubi.create.content.kinetics.fan.EncasedFanBlockEntity;
import com.simibubi.create.content.kinetics.fan.processing.AllFanProcessingTypes;
import com.simibubi.create.content.kinetics.fan.processing.FanProcessingType;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmBlockEntity;
import com.simibubi.create.content.kinetics.mechanicalArm.ArmInteractionPoint;
import com.simibubi.create.content.kinetics.millstone.MillstoneBlockEntity;
import com.simibubi.create.content.kinetics.mixer.MechanicalMixerBlockEntity;
import com.simibubi.create.content.kinetics.press.MechanicalPressBlockEntity;
import com.simibubi.create.content.kinetics.saw.SawBlock;
import com.simibubi.create.content.kinetics.saw.SawBlockEntity;
import com.simibubi.create.content.kinetics.steamEngine.PoweredShaftBlockEntity;
import com.simibubi.create.content.kinetics.steamEngine.SteamEngineBlockEntity;
import com.simibubi.create.content.logistics.chute.ChuteBlockEntity;
import com.simibubi.create.content.logistics.chute.SmartChuteBlockEntity;
import com.simibubi.create.content.logistics.crate.CreativeCrateBlockEntity;
import com.simibubi.create.content.logistics.depot.DepotBlockEntity;
import com.simibubi.create.content.logistics.funnel.AbstractFunnelBlock;
import com.simibubi.create.content.logistics.funnel.BeltFunnelBlock;
import com.simibubi.create.content.logistics.funnel.FunnelBlock;
import com.simibubi.create.content.logistics.funnel.FunnelBlockEntity;
import com.simibubi.create.content.logistics.packagePort.PackagePortBlockEntity;
import com.simibubi.create.content.logistics.packager.PackagerBlock;
import com.simibubi.create.content.logistics.packager.PackagerBlockEntity;
import com.simibubi.create.content.logistics.tunnel.BeltTunnelBlockEntity;
import com.simibubi.create.content.logistics.tunnel.BrassTunnelBlockEntity;
import com.simibubi.create.content.processing.basin.BasinBlock;
import com.simibubi.create.content.processing.basin.BasinBlockEntity;
import com.simibubi.create.content.processing.burner.BlazeBurnerBlockEntity;
import com.simibubi.create.content.redstone.thresholdSwitch.ThresholdSwitchBlock;
import com.simibubi.create.content.redstone.thresholdSwitch.ThresholdSwitchBlockEntity;
import com.simibubi.create.foundation.blockEntity.IMultiBlockEntityContainer;
import com.simibubi.create.foundation.blockEntity.SmartBlockEntity;
import com.simibubi.create.foundation.blockEntity.behaviour.BlockEntityBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.filtering.FilteringBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.scrollValue.ScrollOptionBehaviour;
import com.simibubi.create.foundation.blockEntity.behaviour.scrollValue.ScrollValueBehaviour;
import com.simibubi.create.infrastructure.config.AllConfigs;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.ints.IntArrayList;
import it.unimi.dsi.fastutil.ints.IntOpenHashSet;
import it.unimi.dsi.fastutil.longs.Long2IntOpenHashMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.animal.Sheep;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.ShearsItem;
import net.minecraft.world.item.crafting.AbstractCookingRecipe;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.item.crafting.SingleRecipeInput;
import net.minecraft.world.level.GameRules;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.HopperBlock;
import net.minecraft.world.level.block.RedStoneWireBlock;
import net.minecraft.world.level.block.entity.AbstractFurnaceBlockEntity;
import net.minecraft.world.level.block.entity.BlastFurnaceBlockEntity;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.HopperBlockEntity;
import net.minecraft.world.level.block.entity.SmokerBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.capability.IFluidHandler;
import net.neoforged.neoforge.items.IItemHandler;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.function.Predicate;

/**
 * One district's compile for Create 6.0.10 and the vanilla blocks factories use with it (hoppers,
 * furnaces): GLASSBOX.md §1.4 and §2, docs/industry-boundary.md §9.3. {@link #visit} sorts the block
 * entities it models into kinds; {@link #finish} then builds, one step per call while time allows:
 * <ol>
 *   <li>kinetic domains from Create's own networks (levels: one for fixed sources, 19 for a boiler);</li>
 *   <li>a node per component, with where it takes items and fluids in and gives them out;</li>
 *   <li>belts as chains of movers with their stations (presses, deployers, spouts, fan zones), takers
 *       (belt funnels) and splitters (tunnels);</li>
 *   <li>edges: funnels, chutes, arms, hoppers, pipes, belt ends, machine outputs, drops, heat, control;</li>
 *   <li>goods: what can reach each node (seeded from contents and sources), and each machine's
 *       recipes through its own finder (never the recipe JSON);</li>
 *   <li>rate tables per speed level from {@link Cycles} (each machine's own countdown, G-B5);</li>
 *   <li>domain members, edge filters.</li>
 * </ol>
 * Reads the world only; never changes it. Choices are in docs/choices-glassbox-g1.md (GB30 onwards).
 */
final class CreateCompile implements GlassExtractor.Compile {

    static final int NONE = GraphBuilder.NONE;

    enum K {
        MILLSTONE, CRUSHER, PRESS, MIXER, SAW, DEPLOYER, FAN, CRAFTER, SPOUT, DRAIN, BASIN, BELT, FUNNEL, TUNNEL,
        ARM, DEPOT, CHUTE, PUMP, BOILER, ENGINE, BURNER, SWITCH, PACKAGER, FROGPORT, CHAIN, HOPPER, FURNACE,
        CRATE, CREATIVE_TANK, PIPE, KINETIC,
        /** A portable storage interface: claimed with no node (SECTIONS.md §4.4, choice SC95). */
        INTERFACE
    }

    /** How a node's goods change on the way through it (the reach propagation, step 5). */
    private enum Pass { SAME, MACHINE, SOURCE, NONE_OUT }

    /** A craft's batch that is the feeder's (a crusher or saw takes the whole stack it is given). */
    static final int FEEDER_BATCH = -1;

    /** One rate entry family: {@code batch} per {@code ticks(rpm, batch)} for recipe {@code recipe}. */
    interface Cyc {
        /** Ticks per operation at this speed and batch, or null when it never completes. */
        Rat ticks(float rpm, int batch);
    }

    record Craft(int recipe, int batch, Cyc cycle) {}

    /** A sequenced assembly step's recipe: its sequence, its unrolled step (0-based) and the unrolled length. */
    record SeqStep(String id, int step, int total) {}

    /** A belt and a fan processing type: the goods that type made on that belt (never processed again). */
    record FanKey(Object belt, Object type) {}

    final Map<FanKey, IntOpenHashSet> fanDone = new java.util.HashMap<>();

    /** Recipe index to its sequenced step, for the recipes {@code Recipes.stepRecipe} made. */
    final Int2ObjectOpenHashMap<SeqStep> seqSteps = new Int2ObjectOpenHashMap<>();

    /** The compile's view of one of its nodes. */
    static final class Info {
        final K kind;
        final BlockEntity be;
        final GraphBuilder.Node node;
        Pass pass = Pass.SAME;
        int domain = NONE;
        Rat ratio = Rat.ONE;
        boolean kinetic;
        float rpmNow;
        final IntOpenHashSet reach = new IntOpenHashSet();
        final IntOpenHashSet out = new IntOpenHashSet();
        final IntOpenHashSet tried = new IntOpenHashSet();
        final List<Craft> crafts = new ArrayList<>();
        /** A mover's shared transfer: {@code moverBatch} units per {@code mover} ticks. */
        Cyc mover;
        int moverBatch;
        /** Units per operation it hands on (a feeder's batch for the next node), 0 unknown. */
        int unitBatch = 64;
        /** Kind-specific: the host (belt or depot) of a station, a fan's processing type, a heater's level. */
        Object extra;
        /** The kinetic block entity whose domain the node took ({@link #attachKinetic}), or Long.MIN_VALUE. */
        long kinPos = Long.MIN_VALUE;
        /**
         * Ticks a hand-off loop adds to each of its crafts' cycles at level {@code l} (choice GB43), or null:
         * a machine that refills only once empty, from a parked chute.
         */
        java.util.function.IntFunction<Rat> handOff;
        /** A pass-through machine of a collapsed sequenced line: no entry of its own, unlimited (§9.3). */
        boolean noEntry;
        /** A machine whose recipes need every input at once (basin, crafter grid, spout): re-matched on growth. */
        boolean multiInput;
        int multiSeen = -1;
        /** Goods its recipes consume: a pass-through machine does not hand them on. */
        final IntOpenHashSet consumed = new IntOpenHashSet();
        final LongOpenHashSet triedPairs = new LongOpenHashSet();
        final java.util.Set<net.minecraft.resources.ResourceLocation> triedRecipes = new java.util.HashSet<>();
        List<net.minecraft.world.item.crafting.RecipeHolder<? extends net.minecraft.world.item.crafting.Recipe<?>>> candidates;

        Info(K kind, BlockEntity be, GraphBuilder.Node node) {
            this.kind = kind;
            this.be = be;
            this.node = node;
        }
    }

    /** A kinetic block entity's place in a domain. */
    private record Kin(int domain, Rat ratio, float rpm, boolean ratioKnown) {}

    /** An edge's filter, applied to prototypes during propagation and written as GOOD lists at the end. */
    private record EdgeTest(Predicate<ItemStack> items, Predicate<FluidStack> fluids) {}

    final GraphBuilder g;
    final ServerLevel level;
    private final List<BlockEntity> claimed = new ArrayList<>();
    private final Map<K, List<BlockEntity>> byKind = new java.util.EnumMap<>(K.class);
    private int step;

    private final Int2ObjectOpenHashMap<Info> infos = new Int2ObjectOpenHashMap<>();
    private final Long2IntOpenHashMap acceptItem = intMap(), supplyItem = intMap(), acceptFluid = intMap(), supplyFluid = intMap();
    private final Long2ObjectOpenHashMap<Kin> kin = new Long2ObjectOpenHashMap<>();
    private final Long2IntOpenHashMap domainByNetwork = intMap();
    private final List<List<Rat>> levelSpeeds = new ArrayList<>();
    private final LongOpenHashSet claimedPos = new LongOpenHashSet();
    private final Map<GraphBuilder.Edge, EdgeTest> edgeTests = new java.util.IdentityHashMap<>();
    private final Int2ObjectOpenHashMap<ItemStack> itemProto = new Int2ObjectOpenHashMap<>();
    private final Int2ObjectOpenHashMap<FluidStack> fluidProto = new Int2ObjectOpenHashMap<>();
    /** Belt segment or depot position: the chain element items enter there. */
    private final Long2IntOpenHashMap hostEntry = intMap();
    private final Long2ObjectOpenHashMap<Belt> beltBySegment = new Long2ObjectOpenHashMap<>();
    private final List<Belt> belts = new ArrayList<>();
    /** Fan zones: host position to (fan info, processing type). */
    private final Long2ObjectOpenHashMap<List<Object[]>> fanAt = new Long2ObjectOpenHashMap<>();
    private int sink = NONE;
    private int boundedNodes;
    /** Belt movers under an andesite tunnel: their in-edges carry the belt's stack as the entry size. */
    private final IntArrayList andesiteMovers = new IntArrayList();

    private static Long2IntOpenHashMap intMap() {
        Long2IntOpenHashMap m = new Long2IntOpenHashMap();
        m.defaultReturnValue(NONE);
        return m;
    }

    CreateCompile(GraphBuilder g) {
        this.g = g;
        this.level = g.level;
        long chunk = Long.MAX_VALUE;
        for (long ch : g.district.chunks)
            chunk = Math.min(chunk, ch);
        if (chunk != Long.MAX_VALUE)
            g.frame = SableFrames.frame(level, net.minecraft.world.level.ChunkPos.getX(chunk), net.minecraft.world.level.ChunkPos.getZ(chunk));
    }

    // =============================================================================================
    // visit
    // =============================================================================================

    @Override
    public boolean visit(BlockEntity be) {
        K k = classify(be);
        if (k == null)
            return false;
        claimed.add(be);
        claimedPos.add(be.getBlockPos().asLong());
        byKind.computeIfAbsent(k, x -> new ArrayList<>()).add(be);
        if (k == K.BOILER)
            g.claimStore(be.getBlockPos());
        return true;
    }

    /** The kind this compile models a block entity as, or null to leave it to the residual or the stores. */
    private K classify(BlockEntity be) {
        if (be instanceof MillstoneBlockEntity)
            return K.MILLSTONE;
        if (be instanceof CrushingWheelControllerBlockEntity)
            return K.CRUSHER;
        if (be instanceof MechanicalPressBlockEntity)
            return K.PRESS;
        if (be instanceof MechanicalMixerBlockEntity)
            return K.MIXER;
        if (be instanceof SawBlockEntity)
            return K.SAW;
        if (be instanceof DeployerBlockEntity d)
            return deployerModelled(d) ? K.DEPLOYER : null;
        if (be instanceof EncasedFanBlockEntity)
            return K.FAN;
        if (be instanceof MechanicalCrafterBlockEntity)
            return K.CRAFTER;
        if (be instanceof SpoutBlockEntity)
            return K.SPOUT;
        if (be instanceof ItemDrainBlockEntity)
            return K.DRAIN;
        if (be instanceof BasinBlockEntity)
            return K.BASIN;
        if (be instanceof BeltBlockEntity)
            return K.BELT;
        if (be instanceof FunnelBlockEntity)
            return K.FUNNEL;
        if (be instanceof BeltTunnelBlockEntity)
            return K.TUNNEL;
        if (be instanceof ArmBlockEntity a)
            return CreateGlassAccess.armInputs(a) != null && CreateGlassAccess.armOutputs(a) != null ? K.ARM : null;
        if (be instanceof DepotBlockEntity)
            return K.DEPOT;
        if (be instanceof ChuteBlockEntity)
            return K.CHUTE;
        if (be instanceof PumpBlockEntity)
            return K.PUMP;
        if (be instanceof CreativeFluidTankBlockEntity)
            return K.CREATIVE_TANK;
        if (be instanceof FluidTankBlockEntity t) {
            FluidTankBlockEntity c = tankController(t);
            return c != null && c.boiler != null && c.boiler.attachedEngines > 0 ? K.BOILER : null;
        }
        if (be instanceof SteamEngineBlockEntity)
            return K.ENGINE;
        if (be instanceof BlazeBurnerBlockEntity)
            return K.BURNER;
        if (be instanceof ThresholdSwitchBlockEntity)
            return K.SWITCH;
        if (be instanceof PackagerBlockEntity)
            return K.PACKAGER;
        if (be instanceof PackagePortBlockEntity)
            return K.FROGPORT;
        if (be instanceof ChainConveyorBlockEntity)
            return K.CHAIN;
        if (be instanceof CreativeCrateBlockEntity)
            return K.CRATE;
        if (be instanceof HopperBlockEntity)
            return K.HOPPER;
        if (be instanceof AbstractFurnaceBlockEntity)
            return K.FURNACE;
        // A station's interface exposes a docked train's storage, whose change is the station's cargo, booked
        // on the inventories IND_STATION_PORTS names (SECTIONS.md §4.4). Claimed with no node, so the movers
        // beside it have no supply in the graph; left to the residual, it was a black box that left every
        // station-fed unit unbounded (choice SC95)
        if (be instanceof PortableStorageInterfaceBlockEntity)
            return K.INTERFACE;
        if (be instanceof KineticBlockEntity)
            return K.KINETIC;
        if (BlockEntityBehaviour.get(be, FluidTransportBehaviour.TYPE) != null)
            return K.PIPE;
        return null;
    }

    /** A deployer the compile models: one over a belt or depot, or a shearing deployer in front of animals. */
    private boolean deployerModelled(DeployerBlockEntity d) {
        String mode = CreateGlassAccess.deployerMode(d);
        if (mode == null)
            return false;
        Direction facing = d.getBlockState().getValue(BlockStateProperties.FACING);
        BlockPos target = d.getBlockPos().relative(facing, 2);
        if (facing == Direction.DOWN && isHost(d.getBlockPos().below(2)))
            return true;
        return "USE".equals(mode) && held(d).getItem() instanceof ShearsItem
                && !level.getEntitiesOfClass(Sheep.class, new AABB(target)).isEmpty();
    }

    private static ItemStack held(DeployerBlockEntity d) {
        return d.getPlayer() == null ? ItemStack.EMPTY : d.getPlayer().getMainHandItem();
    }

    /** A position a press, deployer or spout above can work on: a belt segment or a depot. */
    private boolean isHost(BlockPos p) {
        BlockEntity be = blockAt(p);
        return be instanceof BeltBlockEntity || be instanceof DepotBlockEntity;
    }

    private List<BlockEntity> all(K k) {
        return byKind.getOrDefault(k, List.of());
    }

    // ---- reads that never load a chunk (SECTIONS.md §2.8, slice S1; choice SC10) ----------------
    // The district's own chunks block-tick (GlassCompiler compiles only then), so their eight
    // neighbours are FULL; but a walk (pipes, chains, dust, fan reach), a network's root and a
    // multiblock's controller can lie further out. Every world read here asks getChunkNow first and
    // reads a chunk that is not loaded as nothing; each such stop is counted in g.unloadedStops.

    /** {@code Level.getBlockEntity} where the chunk is loaded; null (a stop, counted) where it is not. */
    private BlockEntity blockAt(BlockPos p) {
        if (level.isOutsideBuildHeight(p))
            return null;
        net.minecraft.world.level.chunk.LevelChunk c = Loaded.chunk(level, p);
        if (c == null) {
            g.unloadedStops++;
            return null;
        }
        return c.getBlockEntity(p, net.minecraft.world.level.chunk.LevelChunk.EntityCreationType.IMMEDIATE);
    }

    /** A tank's controller through {@link #blockAt} ({@code getControllerBE} reads the level); null where it is not loaded. */
    private FluidTankBlockEntity tankController(FluidTankBlockEntity t) {
        if (t.isController())
            return t;
        BlockPos c = t.getController();
        return c != null && blockAt(c) instanceof FluidTankBlockEntity x ? x : null;
    }

    /** {@code Level.getBlockState} where the chunk is loaded; null (a stop, counted) where it is not. */
    private BlockState stateAt(BlockPos p) {
        BlockState s = Loaded.state(level, p);
        if (s == null)
            g.unloadedStops++;
        return s;
    }

    /** {@code BlockEntityBehaviour.get(level, p, type)} where the chunk is loaded; null where it is not. */
    private <T extends BlockEntityBehaviour> T behaviourAt(BlockPos p, com.simibubi.create.foundation.blockEntity.behaviour.BehaviourType<T> type) {
        BlockEntity be = blockAt(p);
        return be == null ? null : BlockEntityBehaviour.get(be, type);
    }

    /** {@code Level.getCapability(cap, p, context)} where the chunk (and a multiblock's controller) is loaded ({@link Loaded#capability}). */
    private <T, C> T capAt(net.neoforged.neoforge.capabilities.BlockCapability<T, C> cap, BlockPos p, C context) {
        if (!level.isOutsideBuildHeight(p) && !Loaded.at(level, p)) {
            g.unloadedStops++;
            return null;
        }
        return Loaded.capability(level, cap, p, context);
    }

    /** Whether every chunk within {@code r} blocks of {@code p} (horizontally) is loaded: a read that looks around {@code p}. */
    private boolean loadedAround(BlockPos p, int r) {
        for (int dx = -r; dx <= r; dx += r == 0 ? 1 : r)
            for (int dz = -r; dz <= r; dz += r == 0 ? 1 : r)
                if (!Loaded.at(level, p.offset(dx, 0, dz))) {
                    g.unloadedStops++;
                    return false;
                }
        return true;
    }

    // =============================================================================================
    // finish
    // =============================================================================================

    @Override
    public boolean finish(long deadlineNanos) {
        while (step < 7) {
            switch (step) {
                case 0 -> kinetics();
                case 1 -> nodes();
                case 2 -> beltChains();
                case 3 -> edges();
                case 4 -> goods();
                case 5 -> rates();
                case 6 -> finishUp();
                default -> {}
            }
            step++;
            if (System.nanoTime() > deadlineNanos && step < 7)
                return false;
        }
        return true;
    }

    // ---- step 0: kinetic domains ----------------------------------------------------------------

    private void kinetics() {
        // clutches, gearshifts, smart chutes and funnels a modelled switch powers: compiled unpowered,
        // the switch's CONTROL edge gates them (GB51)
        for (BlockEntity be : all(K.SWITCH))
            switchDriven.addAll(poweredBy(be.getBlockPos(), ThresholdSwitchBlock.getTargetDirection(be.getBlockState())));
        boolean stressEnabled = IRotate.StressImpact.isEnabled();
        List<KineticBlockEntity> ks = new ArrayList<>();
        for (BlockEntity be : claimed)
            if (be instanceof KineticBlockEntity k)
                ks.add(k);
        Long2ObjectOpenHashMap<com.simibubi.create.content.kinetics.KineticNetwork> networks = new Long2ObjectOpenHashMap<>();
        for (KineticBlockEntity k : ks) {
            Long id = k.network;
            if (id == null || networks.containsKey(id.longValue()))
                continue;
            com.simibubi.create.content.kinetics.KineticNetwork net = CreateGlassAccess.network(level, id);
            if (net != null)
                networks.put(id.longValue(), net);
        }
        // Boilers driving each network: engines' shafts that are sources of it.
        Long2ObjectOpenHashMap<List<SteamEngineBlockEntity>> enginesByNet = new Long2ObjectOpenHashMap<>();
        for (BlockEntity be : all(K.ENGINE)) {
            SteamEngineBlockEntity e = (SteamEngineBlockEntity) be;
            PoweredShaftBlockEntity shaft = e.getShaft();
            if (shaft != null && shaft.network != null)
                enginesByNet.computeIfAbsent(shaft.network.longValue(), x -> new ArrayList<>()).add(e);
        }
        long[] ids = networks.keySet().toLongArray();
        java.util.Arrays.sort(ids);
        for (long id : ids) {
            var net = networks.get(id);
            GraphBuilder.Domain d = g.domain(id);
            domainByNetwork.put(id, d.index);
            BlockPos rootPos = BlockPos.of(id);
            BlockEntity rootBe = blockAt(rootPos);
            KineticBlockEntity root = rootBe instanceof KineticBlockEntity rk ? rk : null;
            if (root == null && !Loaded.at(level, rootPos))
                for (KineticBlockEntity s : net.sources.keySet())
                    if (s.getBlockPos().equals(rootPos))
                        root = s; // the root's chunk is not loaded: the network keeps it as Create last saw it
            float rootSpeed = root == null ? referenceSpeed(net) : root.getGeneratedSpeed();
            if (rootSpeed == 0 && root != null)
                rootSpeed = root.getTheoreticalSpeed();
            boolean overstressed = false;
            // a network whose members lie outside the district's chunks (another district's, or no
            // district's): until SEC-1's slice S5 models shared networks, an unstable domain here and in
            // every other district on it (choice SC11), the boiler's district included. Members whose chunk
            // has not loaded this session are not listed, so the test is structural (choice SC44): a listed
            // member outside, or a link out of the district from a listed member inside (reachesOut). The
            // chunks outside it reaches are kept for edits there (choice SC77).
            boolean crossing = false;
            float capacity = 0, stress = 0;
            for (var e : net.sources.entrySet()) {
                KineticBlockEntity s = e.getKey();
                if (listed(net, s)) {
                    capacity += e.getValue() * Math.abs(s.getGeneratedSpeed());
                    crossing |= reached(s.getBlockPos());
                }
            }
            for (var e : net.members.entrySet()) {
                KineticBlockEntity m = e.getKey();
                if (listed(net, m)) {
                    stress += e.getValue() * Math.abs(m.getTheoreticalSpeed());
                    overstressed |= m.isOverStressed();
                    crossing |= reached(m.getBlockPos());
                }
            }
            crossing |= reachesOut(id, net);
            d.liveCapacity = capacity;
            d.liveStress = stress;
            if (overstressed)
                d.flags |= GlassRecords.Domain.OVERSTRESSED;
            if (!stressEnabled)
                d.flags |= GlassRecords.Domain.STRESS_DISABLED;
            List<Rat> speeds = new ArrayList<>();
            List<SteamEngineBlockEntity> engines = enginesByNet.get(id);
            FluidTankBlockEntity boiler = engines == null ? null : singleBoiler(engines);
            if (!crossing && boiler != null && root instanceof PoweredShaftBlockEntity rootShaft && rootShaft.capacityKey != null) {
                // A boiler drives the domain: tabulate every level 0..18 (G-B5, §9.3 "Boiler").
                int engineCount = boiler.boiler.attachedEngines;
                double otherCapacity = 0;
                for (var e : net.sources.entrySet())
                    if (!(e.getKey() instanceof PoweredShaftBlockEntity))
                        otherCapacity += e.getValue() * Math.abs(e.getKey().getGeneratedSpeed());
                int dir = rootShaft.movementDirection;
                for (int l = 0; l <= 18; l++) {
                    float eff = Cycles.boilerEfficiency(l, engineCount, false);
                    float sp = eff > 0 ? dir * Cycles.engineSpeed(eff) : 0;
                    double cap = otherCapacity;
                    for (var e : net.sources.entrySet())
                        if (e.getKey() instanceof PoweredShaftBlockEntity ps && ps.capacityKey != null)
                            cap += Cycles.engineCapacityPerRpm(eff, com.simibubi.create.api.stress.BlockStressValues.getCapacity(ps.capacityKey))
                                    * Math.abs(sp);
                    Rat speed = Rat.ofFloat(sp);
                    d.levels.add(new GraphBuilder.Level(l, speed, cap));
                    speeds.add(speed);
                }
                int current = Cycles.boilerLevel(boiler.boiler.activeHeat, boiler.getTotalTankSize(), boiler.boiler.waterSupply);
                d.current = Math.max(0, Math.min(18, current));
                // the driver (the boiler node) is set when the node exists (step 1)
                d.driver = NONE;
                boilerDomains.put(boiler.getBlockPos().asLong(), d.index);
                if (rootSpeed == 0)
                    rootSpeed = Cycles.engineSpeed(1f) * dir; // ratios are relative to the root at any level
            } else {
                Rat speed = Rat.ofFloat(rootSpeed);
                d.levels.add(new GraphBuilder.Level(0, speed, capacity));
                d.current = 0;
                speeds.add(speed);
                if ((engines != null && boiler == null) || crossing)
                    unstableDomains.add(d.index); // engines of several boilers, or a shared network: the current level only
            }
            levelSpeeds.add(speeds);
            domainRoot.put(d.index, rootSpeed);
            for (var e : net.members.entrySet()) {
                KineticBlockEntity m = e.getKey();
                if (!listed(net, m))
                    continue; // replaced in a loaded chunk by a part that joined, or by another block: Create's next recount drops it
                float sp = m.getTheoreticalSpeed();
                // a stopped network (a cold boiler) gives every member speed 0: the ratio is unknown then
                boolean known = rootSpeed != 0 && sp != 0;
                Rat ratio = known ? Rat.ofFloat(sp).div(Rat.ofFloat(rootSpeed)).fitSigned() : Rat.ONE;
                kin.put(m.getBlockPos().asLong(), new Kin(d.index, ratio, Math.abs(m.getSpeed()), known));
                memberImpact.put(m.getBlockPos().asLong(), (double) e.getValue());
            }
        }
        walk();
    }

    /**
     * Whether a network still lists {@code k} as Create's guarded recount does (PATCHES.md row 53): its
     * chunk is not loaded (kept as Create last saw it), it is the block entity there now, or the one there now
     * is a new object of the same network that has not joined yet ({@link Loaded#keptFor}).
     */
    private boolean listed(com.simibubi.create.content.kinetics.KineticNetwork net, KineticBlockEntity k) {
        BlockPos p = k.getBlockPos();
        if (!Loaded.at(level, p))
            return true;
        BlockEntity found = Loaded.blockEntity(level, p);
        return found == k || Loaded.keptFor(net, k, found);
    }

    /**
     * The speed a domain's ratios are relative to when its root is neither loaded nor listed (its chunk has
     * not loaded this session, so Create holds no block entity for it, choice SC44): the theoretical speed
     * of the listed member in a loaded chunk with the lowest position that turns, else 0 (the ratios are
     * then unknown and the members bounded).
     */
    private float referenceSpeed(com.simibubi.create.content.kinetics.KineticNetwork net) {
        long best = Long.MAX_VALUE;
        float speed = 0;
        for (KineticBlockEntity m : net.members.keySet()) {
            BlockPos p = m.getBlockPos();
            if (!Loaded.at(level, p) || !listed(net, m) || m.getTheoreticalSpeed() == 0)
                continue;
            if (p.asLong() < best) {
                best = p.asLong();
                speed = m.getTheoreticalSpeed();
            }
        }
        return speed;
    }

    /**
     * Whether a listed member of {@code net} inside the district links to a chunk outside it (choice SC44):
     * through a face across the district's border or one of its own additions ({@code addPropagationLocations}:
     * a chain conveyor's connections, a large cog's diagonals), to a position whose chunk is not loaded (it
     * may hold members that never loaded this session) or to a kinetic block entity of the same network.
     * Each chunk it links into is kept (choice SC77), and so is each loaded chunk outside the district across
     * a face that any listed member, inside or out, may turn, though nothing is there yet: a part a player
     * places there joins the network (review M3). Reads only loaded chunks.
     */
    private boolean reachesOut(long id, com.simibubi.create.content.kinetics.KineticNetwork net) {
        boolean out = false;
        for (KineticBlockEntity m : net.members.keySet()) {
            if (!listed(net, m) || !Loaded.at(level, m.getBlockPos()))
                continue;
            BlockPos p = m.getBlockPos();
            boolean in = !outside(m);
            int lx = p.getX() & 15, lz = p.getZ() & 15;
            if (!in && lx != 0 && lx != 15 && lz != 0 && lz != 15)
                continue; // outside and off its chunk's border: no face leaves its chunk
            BlockState state = m.getBlockState();
            IRotate block = state.getBlock() instanceof IRotate r ? r : null;
            for (Direction f : Direction.values()) {
                if (f.getAxis() == Direction.Axis.Y)
                    continue;
                BlockPos n = p.relative(f);
                boolean may = block != null && connects(block, p, state, f);
                if (in && linksOutTo(id, n, may))
                    out |= reached(n);
                else if (may && ((n.getX() >> 4) != (p.getX() >> 4) || (n.getZ() >> 4) != (p.getZ() >> 4)) && Loaded.at(level, n))
                    reached(n);
            }
            if (in && block != null)
                for (BlockPos n : m.addPropagationLocations(block, state, new ArrayList<>()))
                    if (linksOutTo(id, n, true))
                        out |= reached(n);
        }
        return out;
    }

    /**
     * Whether {@code p} lies outside the district's chunks; if so its chunk is kept as one a network of the
     * compile reaches (choice SC77: a player's edit there, in no district, is this district's too).
     */
    private boolean reached(BlockPos p) {
        long c = net.minecraft.world.level.ChunkPos.asLong(p.getX() >> 4, p.getZ() >> 4);
        if (g.district.chunks.contains(c))
            return false;
        g.reachedChunks.add(c);
        return true;
    }

    /**
     * Whether position {@code n} outside the district holds, or may hold, a member of network {@code id}:
     * a kinetic block entity of the network, or a chunk that is not loaded where {@code mayConnect} (the
     * member links that way at all).
     */
    private boolean linksOutTo(long id, BlockPos n, boolean mayConnect) {
        if (!g.district.chunks.contains(net.minecraft.world.level.ChunkPos.asLong(n.getX() >> 4, n.getZ() >> 4)))
            return (mayConnect && !Loaded.at(level, n))
                    || (Loaded.blockEntity(level, n) instanceof KineticBlockEntity o && o.network != null && o.network == id);
        return false;
    }

    /**
     * Whether a kinetic block may turn its neighbour across face {@code f}. Create's own shaft and cog
     * (those classes exactly) do only through a shaft end that way or, for a cog, in its plane; any other
     * block may (a chain drive links across its sides, which no shaft end says, and an addon's shaft may
     * link sideways), so it counts, on the safe side. Reads the block's own state only.
     */
    private boolean connects(IRotate block, BlockPos p, BlockState state, Direction f) {
        // Create's own shaft and cog only: an addon's subclass may link sideways
        Class<?> c = block.getClass();
        if (c != com.simibubi.create.content.kinetics.simpleRelays.ShaftBlock.class
                && c != com.simibubi.create.content.kinetics.simpleRelays.CogWheelBlock.class)
            return true;
        if (block.hasShaftTowards(level, p, state, f))
            return true;
        return (com.simibubi.create.content.kinetics.simpleRelays.ICogWheel.isSmallCog(state)
                || com.simibubi.create.content.kinetics.simpleRelays.ICogWheel.isLargeCog(state))
                && block.getRotationAxis(state) != f.getAxis();
    }

    /** Whether a block entity lies outside the district's chunks. */
    private boolean outside(BlockEntity be) {
        BlockPos p = be.getBlockPos();
        return !g.district.chunks.contains(net.minecraft.world.level.ChunkPos.asLong(p.getX() >> 4, p.getZ() >> 4));
    }

    /** Positions a modelled switch powers (its neighbours but its target, and along dust). */
    private final LongOpenHashSet switchDriven = new LongOpenHashSet();
    /** Block entities the walk put in a domain (not in a live network at compile), with their signed speed. */
    private final it.unimi.dsi.fastutil.longs.Long2FloatOpenHashMap walkedSpeed = new it.unimi.dsi.fastutil.longs.Long2FloatOpenHashMap();
    /** Each domain's root speed, the one its members' ratios are relative to. */
    private final it.unimi.dsi.fastutil.ints.Int2FloatOpenHashMap domainRoot = new it.unimi.dsi.fastutil.ints.Int2FloatOpenHashMap();
    /** Per switch-driven clutch: the district's kinetic block entities it turns (empty: something else drives them). */
    private final Long2ObjectOpenHashMap<LongOpenHashSet> clutchTurns = new Long2ObjectOpenHashMap<>();
    /** The side the walk entered each block entity it put in a domain from: its source on the running line. */
    private final Long2ObjectOpenHashMap<Direction> walkSource = new Long2ObjectOpenHashMap<>();

    /**
     * Puts the district's kinetic block entities that are in no live network into the domain of a
     * connected one (choice GB51): past a clutch a modelled switch drives (engaged: the running line), and
     * block entities that have not joined their network yet (they do on their first tick). Their ratio is
     * the product of Create's speed factors along the walk ({@link KineticWalk}), so the compile is the same
     * with the clutch on or off and before or after the first tick. The walk stays in the district, never
     * enters a generator or a speed controller (another drive, or a speed set rather than conveyed), and
     * stops at a link Create would not turn (a powered clutch no switch drives).
     */
    private void walk() {
        java.util.ArrayDeque<KineticBlockEntity> queue = new java.util.ArrayDeque<>();
        for (BlockEntity be : claimed)
            if (be instanceof KineticBlockEntity k && kin.containsKey(k.getBlockPos().asLong()))
                queue.add(k);
        int steps = 0;
        while (!queue.isEmpty() && steps++ < 65536) {
            KineticBlockEntity k = queue.poll();
            long kp = k.getBlockPos().asLong();
            Kin kk = kin.get(kp);
            Direction src = walkSource.containsKey(kp) ? walkSource.get(kp) : KineticWalk.liveSource(k);
            for (BlockPos q : KineticWalk.neighbours(k)) {
                long qp = q.asLong();
                if (!claimedPos.contains(qp) || kin.containsKey(qp))
                    continue;
                if (!(blockAt(q) instanceof KineticBlockEntity n) || n.isRemoved() || n.getGeneratedSpeed() != 0
                        || n instanceof com.simibubi.create.content.kinetics.speedController.SpeedControllerBlockEntity)
                    continue;
                float m = KineticWalk.modifier(k, src, n, switchDriven::contains);
                if (m == 0 || Float.isNaN(m))
                    continue;
                Rat ratio = kk.ratio().mul(Rat.ofFloat(m)).fitSigned();
                float speed = (float) (ratio.toDouble() * domainRoot.get(kk.domain()));
                kin.put(qp, new Kin(kk.domain(), ratio, Math.abs(speed), kk.ratioKnown()));
                walkedSpeed.put(qp, speed);
                walkSource.put(qp, KineticWalk.sideToward(q, k.getBlockPos()));
                memberImpact.put(qp, com.simibubi.create.api.stress.BlockStressValues.getImpact(n.getBlockState().getBlock()));
                queue.add(n);
            }
        }
    }

    /**
     * What a switch-driven clutch turns (choice GB51): the district's kinetic block entities connected to its
     * side away from its source, walked as {@link #walk} does whether the clutch is engaged or not. Empty when
     * the walk meets a generator (the part has another drive, so the clutch alone does not stop it) or the
     * clutch has no source.
     */
    private LongOpenHashSet turnedBy(KineticBlockEntity clutch) {
        LongOpenHashSet out = clutchTurns.get(clutch.getBlockPos().asLong());
        if (out != null)
            return out;
        out = new LongOpenHashSet();
        clutchTurns.put(clutch.getBlockPos().asLong(), out);
        // the walk's source when it put the clutch in a domain (it had not joined its network at compile)
        long cp = clutch.getBlockPos().asLong();
        Direction src = walkSource.containsKey(cp) ? walkSource.get(cp) : KineticWalk.liveSource(clutch);
        if (src == null)
            return out;
        BlockPos upstream = clutch.getBlockPos().relative(src);
        java.util.ArrayDeque<KineticBlockEntity> queue = new java.util.ArrayDeque<>();
        Long2ObjectOpenHashMap<Direction> srcOf = new Long2ObjectOpenHashMap<>();
        LongOpenHashSet seen = new LongOpenHashSet();
        seen.add(clutch.getBlockPos().asLong());
        srcOf.put(clutch.getBlockPos().asLong(), src);
        queue.add(clutch);
        int steps = 0;
        while (!queue.isEmpty() && steps++ < 65536) {
            KineticBlockEntity k = queue.poll();
            for (BlockPos q : KineticWalk.neighbours(k)) {
                long qp = q.asLong();
                if ((k == clutch && q.equals(upstream)) || !claimedPos.contains(qp) || seen.contains(qp))
                    continue;
                if (!(blockAt(q) instanceof KineticBlockEntity n) || n.isRemoved())
                    continue;
                float m = KineticWalk.modifier(k, srcOf.get(k.getBlockPos().asLong()), n, switchDriven::contains);
                if (m == 0 || Float.isNaN(m))
                    continue;
                if (n.getGeneratedSpeed() != 0) {
                    out.clear();
                    return out;
                }
                seen.add(qp);
                out.add(qp);
                srcOf.put(qp, KineticWalk.sideToward(q, k.getBlockPos()));
                queue.add(n);
            }
        }
        return out;
    }

    /** A kinetic block entity's signed speed: the walk's when it placed it, else Create's theoretical speed. */
    float signedSpeed(KineticBlockEntity k) {
        long p = k.getBlockPos().asLong();
        return walkedSpeed.containsKey(p) ? walkedSpeed.get(p) : k.getTheoreticalSpeed();
    }

    private final Long2IntOpenHashMap boilerDomains = intMap();
    private final IntOpenHashSet unstableDomains = new IntOpenHashSet();
    private final it.unimi.dsi.fastutil.longs.Long2DoubleOpenHashMap memberImpact = new it.unimi.dsi.fastutil.longs.Long2DoubleOpenHashMap();

    private static FluidTankBlockEntity singleBoiler(List<SteamEngineBlockEntity> engines) {
        FluidTankBlockEntity found = null;
        for (SteamEngineBlockEntity e : engines) {
            FluidTankBlockEntity t = e.getTank();
            if (t == null)
                return null;
            if (found != null && found != t)
                return null;
            found = t;
        }
        return found;
    }

    /** Attaches a node to the kinetic domain of the block entity at {@code pos}. */
    private void attachKinetic(Info info, BlockPos pos) {
        Kin k = kin.get(pos.asLong());
        info.kinetic = true;
        info.kinPos = pos.asLong();
        if (k == null) {
            info.rpmNow = 0;
            return;
        }
        info.domain = k.domain();
        info.ratio = k.ratio();
        info.rpmNow = k.rpm();
        info.node.domain(k.domain());
        if (!k.ratioKnown() || unstableDomains.contains(k.domain()))
            bounded(info.node);
    }

    private void bounded(GraphBuilder.Node n) {
        if ((n.flags & GlassRecords.Node.BOUNDED) == 0)
            boundedNodes++;
        n.flag(GlassRecords.Node.BOUNDED);
    }

    // ---- step 1: nodes --------------------------------------------------------------------------

    private Info info(K kind, BlockEntity be, int nodeKind, int model) {
        return info(kind, be, nodeKind, model, be.getBlockPos());
    }

    private Info info(K kind, BlockEntity be, int nodeKind, int model, BlockPos at) {
        GraphBuilder.Node n = g.node(nodeKind, model, at);
        Info i = new Info(kind, be, n);
        infos.put(n.index, i);
        return i;
    }

    private void accepts(BlockPos p, Info i) {
        acceptItem.put(p.asLong(), i.node.index);
    }

    private void supplies(BlockPos p, Info i) {
        supplyItem.put(p.asLong(), i.node.index);
    }

    private void nodes() {
        for (BlockEntity be : all(K.MILLSTONE)) {
            Info i = info(K.MILLSTONE, be, GlassRecords.Node.MACHINE, GlassRecords.Model.MILLSTONE);
            i.pass = Pass.MACHINE;
            i.node.flag(GlassRecords.Node.ANY_OUTPUT_BLOCKS);
            attachKinetic(i, be.getBlockPos());
            accepts(be.getBlockPos(), i);
            supplies(be.getBlockPos(), i);
        }
        for (BlockEntity be : all(K.CRUSHER)) {
            Info i = info(K.CRUSHER, be, GlassRecords.Node.MACHINE, GlassRecords.Model.CRUSHER);
            i.pass = Pass.MACHINE;
            for (Direction d : Direction.Plane.HORIZONTAL) {
                BlockPos w = be.getBlockPos().relative(d);
                if (blockAt(w) instanceof CrushingWheelBlockEntity) {
                    attachKinetic(i, w);
                    break;
                }
            }
            accepts(be.getBlockPos(), i);
            supplies(be.getBlockPos(), i);
        }
        for (BlockEntity be : all(K.BASIN))
            basinNode((BasinBlockEntity) be);
        for (BlockEntity be : all(K.DEPOT)) {
            Info i = info(K.DEPOT, be, GlassRecords.Node.BUFFER, GlassRecords.Model.DEPOT);
            accepts(be.getBlockPos(), i);
            supplies(be.getBlockPos(), i);
            hostEntry.put(be.getBlockPos().asLong(), i.node.index);
        }
        for (BlockEntity be : all(K.SAW)) {
            SawBlockEntity s = (SawBlockEntity) be;
            if (s.getBlockState().getValue(BlockStateProperties.FACING) != Direction.UP)
                continue; // a tree cutter: kinetic only
            Info i = info(K.SAW, be, GlassRecords.Node.MACHINE, GlassRecords.Model.SAW);
            i.pass = Pass.MACHINE;
            i.node.flag(GlassRecords.Node.PASS_THROUGH);
            attachKinetic(i, be.getBlockPos());
            accepts(be.getBlockPos(), i);
            supplies(be.getBlockPos(), i);
        }
        for (BlockEntity be : all(K.FAN))
            fanZones((EncasedFanBlockEntity) be);
        crafterGrids();
        for (BlockEntity be : all(K.BURNER))
            burnerNode((BlazeBurnerBlockEntity) be);
        for (BlockEntity be : all(K.BOILER)) {
            FluidTankBlockEntity t = (FluidTankBlockEntity) be;
            if (!t.isController())
                continue;
            boilerNode(t);
        }
        for (BlockEntity be : all(K.BOILER)) {
            FluidTankBlockEntity t = (FluidTankBlockEntity) be;
            int b = acceptFluid.get(t.getController().asLong());
            if (b != NONE)
                acceptFluid.put(t.getBlockPos().asLong(), b);
        }
        for (BlockEntity be : all(K.FUNNEL))
            funnelNode((FunnelBlockEntity) be);
        for (BlockEntity be : all(K.CHUTE))
            chuteNode((ChuteBlockEntity) be);
        for (BlockEntity be : all(K.ARM)) {
            Info i = info(K.ARM, be, GlassRecords.Node.MOVER, GlassRecords.Model.ARM);
            attachKinetic(i, be.getBlockPos());
        }
        for (BlockEntity be : all(K.HOPPER)) {
            Info i = info(K.HOPPER, be, GlassRecords.Node.MOVER, GlassRecords.Model.HOPPER);
            boolean enabled = be.getBlockState().getValue(HopperBlock.ENABLED);
            i.moverBatch = enabled ? 1 : 0;
            i.mover = (rpm, b) -> Rat.of(8, 1);
            i.unitBatch = 1;
            accepts(be.getBlockPos(), i);
        }
        for (BlockEntity be : all(K.FURNACE)) {
            Info i = info(K.FURNACE, be, GlassRecords.Node.MACHINE, GlassRecords.Model.TIMED);
            i.pass = Pass.MACHINE;
            accepts(be.getBlockPos(), i);
            supplies(be.getBlockPos(), i);
        }
        for (BlockEntity be : all(K.PUMP)) {
            Info i = info(K.PUMP, be, GlassRecords.Node.MOVER, GlassRecords.Model.PUMP);
            i.node.flag(GlassRecords.Node.FLUID);
            attachKinetic(i, be.getBlockPos());
        }
        for (BlockEntity be : all(K.CRATE)) {
            Info i = info(K.CRATE, be, GlassRecords.Node.SOURCE, GlassRecords.Model.UNKNOWN);
            i.pass = Pass.SOURCE;
            FilteringBehaviour f = BlockEntityBehaviour.get(be, FilteringBehaviour.TYPE);
            ItemStack item = f == null ? ItemStack.EMPTY : f.getFilter();
            if (!item.isEmpty()) {
                int good = good(item);
                GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.OTHER, GlassRecords.Heat.NONE, 0, g.name("create:creative_crate"));
                r.out(good, 1, 1);
                i.crafts.add(new Craft(g.recipe(r), 0, null));
                i.out.add(good);
            }
            supplies(be.getBlockPos(), i);
        }
        for (BlockEntity be : all(K.CREATIVE_TANK)) {
            FluidTankBlockEntity t = (FluidTankBlockEntity) be;
            if (!t.isController())
                continue;
            Info i = info(K.CREATIVE_TANK, be, GlassRecords.Node.SOURCE, GlassRecords.Model.UNKNOWN);
            i.pass = Pass.SOURCE;
            i.node.flag(GlassRecords.Node.FLUID);
            FluidStack fs = t.getTankInventory().getFluid();
            if (!fs.isEmpty()) {
                int good = good(fs);
                GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.OTHER, GlassRecords.Heat.NONE, 0, g.name("create:creative_fluid_tank"));
                r.out(good, 1, 1);
                i.crafts.add(new Craft(g.recipe(r), 0, null));
                i.out.add(good);
            }
            supplyFluid.put(t.getBlockPos().asLong(), i.node.index);
        }
        for (BlockEntity be : all(K.CREATIVE_TANK)) {
            FluidTankBlockEntity t = (FluidTankBlockEntity) be;
            int s = supplyFluid.get(t.getController().asLong());
            if (s != NONE) {
                supplyFluid.put(t.getBlockPos().asLong(), s);
                acceptFluid.put(t.getBlockPos().asLong(), s);
            }
        }
        for (BlockEntity be : all(K.DRAIN)) {
            Info i = info(K.DRAIN, be, GlassRecords.Node.MACHINE, GlassRecords.Model.DRAIN);
            i.pass = Pass.MACHINE;
            i.node.flag(GlassRecords.Node.PASS_THROUGH);
            accepts(be.getBlockPos(), i);
            supplyFluid.put(be.getBlockPos().asLong(), i.node.index);
            i.crafts.size();
        }
        packageLogistics();
        for (BlockEntity be : all(K.SWITCH))
            info(K.SWITCH, be, GlassRecords.Node.CONTROLLER, GlassRecords.Model.THRESHOLD_SWITCH);
        for (BlockEntity be : all(K.DEPLOYER))
            deployerSheep((DeployerBlockEntity) be);
    }

    private void basinNode(BasinBlockEntity basin) {
        BlockPos p = basin.getBlockPos();
        BlockEntity op = blockAt(p.above(2));
        boolean mixer = op instanceof MechanicalMixerBlockEntity && claimedPos.contains(op.getBlockPos().asLong());
        boolean press = op instanceof MechanicalPressBlockEntity && claimedPos.contains(op.getBlockPos().asLong());
        Info i;
        if (mixer || press) {
            i = info(mixer ? K.MIXER : K.PRESS, op, GlassRecords.Node.MACHINE, mixer ? GlassRecords.Model.MIXER : GlassRecords.Model.PRESS, p);
            i.pass = Pass.MACHINE;
            i.multiInput = true;
            i.extra = basin;
            attachKinetic(i, op.getBlockPos());
            operatorAtBasin.add(op.getBlockPos().asLong());
        } else {
            i = info(K.BASIN, basin, GlassRecords.Node.BUFFER, GlassRecords.Model.BASIN);
            i.extra = basin;
        }
        accepts(p, i);
        supplies(p, i);
        acceptFluid.put(p.asLong(), i.node.index);
        supplyFluid.put(p.asLong(), i.node.index);
    }

    private final LongOpenHashSet operatorAtBasin = new LongOpenHashSet();

    private void burnerNode(BlazeBurnerBlockEntity b) {
        Info i = info(K.BURNER, b, GlassRecords.Node.HEATER, GlassRecords.Model.BLAZE_BURNER);
        i.pass = Pass.NONE_OUT;
        boolean special = b.getActiveFuel() == BlazeBurnerBlockEntity.FuelType.SPECIAL;
        int heat = special ? GlassRecords.Heat.SEETHING : GlassRecords.Heat.KINDLED;
        // burn ticks per fuel item: 0 never runs out (creative); else the shortest of the fuels that reach
        // it (Recipes.fuel), 1 while none does (no fuel is accepted, so its duty is 0 whatever it is)
        long burn = b.isCreative() ? 0 : 1;
        i.node.params(b.isCreative() ? Integer.MAX_VALUE : b.getRemainingBurnTime(), b.isCreative() ? heatOf(b) : heat, burn, BlazeBurnerBlockEntity.INSERTION_THRESHOLD);
        i.extra = b.isCreative() ? heatOf(b) : heat;
        accepts(b.getBlockPos(), i);
    }

    private static int heatOf(BlazeBurnerBlockEntity b) {
        return switch (b.getHeatLevelFromBlock()) {
            case NONE -> GlassRecords.Heat.NONE;
            case SMOULDERING -> GlassRecords.Heat.SMOULDERING;
            case FADING -> GlassRecords.Heat.FADING;
            case KINDLED -> GlassRecords.Heat.KINDLED;
            case SEETHING -> GlassRecords.Heat.SEETHING;
        };
    }

    private void boilerNode(FluidTankBlockEntity t) {
        Info i = info(K.BOILER, t, GlassRecords.Node.BOILER, GlassRecords.Model.BOILER);
        i.pass = Pass.NONE_OUT;
        i.node.flag(GlassRecords.Node.FLUID);
        var b = t.boiler;
        i.node.params(t.getTotalTankSize(), b.attachedEngines, b.activeHeat, Math.round((double) b.waterSupply * 1000));
        acceptFluid.put(t.getBlockPos().asLong(), i.node.index);
        int d = boilerDomains.get(t.getBlockPos().asLong());
        if (d != NONE)
            g.domains().get(d).driver = i.node.index;
    }

    private void funnelNode(FunnelBlockEntity f) {
        BlockState s = f.getBlockState();
        Direction facing = AbstractFunnelBlock.getFunnelFacing(s);
        if (facing == null)
            return;
        boolean brass = s.getBlock() == com.simibubi.create.AllBlocks.BRASS_FUNNEL.get()
                || s.getBlock() == com.simibubi.create.AllBlocks.BRASS_BELT_FUNNEL.get();
        int model = brass ? GlassRecords.Model.FUNNEL_BRASS : GlassRecords.Model.FUNNEL_ANDESITE;
        boolean powered = s.hasProperty(AbstractFunnelBlock.POWERED) && redstonePowered(f.getBlockPos());
        FilteringBehaviour filter = BlockEntityBehaviour.get(f, FilteringBehaviour.TYPE);
        boolean beltFunnel = s.getBlock() instanceof BeltFunnelBlock;
        boolean extracting;
        if (beltFunnel)
            extracting = s.getValue(BeltFunnelBlock.SHAPE) == BeltFunnelBlock.Shape.PUSHING;
        else
            extracting = s.getBlock() instanceof FunnelBlock && s.getValue(FunnelBlock.EXTRACTING);
        if (beltFunnel && !extracting && s.getValue(BeltFunnelBlock.SHAPE) != BeltFunnelBlock.Shape.PULLING)
            return; // retracted or extended: a belt funnel that neither pulls nor pushes
        Info i;
        if (extracting) {
            i = info(K.FUNNEL, f, GlassRecords.Node.MOVER, model);
            int amount = funnelAmount(f, s, filter, brass);
            i.moverBatch = powered ? 0 : amount;
            int period = Cycles.funnelPeriod(AllConfigs.server().logistics.defaultExtractionTimer.get());
            i.mover = (rpm, b) -> Rat.of(period, 1);
            i.unitBatch = amount;
            if (!beltFunnel)
                i.node.flag(GlassRecords.Node.CALIBRATED); // waits until its dropped item leaves (GB)
        } else {
            i = info(K.FUNNEL, f, GlassRecords.Node.JUNCTION, model);
            if (!beltFunnel)
                accepts(f.getBlockPos(), i);
        }
        i.extra = new Object[] {facing, extracting, beltFunnel, filter, powered};
    }

    private int funnelAmount(FunnelBlockEntity f, BlockState s, FilteringBehaviour filter, boolean brass) {
        if (!brass || filter == null || !filter.isActive())
            return 1; // andesite: one item per extraction (FunnelBlockEntity.getAmountToExtract)
        return filter.anyAmount() ? 64 : Math.max(1, filter.getAmount());
    }

    /**
     * Whether a smart chute or funnel at {@code p} is held by redstone (choice GB51): never when a modelled
     * switch powers it (it runs, and the switch's CONTROL edge gates it); otherwise the signal it gets now
     * ({@code hasNeighborSignal}, what its block tick sets POWERED to), not the POWERED property, which lags a
     * tick behind a change and starts true on a smart chute placed without a player (its default state).
     */
    private boolean redstonePowered(BlockPos p) {
        // hasNeighborSignal reads the six neighbours and theirs: within two blocks of p
        return !switchDriven.contains(p.asLong()) && loadedAround(p, 2) && level.hasNeighborSignal(p);
    }

    private void chuteNode(ChuteBlockEntity c) {
        boolean smart = c instanceof SmartChuteBlockEntity;
        Info i = info(K.CHUTE, c, GlassRecords.Node.MOVER, smart ? GlassRecords.Model.SMART_CHUTE : GlassRecords.Model.CHUTE);
        int amount = Cycles.CHUTE_EXTRACTION;
        boolean powered = false;
        if (smart) {
            FilteringBehaviour f = BlockEntityBehaviour.get(c, FilteringBehaviour.TYPE);
            amount = f == null ? 64 : Cycles.smartChuteExtraction(f.isCountVisible(), f.anyAmount(), f.getAmount());
            powered = redstonePowered(c.getBlockPos());
        }
        float motion = c.getItemMotion();
        i.moverBatch = powered ? 0 : amount;
        i.unitBatch = amount;
        i.extra = motion;
        accepts(c.getBlockPos(), i);
        supplies(c.getBlockPos(), i);
    }

    // ---- fans -----------------------------------------------------------------------------------

    private void fanZones(EncasedFanBlockEntity fan) {
        // the current at the walk's speed, rebuilt from the blocks (AirCurrent.findAffectedHandlers below)
        FanCurrent air = FanCurrent.of(fan, signedSpeed(fan));
        if (air == null || air.maxDistance <= 0)
            return;
        BlockPos start = fan.getBlockPos();
        int limit = (float) (int) air.maxDistance == air.maxDistance ? (int) air.maxDistance : (int) air.maxDistance + 1;
        for (int d = 1; d <= limit; d++) {
            FanProcessingType segType = air.typeAt((float) (d - 1));
            for (int off = 0; off <= 1; off++) {
                BlockPos p = start.relative(air.direction, d).below(off);
                if (behaviourAt(p, TransportedItemStackHandlerBehaviour.TYPE) != null) {
                    FanProcessingType t = FanProcessingType.getAt(level, p);
                    BlockPos catalyst = t != null ? p : air.catalystAt((float) (d - 1));
                    if (t == null)
                        t = segType;
                    // a blaze burner of the district as the catalyst: blasting while it burns (heat FADING
                    // or more, BlastingType.isValidAt), smoking when it smoulders. Compiled as blasting
                    // whatever its heat now (its heat is state, not structure, GB59), the zone's recipe
                    // needs heat and a HEAT edge from the burner gates it (GB44); the smoking regime of
                    // a burner out of fuel is not modelled (UNVERIFIED)
                    BlockPos burner = catalyst != null && claimedPos.contains(catalyst.asLong())
                            && blockAt(catalyst) instanceof BlazeBurnerBlockEntity ? catalyst : null;
                    if (burner != null)
                        t = AllFanProcessingTypes.BLASTING;
                    if (t != null)
                        fanAt.computeIfAbsent(p.asLong(), x -> new ArrayList<>()).add(new Object[] {fan, t, burner});
                }
                if (air.direction.getAxis().isVertical())
                    break;
            }
        }
    }

    /**
     * Whether Create's walk of {@code c}'s crafter grid ({@code RecipeGridHandler.getAllCraftersOfChainIf},
     * which looks each crafter's six neighbours up through {@code Level.getBlockEntity}) stays in loaded
     * chunks: every face-connected crafter's neighbours are. Not, and the grid is not modelled (as one
     * Create's walk finds empty).
     */
    private boolean crafterGridLoaded(MechanicalCrafterBlockEntity c) {
        LongOpenHashSet seen = new LongOpenHashSet();
        java.util.ArrayDeque<BlockPos> q = new java.util.ArrayDeque<>();
        q.add(c.getBlockPos());
        seen.add(c.getBlockPos().asLong());
        while (!q.isEmpty() && seen.size() < 4096) {
            BlockPos p = q.poll();
            for (Direction d : Direction.values()) {
                BlockPos n = p.relative(d);
                if (level.isOutsideBuildHeight(n))
                    continue;
                if (!Loaded.at(level, n)) {
                    g.unloadedStops++;
                    return false;
                }
                if (seen.add(n.asLong()) && Loaded.blockEntity(level, n) instanceof MechanicalCrafterBlockEntity)
                    q.add(n);
            }
        }
        return q.isEmpty();
    }

    private static int fanRecipeType(FanProcessingType t) {
        if (t == AllFanProcessingTypes.SPLASHING)
            return GlassRecords.RecipeType.SPLASHING;
        if (t == AllFanProcessingTypes.BLASTING)
            return GlassRecords.RecipeType.BLASTING;
        if (t == AllFanProcessingTypes.SMOKING)
            return GlassRecords.RecipeType.SMOKING;
        if (t == AllFanProcessingTypes.HAUNTING)
            return GlassRecords.RecipeType.HAUNTING;
        return GlassRecords.RecipeType.OTHER;
    }

    // ---- crafters -------------------------------------------------------------------------------

    private void crafterGrids() {
        LongOpenHashSet done = new LongOpenHashSet();
        for (BlockEntity be : all(K.CRAFTER)) {
            MechanicalCrafterBlockEntity c = (MechanicalCrafterBlockEntity) be;
            if (done.contains(c.getBlockPos().asLong()) || !crafterGridLoaded(c))
                continue;
            List<MechanicalCrafterBlockEntity> chain = RecipeGridHandler.getAllCraftersOfChainIf(c, x -> true, true);
            if (chain == null || chain.isEmpty())
                continue;
            MechanicalCrafterBlockEntity end = null;
            for (MechanicalCrafterBlockEntity m : chain) {
                done.add(m.getBlockPos().asLong());
                if (RecipeGridHandler.getTargetingCrafter(m) == null)
                    end = m;
            }
            if (end == null)
                continue;
            Info i = info(K.CRAFTER, end, GlassRecords.Node.MACHINE, GlassRecords.Model.CRAFTER);
            i.pass = Pass.MACHINE;
            i.multiInput = true;
            i.extra = chain;
            attachKinetic(i, end.getBlockPos());
            for (MechanicalCrafterBlockEntity m : chain)
                accepts(m.getBlockPos(), i);
        }
    }

    /** Hand-offs from the farthest crafter of a grid to its end crafter. */
    static int crafterDepth(MechanicalCrafterBlockEntity end) {
        int depth = 0;
        List<MechanicalCrafterBlockEntity> frontier = List.of(end);
        java.util.Set<MechanicalCrafterBlockEntity> seen = new java.util.HashSet<>(frontier);
        while (true) {
            List<MechanicalCrafterBlockEntity> next = new ArrayList<>();
            for (MechanicalCrafterBlockEntity m : frontier)
                for (MechanicalCrafterBlockEntity p : RecipeGridHandler.getPrecedingCrafters(m))
                    if (seen.add(p))
                        next.add(p);
            if (next.isEmpty() || depth > 16)
                return depth;
            depth++;
            frontier = next;
        }
    }

    // ---- package logistics (BOUNDED: GB) ---------------------------------------------------------

    private final Long2IntOpenHashMap unpackAt = intMap();

    private void packageLogistics() {
        for (BlockEntity be : all(K.PACKAGER)) {
            Info pack = info(K.PACKAGER, be, GlassRecords.Node.MOVER, GlassRecords.Model.PACKAGER);
            int per = Cycles.packager();
            pack.moverBatch = 9 * 64;
            pack.mover = (rpm, b) -> Rat.of(per, 1);
            bounded(pack.node);
            supplies(be.getBlockPos(), pack);
            Info unpack = info(K.PACKAGER, be, GlassRecords.Node.MOVER, GlassRecords.Model.PACKAGER);
            unpack.moverBatch = 9 * 64;
            unpack.mover = (rpm, b) -> Rat.of(per, 1);
            bounded(unpack.node);
            unpack.extra = "unpack";
            accepts(be.getBlockPos(), unpack);
            unpackAt.put(be.getBlockPos().asLong(), unpack.node.index);
        }
        for (BlockEntity be : all(K.FROGPORT)) {
            Info port = info(K.FROGPORT, be, GlassRecords.Node.MOVER, GlassRecords.Model.FROGPORT);
            int per = Cycles.frogport();
            port.moverBatch = 9 * 64;
            port.mover = (rpm, b) -> Rat.of(Math.max(1, per), 1);
            bounded(port.node);
            accepts(be.getBlockPos(), port);
            supplies(be.getBlockPos(), port);
        }
    }

    // ---- sheep (G-B17, C32) ---------------------------------------------------------------------

    /**
     * Mean ticks from a shearing to the sheep's next {@code ate()} (corpus 13): 1.5 to the first even tick
     * ({@code Mob.serverAiStep} tries goals when {@code tickCount + id} is even), 2 (1/p - 1) more for
     * {@code EatBlockGoal.canUse}'s 1 in 1000 per try, then 34 from the start to the eat.
     */
    static final double SHEEP_REGROW_TICKS = 1.5 + 2 * (1000 - 1) + 34;

    private void deployerSheep(DeployerBlockEntity d) {
        Direction facing = d.getBlockState().getValue(BlockStateProperties.FACING);
        if (facing == Direction.DOWN && isHost(d.getBlockPos().below(2)))
            return; // a station: built with its belt or depot
        BlockPos target = d.getBlockPos().relative(facing, 2);
        List<Entity> entities = level.getEntitiesOfClass(Entity.class, new AABB(target), e -> !(e instanceof AbstractContraptionEntity));
        int total = entities.size(), eligible = 0;
        boolean onGrassBlock = false;
        java.util.Map<net.minecraft.world.item.DyeColor, Integer> colors = new java.util.TreeMap<>();
        for (Entity e : entities) {
            if (!(e instanceof Sheep sh) || sh.isBaby())
                continue;
            BlockPos feet = sh.blockPosition();
            BlockState atFeet = stateAt(feet), underFeet = stateAt(feet.below());
            boolean shortGrass = atFeet != null && atFeet.is(Blocks.SHORT_GRASS);
            boolean grassBlock = underFeet != null && underFeet.is(Blocks.GRASS_BLOCK);
            if (!shortGrass && !grassBlock)
                continue;
            onGrassBlock |= grassBlock || shortGrass;
            eligible++;
            colors.merge(sh.getColor(), 1, Integer::sum);
        }
        Info i = info(K.DEPLOYER, d, GlassRecords.Node.SOURCE, GlassRecords.Model.ANIMAL_SHEEP);
        i.pass = Pass.SOURCE;
        i.node.flag(GlassRecords.Node.STOCHASTIC);
        i.node.params(total, eligible, 14, 3); // wool per shear 1 + nextInt(3): E[X^2] = 14/3
        attachKinetic(i, d.getBlockPos());
        if (onGrassBlock && level.getGameRules().getBoolean(GameRules.RULE_MOBGRIEFING))
            bounded(i.node); // eating turns grass to dirt (or removes the short grass): regrowth stops
        supplies(d.getBlockPos(), i);
        if (eligible == 0)
            return;
        GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.SHEARING, GlassRecords.Heat.NONE, 0, g.name("minecraft:sheep_shearing"));
        ItemStack shears = held(d);
        r.in(good(shears.copyWithCount(1)), 1, 1, GlassRecords.Recipe.TERM_CATALYST);
        for (var e : colors.entrySet()) {
            ItemStack wool = new ItemStack(woolOf(e.getKey()));
            int wg = good(wool);
            r.out(wg, 2L * e.getValue(), eligible);
            i.out.add(wg);
        }
        int rec = g.recipe(r);
        final int tot = total, el = eligible;
        // one craft per shearing; its period is the pen's renewal fixed point (Cycles.shearingPeriod), an
        // expectation, rounded up to 1/1024 tick
        i.crafts.add(new Craft(rec, 1, (rpm, b) -> {
            double p = Cycles.shearingPeriod(rpm, el, tot, SHEEP_REGROW_TICKS);
            return p <= 0 ? null : Rat.of((long) Math.ceil(p * 1024), 1024);
        }));
    }

    private static net.minecraft.world.item.Item woolOf(net.minecraft.world.item.DyeColor c) {
        return switch (c) {
            case WHITE -> Items.WHITE_WOOL;
            case ORANGE -> Items.ORANGE_WOOL;
            case MAGENTA -> Items.MAGENTA_WOOL;
            case LIGHT_BLUE -> Items.LIGHT_BLUE_WOOL;
            case YELLOW -> Items.YELLOW_WOOL;
            case LIME -> Items.LIME_WOOL;
            case PINK -> Items.PINK_WOOL;
            case GRAY -> Items.GRAY_WOOL;
            case LIGHT_GRAY -> Items.LIGHT_GRAY_WOOL;
            case CYAN -> Items.CYAN_WOOL;
            case PURPLE -> Items.PURPLE_WOOL;
            case BLUE -> Items.BLUE_WOOL;
            case BROWN -> Items.BROWN_WOOL;
            case GREEN -> Items.GREEN_WOOL;
            case RED -> Items.RED_WOOL;
            case BLACK -> Items.BLACK_WOOL;
        };
    }

    // ---- step 2: belts --------------------------------------------------------------------------

    /** A belt: its segments in movement order and the chain of nodes along it. */
    private final class Belt {
        final BeltBlockEntity controller;
        final List<BlockPos> segments = new ArrayList<>(); // in movement order
        final boolean positive;
        final Info info; // carries the belt's domain and speed
        /** Movement-order index to the chain elements there, in order. */
        final TreeMap<Integer, List<Integer>> elements = new TreeMap<>();
        final IntArrayList movers = new IntArrayList();
        int stack = 64;
        /** Where the belt's end goes (BELT into an input, DROP when it ejects), NONE when blocked. */
        int endKind, endTarget = NONE;

        Belt(BeltBlockEntity controller, Info info) {
            this.controller = controller;
            this.info = info;
            this.positive = directionAwareSpeed() >= 0;
            int len = controller.beltLength;
            for (int k = 0; k < len; k++)
                segments.add(BeltHelper.getPositionForOffset(controller, positive ? k : len - 1 - k));
        }

        int index(BlockPos p) {
            return segments.indexOf(p);
        }

        /**
         * {@code BeltBlockEntity.getDirectionAwareBeltMovementSpeed} at the walk's speed ({@link #signedSpeed}),
         * not the live one: a belt that has not turned yet, or turns behind a switch-driven clutch, has its
         * running direction (GB51).
         */
        float directionAwareSpeed() {
            Direction f = controller.getBlockState().getValue(BlockStateProperties.HORIZONTAL_FACING);
            int offset = f.getAxisDirection().getStep();
            if (f.getAxis() == Direction.Axis.X)
                offset *= -1;
            return signedSpeed(controller) / 480f * offset;
        }

        /** {@code BeltBlockEntity.getMovementFacing} at the walk's speed. */
        Direction movementFacing() {
            Direction.Axis axis = controller.getBlockState().getValue(BlockStateProperties.HORIZONTAL_FACING).getAxis();
            return Direction.fromAxisAndDirection(axis, signedSpeed(controller) / 480f < 0 ^ axis == Direction.Axis.X
                    ? Direction.AxisDirection.NEGATIVE : Direction.AxisDirection.POSITIVE);
        }

        /** The mover where items join at movement index {@code k} (created on first use). */
        int entry(int k) {
            List<Integer> at = elements.computeIfAbsent(k, x -> new ArrayList<>());
            if (!at.isEmpty() && movers.contains(at.get(0).intValue()))
                return at.get(0);
            int m = newMover(segments.get(k));
            at.add(0, m);
            return m;
        }

        int newMover(BlockPos at) {
            Info m = info(K.BELT, controller, GlassRecords.Node.MOVER, GlassRecords.Model.BELT, at);
            m.domain = info.domain;
            m.ratio = info.ratio;
            m.kinetic = true;
            m.rpmNow = info.rpmNow;
            if (info.domain != NONE)
                m.node.domain(info.domain);
            m.extra = this;
            movers.add(m.node.index);
            return m.node.index;
        }

        void add(int k, int node) {
            elements.computeIfAbsent(k, x -> new ArrayList<>()).add(node);
        }

        BlockPos next() {
            return BeltHelper.getPositionForOffset(controller, positive ? controller.beltLength : -1);
        }
    }

    private void beltChains() {
        for (BlockEntity be : all(K.BELT)) {
            BeltBlockEntity b = (BeltBlockEntity) be;
            if (!b.isController())
                continue;
            Info carrier = new Info(K.BELT, b, null);
            Kin k = kin.get(b.getBlockPos().asLong());
            if (k != null) {
                carrier.domain = k.domain();
                carrier.ratio = k.ratio();
                carrier.rpmNow = k.rpm();
            }
            Belt belt = new Belt(b, carrier);
            belts.add(belt);
            for (int i = 0; i < belt.segments.size(); i++) {
                BlockPos seg = belt.segments.get(i);
                beltBySegment.put(seg.asLong(), belt);
            }
            for (int i = 0; i < belt.segments.size(); i++)
                stations(belt, i);
        }
        for (BlockEntity be : all(K.DEPOT))
            depotStations((DepotBlockEntity) be);
    }

    /** Stations and fan zones over belt segment {@code k}; tunnels and taking funnels above it. */
    private void stations(Belt belt, int k) {
        BlockPos seg = belt.segments.get(k);
        int station = station(seg, belt);
        if (station != NONE)
            belt.add(k, station);
        List<Object[]> fans = fanAt.get(seg.asLong());
        if (fans != null)
            for (Object[] f : fans) {
                int z = fanZone(seg, (EncasedFanBlockEntity) f[0], (FanProcessingType) f[1], belt, (BlockPos) f[2]);
                belt.add(k, z);
            }
    }

    /** The station working on host {@code host} (a press, deployer or spout two above), or NONE. */
    private int station(BlockPos host, Belt belt) {
        BlockEntity be = blockAt(host.above(2));
        if (be == null || !claimedPos.contains(be.getBlockPos().asLong()) || operatorAtBasin.contains(be.getBlockPos().asLong()))
            return NONE;
        boolean bulk = AllConfigs.server().recipes.bulkPressing.get();
        Info i;
        if (be instanceof MechanicalPressBlockEntity) {
            i = info(K.PRESS, be, GlassRecords.Node.MACHINE, GlassRecords.Model.PRESS);
            attachKinetic(i, be.getBlockPos());
            i.extra = belt == null ? "depot" : belt;
            if (bulk)
                bounded(i.node);
        } else if (be instanceof DeployerBlockEntity d && d.getBlockState().getValue(BlockStateProperties.FACING) == Direction.DOWN) {
            i = info(K.DEPLOYER, be, GlassRecords.Node.MACHINE, GlassRecords.Model.DEPLOYER);
            attachKinetic(i, be.getBlockPos());
            i.extra = belt == null ? "depot" : belt;
            accepts(be.getBlockPos(), i); // the held item's refill
            supplies(be.getBlockPos(), i); // its overflow
        } else if (be instanceof SpoutBlockEntity) {
            i = info(K.SPOUT, be, GlassRecords.Node.MACHINE, GlassRecords.Model.SPOUT);
            i.extra = belt == null ? "depot" : belt;
            i.multiInput = true; // an item and a fluid
            acceptFluid.put(be.getBlockPos().asLong(), i.node.index);
        } else {
            return NONE;
        }
        i.pass = Pass.MACHINE;
        i.node.flag(GlassRecords.Node.PASS_THROUGH);
        return i.node.index;
    }

    /** A fan zone at {@code host}; {@code burner} the blaze burner whose heat makes it blasting, or null. */
    private int fanZone(BlockPos host, EncasedFanBlockEntity fan, FanProcessingType type, Belt belt, BlockPos burner) {
        Info i = info(K.FAN, fan, GlassRecords.Node.MACHINE, GlassRecords.Model.FAN_ZONE, host);
        i.pass = Pass.MACHINE;
        i.node.flag(GlassRecords.Node.PASS_THROUGH);
        i.extra = new Object[] {type, belt, host, burner};
        return i.node.index;
    }

    /** A burner-catalysed fan zone's HEAT edge from its burner (param: the burner's heat when lit). */
    private void fanHeatEdge(Info i) {
        if (!(i.extra instanceof Object[] x) || x.length < 4 || !(x[3] instanceof BlockPos burner))
            return;
        int h = acceptItem.get(burner.asLong());
        Info hi = infos.get(h);
        if (hi == null || hi.kind != K.BURNER)
            return;
        GraphBuilder.Edge e = edge(GlassRecords.Edge.HEAT, h, i.node.index);
        if (e != null)
            e.param((Integer) hi.extra);
    }

    private void depotStations(DepotBlockEntity depot) {
        BlockPos p = depot.getBlockPos();
        int buffer = hostEntry.get(p.asLong());
        List<Integer> chain = new ArrayList<>();
        int st = station(p, null);
        if (st != NONE)
            chain.add(st);
        List<Object[]> fans = fanAt.get(p.asLong());
        if (fans != null)
            for (Object[] f : fans)
                chain.add(fanZone(p, (EncasedFanBlockEntity) f[0], (FanProcessingType) f[1], null, (BlockPos) f[2]));
        if (chain.isEmpty())
            return;
        // One node when one station works the depot (§9.3): the depot's edges are the station's.
        if (chain.size() == 1) {
            int m = chain.get(0);
            infos.remove(buffer);
            g.node(buffer).model = GlassRecords.Model.DEPOT; // left unused: an isolated buffer
            orphanBuffers.add(buffer);
            acceptItem.put(p.asLong(), m);
            supplyItem.put(p.asLong(), m);
            hostEntry.put(p.asLong(), m);
            return;
        }
        // Several (a press and a fan zone): buffer -> stations in order; the last one supplies.
        int prev = buffer;
        for (int m : chain) {
            edge(GlassRecords.Edge.LINE, prev, m);
            prev = m;
        }
        supplyItem.put(p.asLong(), prev);
        bounded(g.node(buffer));
    }

    private final IntOpenHashSet orphanBuffers = new IntOpenHashSet();

    // ---- step 3: edges --------------------------------------------------------------------------

    private GraphBuilder.Edge edge(int kind, int from, int to) {
        if (from == NONE || to == NONE || from == to)
            return null;
        return g.edge(kind, from, to);
    }

    private GraphBuilder.Edge edge(int kind, int from, int to, Predicate<ItemStack> items) {
        GraphBuilder.Edge e = edge(kind, from, to);
        if (e != null && items != null)
            edgeTests.put(e, new EdgeTest(items, null));
        return e;
    }

    private GraphBuilder.Edge fluidEdge(int from, int to) {
        GraphBuilder.Edge e = edge(GlassRecords.Edge.PIPE, from, to);
        if (e != null)
            e.flag(GlassRecords.Edge.FLUID);
        return e;
    }

    /** The node that takes items inserted at {@code p}: a modelled machine, a belt, a store, or the residual. */
    private int acceptAt(BlockPos p) {
        int n = acceptItem.get(p.asLong());
        if (n != NONE)
            return n;
        Belt b = beltBySegment.get(p.asLong());
        if (b != null)
            return b.entry(b.index(p));
        return storeOrResidual(p);
    }

    private int supplyAt(BlockPos p) {
        int n = supplyItem.get(p.asLong());
        if (n != NONE)
            return n;
        return storeOrResidual(p);
    }

    private int acceptFluidAt(BlockPos p) {
        int n = acceptFluid.get(p.asLong());
        if (n != NONE)
            return n;
        n = acceptFluid.get(canonical(p).asLong());
        return n != NONE ? n : storeOrResidual(p);
    }

    private int supplyFluidAt(BlockPos p) {
        int n = supplyFluid.get(p.asLong());
        if (n != NONE)
            return n;
        n = supplyFluid.get(canonical(p).asLong());
        return n != NONE ? n : storeOrResidual(p);
    }

    private BlockPos canonical(BlockPos p) {
        BlockEntity be = blockAt(p);
        return be instanceof IMultiBlockEntityContainer m && m.getController() != null ? m.getController() : p;
    }

    private int storeOrResidual(BlockPos p) {
        int s = g.storeAt(p);
        if (s != NONE)
            return s;
        return g.residualAt(p);
    }

    /** Where a dropped item ends up when it falls from {@code from}: a collector below, or the floor (a sink). */
    private int dropAt(BlockPos from) {
        for (int dy = 0; dy <= 8; dy++) {
            BlockPos p = from.below(dy);
            int c = collector(p);
            if (c != NONE)
                return c;
            BlockState s = stateAt(p);
            if (s == null || !s.getCollisionShape(level, p).isEmpty())
                return sink(); // a chunk that is not loaded: the item leaves the district
        }
        return sink();
    }

    /** A block that picks up item entities that land in or on it. */
    private int collector(BlockPos p) {
        BlockEntity be = blockAt(p);
        if (be == null)
            return NONE;
        Belt b = beltBySegment.get(p.asLong());
        if (b != null)
            return b.entry(b.index(p));
        if (be instanceof DepotBlockEntity || be instanceof BasinBlockEntity || be instanceof ChuteBlockEntity
                || be instanceof CrushingWheelControllerBlockEntity || be instanceof HopperBlockEntity
                || (be instanceof SawBlockEntity && be.getBlockState().getValue(BlockStateProperties.FACING) == Direction.UP)
                || (be instanceof FunnelBlockEntity && acceptItem.containsKey(p.asLong())))
            return acceptAt(p);
        return NONE;
    }

    private int sink() {
        if (sink == NONE) {
            GraphBuilder.Node n = g.node(GlassRecords.Node.SINK, GlassRecords.Model.SINK_VOID, g.district.watch.isEmpty() ? BlockPos.ZERO : g.district.watch.get(0));
            sink = n.index;
        }
        return sink;
    }

    /** Where a machine at {@code from} puts an output toward {@code target}: a belt-input there, else a drop. */
    private int outputTo(BlockPos target) {
        if (behaviourAt(target, DirectBeltInputBehaviour.TYPE) != null)
            return acceptAt(target);
        return dropAt(target);
    }

    private void edges() {
        for (Info i : new ArrayList<>(infos.values()))
            switch (i.kind) {
                case MILLSTONE -> {} // fed and emptied by others' edges
                case CRUSHER -> {
                    Direction facing = i.be.getBlockState().getValue(CrushingWheelControllerBlock.FACING);
                    BlockPos p = i.be.getBlockPos();
                    if (facing != Direction.UP) {
                        BlockPos next = p.below().relative(facing, facing.getAxis() == Direction.Axis.Y ? 0 : 1);
                        edge(GlassRecords.Edge.ADJACENCY, i.node.index, outputTo(next));
                    } else {
                        edge(GlassRecords.Edge.DROP, i.node.index, dropAt(p.above()));
                    }
                }
                case SAW -> {
                    if (i.node.kind != GlassRecords.Node.MACHINE)
                        break;
                    SawBlockEntity s = (SawBlockEntity) i.be;
                    var v = s.getItemMovementVec();
                    BlockPos next = s.getBlockPos().offset(BlockPos.containing(v));
                    edge(GlassRecords.Edge.ADJACENCY, i.node.index, outputTo(next));
                }
                case MIXER, PRESS, BASIN -> {
                    if (i.extra instanceof BasinBlockEntity basin)
                        basinEdges(i, basin);
                }
                case CRAFTER -> {
                    MechanicalCrafterBlockEntity end = (MechanicalCrafterBlockEntity) i.be;
                    BlockPos target = end.getBlockPos().relative(MechanicalCrafterBlock.getTargetDirection(end.getBlockState()));
                    int to = behaviourAt(target, DirectBeltInputBehaviour.TYPE) != null || capAt(Capabilities.ItemHandler.BLOCK, target, null) != null
                            ? acceptAt(target) : dropAt(target);
                    edge(GlassRecords.Edge.ADJACENCY, i.node.index, to);
                }
                case BURNER -> heatEdges(i);
                case FAN -> fanHeatEdge(i);
                case FUNNEL -> funnelEdges(i);
                case CHUTE -> chuteEdges(i);
                case ARM -> armEdges(i);
                case HOPPER -> {
                    BlockPos p = i.be.getBlockPos();
                    edge(GlassRecords.Edge.ADJACENCY, supplyAtNotChute(p.above()), i.node.index);
                    Direction facing = i.be.getBlockState().getValue(HopperBlock.FACING);
                    edge(GlassRecords.Edge.ADJACENCY, i.node.index, acceptAt(p.relative(facing)));
                }
                case PUMP -> pumpEdges(i);
                case DRAIN -> drainEdges(i);
                case PACKAGER -> packagerEdges(i);
                case FROGPORT -> frogportEdges(i);
                case SWITCH -> switchEdges(i);
                default -> {}
            }
        // Two passes: tunnels and belt ends may feed another belt (a new entry mover there), so every
        // belt's entries exist before any chain is linked.
        for (Belt b : belts)
            beltOutlets(b);
        for (Belt b : belts)
            beltEdges(b);
        // a brass tunnel whose front flap has a filter: its straight-on link is a filtered output too
        for (GraphBuilder.Edge e : g.edges()) {
            TunnelFront tf = brassFront.get(e.from);
            if (tf == null || (e.kind != GlassRecords.Edge.BELT && e.kind != GlassRecords.Edge.DROP))
                continue;
            e.flag(GlassRecords.Edge.FILTERED_SIDE);
            edgeTests.put(e, new EdgeTest(st -> tf.brass().testFlapFilter(tf.front(), st), null));
        }
    }

    /** Belt movers under a brass tunnel whose front flap has a filter (their straight-on link is filtered). */
    private final Int2ObjectOpenHashMap<TunnelFront> brassFront = new Int2ObjectOpenHashMap<>();

    private record TunnelFront(BrassTunnelBlockEntity brass, Direction front) {}

    private int supplyAtNotChute(BlockPos p) {
        return blockAt(p) instanceof ChuteBlockEntity ? NONE : supplyAt(p);
    }

    private void basinEdges(Info i, BasinBlockEntity basin) {
        BlockPos p = basin.getBlockPos();
        Direction facing = basin.getBlockState().getValue(BasinBlock.FACING);
        if (facing != Direction.DOWN) {
            BlockPos t = p.below().relative(facing);
            edge(GlassRecords.Edge.ADJACENCY, i.node.index, acceptAt(t));
            fluidEdge(i.node.index, acceptFluid.containsKey(t.asLong()) || capAt(Capabilities.FluidHandler.BLOCK, t, null) != null ? acceptFluidAt(t) : NONE);
        }
        if (i.kind == K.BASIN)
            return;
        BlockEntity below = blockAt(p.below());
        if (below instanceof BlazeBurnerBlockEntity) {
            int h = acceptItem.get(below.getBlockPos().asLong());
            Info hi = infos.get(h);
            if (hi != null && hi.kind == K.BURNER) {
                GraphBuilder.Edge e = edge(GlassRecords.Edge.HEAT, h, i.node.index);
                if (e != null)
                    e.param((Integer) hi.extra);
            }
        }
    }

    /** A burner's heat edges into a boiler above it (a basin's come from basinEdges). */
    private void heatEdges(Info i) {
        BlockPos above = i.be.getBlockPos().above();
        if (blockAt(above) instanceof FluidTankBlockEntity t && tankController(t) != null) {
            int b = acceptFluid.get(t.getController().asLong());
            if (b != NONE && infos.get(b) != null && infos.get(b).kind == K.BOILER) {
                GraphBuilder.Edge e = edge(GlassRecords.Edge.HEAT, i.node.index, b);
                if (e != null) // boiler heat when lit: 2 superheated, 1 heated (BoilerHeater, BlazeBurner)
                    e.param((Integer) i.extra >= GlassRecords.Heat.SEETHING ? 2 : (Integer) i.extra >= GlassRecords.Heat.FADING ? 1 : 0);
            }
        }
    }

    private void funnelEdges(Info i) {
        Object[] x = (Object[]) i.extra;
        Direction facing = (Direction) x[0];
        boolean extracting = (Boolean) x[1], beltFunnel = (Boolean) x[2];
        FilteringBehaviour filter = (FilteringBehaviour) x[3];
        Predicate<ItemStack> test = filter == null ? null : filter::test;
        BlockPos p = i.be.getBlockPos();
        BlockPos behind = p.relative(facing.getOpposite());
        if (extracting) {
            edge(GlassRecords.Edge.ADJACENCY, supplyAt(behind), i.node.index, test);
            if (beltFunnel) {
                edge(GlassRecords.Edge.ADJACENCY, i.node.index, acceptAt(p.below()));
            } else {
                BlockPos front = p.relative(facing);
                edge(GlassRecords.Edge.DROP, i.node.index, facing == Direction.UP ? dropAt(p) : dropAt(front));
            }
        } else {
            edge(GlassRecords.Edge.ADJACENCY, i.node.index, acceptAt(behind), test);
            if (beltFunnel) {
                Belt b = beltBySegment.get(p.below().asLong());
                if (b != null)
                    b.add(b.index(p.below()), i.node.index); // a taker on the belt (Rule PREFER_FIRST)
            }
        }
    }

    /**
     * Whether a downward chute with no inventory under it drops its item into the world
     * ({@code ChuteBlockEntity.handleDownwardOutput}): not when it cannot activate (a smart chute powered:
     * {@code SmartChuteBlockEntity.canActivate}), faces sideways (a diagonal chute hands on to its target
     * chute), sits on a funnel facing down, or sits on a block that can support a rigid block
     * ({@code Block.canSupportRigidBlock}: a full floor). Otherwise the item waits in the chute for a
     * taker (an arm, a funnel), and no DROP edge is emitted.
     */
    private boolean dropsBelow(ChuteBlockEntity c) {
        BlockPos p = c.getBlockPos(), below = p.below();
        if (c instanceof SmartChuteBlockEntity && redstonePowered(p))
            return false;
        if (com.simibubi.create.content.logistics.chute.AbstractChuteBlock.getChuteFacing(c.getBlockState()).getAxis().isHorizontal())
            return false;
        BlockState bs = stateAt(below);
        if (bs == null)
            return true; // the floor is in a chunk that is not loaded: nothing holds the item (a drop to the sink)
        if (bs.getBlock() instanceof AbstractFunnelBlock && AbstractFunnelBlock.getFunnelFacing(bs) == Direction.DOWN)
            return false;
        return !net.minecraft.world.level.block.Block.canSupportRigidBlock(level, below);
    }

    private void chuteEdges(Info i) {
        ChuteBlockEntity c = (ChuteBlockEntity) i.be;
        BlockPos p = c.getBlockPos();
        float motion = (Float) i.extra;
        Predicate<ItemStack> test = null;
        if (c instanceof SmartChuteBlockEntity) {
            FilteringBehaviour f = BlockEntityBehaviour.get(c, FilteringBehaviour.TYPE);
            test = f == null ? null : f::test;
        }
        int ticks;
        if (motion < 0) {
            // down: pulls from the inventory above (not a chute), passes to the inventory or chute below, or drops
            BlockEntity above = blockAt(p.above());
            boolean top = !(above instanceof ChuteBlockEntity);
            if (top && above != null)
                edge(GlassRecords.Edge.CHUTE, supplyAt(p.above()), i.node.index, test);
            BlockEntity below = blockAt(p.below());
            if (below instanceof ChuteBlockEntity)
                edge(GlassRecords.Edge.CHUTE, i.node.index, acceptItem.get(p.below().asLong()));
            else if (below != null && (capAt(Capabilities.ItemHandler.BLOCK, p.below(), Direction.UP) != null
                    || behaviourAt(p.below(), DirectBeltInputBehaviour.TYPE) != null))
                edge(GlassRecords.Edge.CHUTE, i.node.index, acceptAt(p.below()));
            else if (dropsBelow(c))
                edge(GlassRecords.Edge.DROP, i.node.index, dropAt(p.below()));
            int perBlock = Cycles.chuteTicksPerBlock(motion);
            ticks = top ? Cycles.chuteGrabPeriod(motion) : perBlock;
        } else if (motion > 0) {
            // up (a fan pushes or pulls the air up): pulls from the inventory below, passes to the inventory
            // or chute above; the period is Cycles' upward re-enactment, a starting estimate (CALIBRATED).
            // It once called the downward one, which threw and left a whole district not compilable
            // (docs/deploy-rehearsal-omegasrv.md: the main base of a live world had an upward chute)
            BlockEntity below = blockAt(p.below());
            if (!(below instanceof ChuteBlockEntity) && below != null)
                edge(GlassRecords.Edge.CHUTE, supplyAt(p.below()), i.node.index, test);
            BlockEntity above = blockAt(p.above());
            if (above != null)
                edge(GlassRecords.Edge.CHUTE, i.node.index, above instanceof ChuteBlockEntity ? acceptItem.get(p.above().asLong()) : acceptAt(p.above()));
            ticks = Cycles.chuteGrabPeriod(motion);
            i.node.flag(GlassRecords.Node.CALIBRATED);
        } else {
            ticks = 0;
        }
        final int t = ticks;
        i.mover = (rpm, b) -> t <= 0 ? null : Rat.of(t, 1);
    }

    private void armEdges(Info i) {
        ArmBlockEntity a = (ArmBlockEntity) i.be;
        List<ArmInteractionPoint> ins = CreateGlassAccess.armInputs(a), outs = CreateGlassAccess.armOutputs(a);
        int order = 0;
        for (ArmInteractionPoint pt : ins) {
            int from = armPointSupply(pt);
            GraphBuilder.Edge e = edge(GlassRecords.Edge.ARM_REACH, from, i.node.index);
            if (e != null)
                e.order(order++);
        }
        order = 0;
        int accept = 0;
        for (ArmInteractionPoint pt : outs) {
            int to = armPointAccept(pt);
            GraphBuilder.Edge e = edge(GlassRecords.Edge.ARM_REACH, i.node.index, to);
            if (e != null) {
                e.order(order++);
                // a funnel point inserts through the funnel: its filter limits what the arm hands it
                if (blockAt(pt.getPos()) instanceof FunnelBlockEntity fb) {
                    FilteringBehaviour ff = BlockEntityBehaviour.get(fb, FilteringBehaviour.TYPE);
                    if (ff != null && !ff.getFilter().isEmpty())
                        edgeTests.put(e, new EdgeTest(ff::test, null));
                }
                Info ti = infos.get(to);
                accept += ti != null && ti.kind == K.BURNER ? 1 : 64;
            }
        }
        ScrollOptionBehaviour<?> mode = null;
        for (BlockEntityBehaviour b : a.getAllBehaviours())
            if (b instanceof ScrollOptionBehaviour<?> s)
                mode = s;
        int rule = GlassRecords.Rule.ROUND_ROBIN;
        if (mode != null && mode.get() instanceof ArmBlockEntity.SelectionMode m)
            rule = switch (m) {
                case ROUND_ROBIN -> GlassRecords.Rule.ROUND_ROBIN;
                case FORCED_ROUND_ROBIN -> GlassRecords.Rule.FORCED_ROUND_ROBIN;
                case PREFER_FIRST -> GlassRecords.Rule.PREFER_FIRST;
            };
        if (outs.size() > 1)
            i.node.rule(rule);
        i.moverBatch = Math.max(1, Math.min(64, accept));
        i.unitBatch = i.moverBatch;
        i.mover = (rpm, b) -> {
            Cycles.Ratio r = Cycles.armCycle(rpm);
            return r == null || r.num() <= 0 ? null : Rat.of(r.num(), r.den());
        };
        if (outs.size() > 1 && accept != 64L * outs.size())
            bounded(i.node); // mixed outputs (a burner takes one): the batch is a bound
    }

    private int armPointSupply(ArmInteractionPoint pt) {
        BlockPos p = pt.getPos();
        Belt b = beltBySegment.get(p.asLong());
        if (b != null) {
            int k = b.index(p);
            int taker = g.node(GlassRecords.Node.JUNCTION, GlassRecords.Model.ARM, p).index;
            infos.put(taker, new Info(K.ARM, null, g.node(taker)));
            b.add(k, taker);
            return taker;
        }
        if (blockAt(p) instanceof FunnelBlockEntity)
            return NONE;
        return supplyAt(p);
    }

    private int armPointAccept(ArmInteractionPoint pt) {
        BlockPos p = pt.getPos();
        BlockEntity be = blockAt(p);
        if (be instanceof FunnelBlockEntity) {
            Direction f = AbstractFunnelBlock.getFunnelFacing(be.getBlockState());
            return f == null ? NONE : acceptAt(p.relative(f.getOpposite()));
        }
        return acceptAt(p);
    }

    private void pumpEdges(Info i) {
        PumpBlockEntity pump = (PumpBlockEntity) i.be;
        Direction front = pump.getBlockState().getValue(PumpBlock.FACING);
        List<Integer> sources = new ArrayList<>(), targets = new ArrayList<>();
        boolean whole = pipeEnds(pump.getBlockPos(), front.getOpposite(), true, sources);
        whole &= pipeEnds(pump.getBlockPos(), front, false, targets);
        if (!whole)
            bounded(i.node); // its pipes run into a chunk that is not loaded: the ends there are not known (SC43)
        for (int s : sources)
            fluidEdge(s, i.node.index);
        for (int t : targets)
            fluidEdge(i.node.index, t);
        int branches = Math.max(1, targets.size());
        i.moverBatch = 1;
        i.mover = (rpm, b) -> {
            int mb = Cycles.pumpTransfer(rpm, branches);
            return mb <= 0 ? null : Rat.of(1, mb);
        };
    }

    /**
     * The fluid endpoints a pipe network reaches from {@code from} through side {@code side}. False when the
     * walk met a chunk that is not loaded and stopped there (never a spill: SC43).
     */
    private boolean pipeEnds(BlockPos from, Direction side, boolean supply, List<Integer> out) {
        java.util.ArrayDeque<BlockPos> q = new java.util.ArrayDeque<>();
        LongOpenHashSet seen = new LongOpenHashSet();
        seen.add(from.asLong());
        BlockPos first = from.relative(side);
        q.add(first);
        seen.add(first.asLong());
        java.util.Map<BlockPos, Direction> via = new java.util.HashMap<>();
        via.put(first, side);
        int budget = 4096;
        boolean whole = true;
        while (!q.isEmpty() && budget-- > 0) {
            BlockPos p = q.poll();
            if (!level.isOutsideBuildHeight(p) && !Loaded.at(level, p)) {
                g.unloadedStops++;
                whole = false;
                continue;
            }
            BlockEntity be = blockAt(p);
            FluidTransportBehaviour pipe = be == null ? null : BlockEntityBehaviour.get(be, FluidTransportBehaviour.TYPE);
            if (pipe != null && !(be instanceof PumpBlockEntity)) {
                BlockState st = be.getBlockState();
                if (!pipe.canHaveFlowToward(st, via.get(p).getOpposite()))
                    continue;
                for (Direction d : Direction.values()) {
                    BlockPos n = p.relative(d);
                    if (!pipe.canHaveFlowToward(st, d) || !seen.add(n.asLong()))
                        continue;
                    via.put(n, d);
                    q.add(n);
                }
                continue;
            }
            if (be instanceof PumpBlockEntity)
                continue; // pumps in series: each is its own mover (GB)
            int n = supply ? supplyFluidAt(p) : acceptFluidAt(p);
            if (n == NONE && be == null) {
                // an open end: a water source block feeds it; else it spills
                net.minecraft.world.level.material.FluidState fluid = Loaded.fluid(level, p);
                if (supply && fluid.isSource() && fluid.is(net.minecraft.tags.FluidTags.WATER))
                    n = waterSource(p);
                else if (!supply)
                    n = sink();
            }
            if (n != NONE && !out.contains(n))
                out.add(n);
        }
        return whole;
    }

    private int waterSource(BlockPos p) {
        Info i = info(K.PIPE, null, GlassRecords.Node.SOURCE, GlassRecords.Model.WATER_SOURCE, p);
        i.pass = Pass.SOURCE;
        i.node.flag(GlassRecords.Node.FLUID);
        int water = good(new FluidStack(net.minecraft.world.level.material.Fluids.WATER, 1000));
        GraphBuilder.Recipe r = new GraphBuilder.Recipe(GlassRecords.RecipeType.OTHER, GlassRecords.Heat.NONE, 0, g.name("minecraft:water_source"));
        r.out(water, 1, 1);
        i.crafts.add(new Craft(g.recipe(r), 0, null));
        i.out.add(water);
        return i.node.index;
    }

    private void drainEdges(Info i) {
        BlockPos p = i.be.getBlockPos();
        // Items leave on the side opposite where they came in: every horizontal belt-input that is not
        // a belt ending into the drain (BOUNDED when there are several).
        List<Integer> outs = new ArrayList<>();
        for (Direction d : Direction.Plane.HORIZONTAL) {
            BlockPos n = p.relative(d);
            Belt b = beltBySegment.get(n.asLong());
            if (b != null && b.next().equals(p))
                continue;
            if (behaviourAt(n, DirectBeltInputBehaviour.TYPE) != null)
                outs.add(acceptAt(n));
        }
        for (int o : outs)
            edge(GlassRecords.Edge.ADJACENCY, i.node.index, o);
        if (outs.size() > 1)
            bounded(i.node);
    }

    private void packagerEdges(Info i) {
        BlockPos p = i.be.getBlockPos();
        Direction facing = i.be.getBlockState().getOptionalValue(PackagerBlock.FACING).orElse(Direction.UP);
        BlockPos target = p.relative(facing.getOpposite());
        if ("unpack".equals(i.extra))
            edge(GlassRecords.Edge.ADJACENCY, i.node.index, acceptAt(target));
        else
            edge(GlassRecords.Edge.ADJACENCY, supplyAt(target), i.node.index);
    }

    private void frogportEdges(Info i) {
        PackagePortBlockEntity port = (PackagePortBlockEntity) i.be;
        BlockPos p = port.getBlockPos();
        // Packages from an adjacent packager go out on the chain; packages for this port come off it
        // into an adjacent packager, which unpacks them (BOUNDED: addresses are not modelled).
        for (Direction d : Direction.values()) {
            BlockPos n = p.relative(d);
            int unpack = unpackAt.get(n.asLong());
            if (unpack != NONE)
                edge(GlassRecords.Edge.ADJACENCY, i.node.index, unpack);
            if (blockAt(n) instanceof PackagerBlockEntity)
                edge(GlassRecords.Edge.ADJACENCY, supplyItem.get(n.asLong()), i.node.index);
        }
        if (port.target == null)
            return;
        BlockPos chain = p.offset(port.target.relativePos);
        LongOpenHashSet network = chainNetwork(chain);
        for (BlockEntity be : all(K.FROGPORT)) {
            if (be == port || !(be instanceof PackagePortBlockEntity other) || other.target == null)
                continue;
            if (network.contains(other.getBlockPos().offset(other.target.relativePos).asLong())) {
                GraphBuilder.Edge e = edge(GlassRecords.Edge.CHAIN, i.node.index, acceptItem.get(other.getBlockPos().asLong()));
                if (e != null)
                    e.flag(GlassRecords.Edge.BOUNDED);
            }
        }
    }

    private LongOpenHashSet chainNetwork(BlockPos start) {
        LongOpenHashSet seen = new LongOpenHashSet();
        java.util.ArrayDeque<BlockPos> q = new java.util.ArrayDeque<>();
        q.add(start);
        seen.add(start.asLong());
        while (!q.isEmpty() && seen.size() < 4096) {
            BlockPos p = q.poll();
            if (!(blockAt(p) instanceof ChainConveyorBlockEntity c))
                continue;
            for (BlockPos rel : c.connections) {
                BlockPos n = p.offset(rel);
                if (seen.add(n.asLong()))
                    q.add(n);
            }
        }
        return seen;
    }

    private void switchEdges(Info i) {
        ThresholdSwitchBlockEntity sw = (ThresholdSwitchBlockEntity) i.be;
        Direction toTarget = ThresholdSwitchBlock.getTargetDirection(sw.getBlockState());
        BlockPos target = sw.getBlockPos().relative(toTarget);
        int observed = supplyAt(target);
        if (observed == NONE)
            observed = supplyFluidAt(target);
        FilteringBehaviour f = BlockEntityBehaviour.get(sw, FilteringBehaviour.TYPE);
        GraphBuilder.Edge obs = edge(GlassRecords.Edge.OBSERVE, observed, i.node.index);
        if (obs != null && f != null && !f.getFilter().isEmpty())
            edgeTests.put(obs, new EdgeTest(f::test, f::test));
        // flags: 1 inverted, 2 on at compile (the state before inversion, which holds between the thresholds)
        i.node.params(sw.onWhenAbove, sw.offWhenBelow, (sw.isInverted() ? 1 : 0) | (sw.getState() ? 2 : 0), Math.max(0, sw.currentLevel));
        // Powered = on XOR inverted; a gated block stops while powered (GB: the param already holds the inversion).
        long param = sw.isInverted() ? GlassRecords.Edge.CONTROL_RUN_WHEN_ON : GlassRecords.Edge.CONTROL_STOP_WHEN_ON;
        for (int t : redstoneTargets(sw.getBlockPos(), toTarget)) {
            GraphBuilder.Edge e = edge(GlassRecords.Edge.CONTROL, i.node.index, t);
            if (e != null)
                e.param(param);
        }
    }

    /** Gated nodes powered by a switch at {@code at}: its neighbours and blocks along redstone dust from it. */
    private IntArrayList redstoneTargets(BlockPos at, Direction notSide) {
        LongOpenHashSet powered = poweredBy(at, notSide);
        IntArrayList out = new IntArrayList();
        for (Info i : infos.values()) {
            if (i.be == null || !powered.contains(i.be.getBlockPos().asLong()))
                continue;
            boolean gated = switch (i.kind) {
                case FUNNEL -> i.node.kind == GlassRecords.Node.MOVER || i.node.kind == GlassRecords.Node.JUNCTION;
                case CHUTE -> i.be instanceof SmartChuteBlockEntity;
                case ARM, HOPPER -> true;
                case DEPLOYER -> i.node.kind == GlassRecords.Node.MACHINE;
                default -> false;
            };
            if (gated && !out.contains(i.node.index))
                out.add(i.node.index);
        }
        // A powered clutch stops everything that turns through it (ClutchBlockEntity: speed modifier 0 on
        // the side away from its source). The compile has it engaged (GB51); the gate is every node whose
        // kinetic block entity is on its far side, the same set whether it is on or off at compile.
        LongOpenHashSet turned = new LongOpenHashSet();
        for (long p : powered)
            if (Loaded.blockEntity(level, BlockPos.of(p)) instanceof com.simibubi.create.content.kinetics.transmission.ClutchBlockEntity c)
                turned.addAll(turnedBy(c));
        if (!turned.isEmpty())
            for (Info i : infos.values()) {
                if (i.node == null)
                    continue;
                long kp = i.kinPos != Long.MIN_VALUE ? i.kinPos : i.be instanceof KineticBlockEntity k ? k.getBlockPos().asLong() : Long.MIN_VALUE;
                if (turned.contains(kp) && !out.contains(i.node.index))
                    out.add(i.node.index);
            }
        return out;
    }

    /** Positions a redstone source at {@code at} powers: its neighbours but {@code notSide}, and along dust. */
    private LongOpenHashSet poweredBy(BlockPos at, Direction notSide) {
        LongOpenHashSet powered = new LongOpenHashSet();
        java.util.ArrayDeque<BlockPos> dust = new java.util.ArrayDeque<>();
        LongOpenHashSet seenDust = new LongOpenHashSet();
        for (Direction d : Direction.values()) {
            if (d == notSide)
                continue;
            BlockPos n = at.relative(d);
            powered.add(n.asLong());
            if (stateAt(n) instanceof BlockState s && s.getBlock() instanceof RedStoneWireBlock && seenDust.add(n.asLong()))
                dust.add(n);
        }
        int steps = 0;
        while (!dust.isEmpty() && steps++ < 64) {
            BlockPos w = dust.poll();
            for (Direction d : Direction.values()) {
                BlockPos n = w.relative(d);
                powered.add(n.asLong());
                BlockState s = stateAt(n);
                if (s != null && !s.isAir() && s.isRedstoneConductor(level, n))
                    for (Direction d2 : Direction.values())
                        powered.add(n.relative(d2).asLong());
            }
            for (Direction d : Direction.Plane.HORIZONTAL)
                for (int dy = -1; dy <= 1; dy++) {
                    BlockPos n = w.relative(d).above(dy);
                    if (stateAt(n) instanceof BlockState s && s.getBlock() instanceof RedStoneWireBlock && seenDust.add(n.asLong()))
                        dust.add(n);
                }
        }
        return powered;
    }

    /** A belt's tunnels (side outputs) and where its end goes: INSERT, EJECT (a drop) or blocked. */
    private void beltOutlets(Belt b) {
        for (int k = 0; k < b.segments.size(); k++) {
            BlockPos above = b.segments.get(k).above();
            BlockEntity be = blockAt(above);
            if (be instanceof BeltTunnelBlockEntity t && claimedPos.contains(above.asLong()))
                tunnel(b, k, t);
        }
        BlockPos next = b.next();
        if (behaviourAt(next, DirectBeltInputBehaviour.TYPE) != null) {
            b.endKind = GlassRecords.Edge.BELT;
            b.endTarget = acceptAt(next);
        } else {
            BlockState s = stateAt(next);
            if (s == null || !s.isFaceSturdy(level, next, b.movementFacing().getOpposite())) {
                b.endKind = GlassRecords.Edge.DROP;
                b.endTarget = dropAt(next);
            }
        }
    }

    /**
     * A belt's chain: links between its elements in movement order, takers, and the end's edge. The
     * belt's own links are THROUGH: an insertion onto a segment (a belt's end, a funnel, a chute)
     * does not wait for the items carried along it ({@code BeltInventory.canInsertAtFromSide} only
     * looks at earlier insertions from the same side, GB72). The end's edge is the next belt's
     * insertion, not THROUGH.
     */
    private void beltEdges(Belt b) {
        if (b.elements.isEmpty())
            return;
        // The chain starts with a mover (where items join), even if only a taker is there.
        b.entry(b.elements.firstKey());
        int prev = NONE;
        for (var e : b.elements.entrySet()) {
            List<Integer> at = new ArrayList<>(e.getValue());
            for (int n : at) {
                Info ni = infos.get(n);
                boolean taker = ni != null && (ni.kind == K.FUNNEL || (ni.kind == K.ARM && ni.be == null));
                if (taker) {
                    // a belt funnel or arm taking from here: the belt mover before it splits, the taker first
                    int m = prev == NONE ? b.entry(e.getKey()) : prev;
                    if (infos.get(m) == null || infos.get(m).kind != K.BELT) {
                        int nm = b.newMover(b.segments.get(e.getKey()));
                        GraphBuilder.Edge se = edge(GlassRecords.Edge.BELT, m, nm);
                        if (se != null)
                            se.flag(GlassRecords.Edge.THROUGH);
                        m = nm;
                    }
                    GraphBuilder.Edge te = edge(GlassRecords.Edge.ADJACENCY, m, n);
                    if (te != null)
                        te.order(0);
                    g.node(m).rule(GlassRecords.Rule.PREFER_FIRST);
                    prev = m;
                    continue;
                }
                if (prev != NONE) {
                    GraphBuilder.Edge le = edge(GlassRecords.Edge.BELT, prev, n);
                    if (le != null) {
                        le.order(1);
                        le.flag(GlassRecords.Edge.THROUGH);
                    }
                }
                prev = n;
            }
        }
        if (b.endTarget != NONE) {
            GraphBuilder.Edge e = edge(b.endKind, prev, b.endTarget);
            if (e != null)
                e.order(1);
        }
    }

    private void tunnel(Belt b, int k, BeltTunnelBlockEntity t) {
        int m = b.entry(k);
        Info mi = infos.get(m);
        BlockPos seg = b.segments.get(k);
        Direction movement = b.movementFacing();
        if (t instanceof BrassTunnelBlockEntity brass) {
            ScrollOptionBehaviour<?> mode = null;
            for (BlockEntityBehaviour x : brass.getAllBehaviours())
                if (x instanceof ScrollOptionBehaviour<?> s)
                    mode = s;
            int rule = GlassRecords.Rule.SPLIT;
            if (mode != null && mode.get() instanceof BrassTunnelBlockEntity.SelectionMode sm)
                rule = sm.ordinal() + 1;
            mi.node.rule(rule);
            int order = 0;
            // own sides first (BrassTunnelBlockEntity.addValidOutputsOf with prioritizeSides), then onward.
            // tick() puts each valid output in the filtered list when its flap has a filter
            // (!flapFilterEmpty), else in the unfiltered one, and serves the filtered list first: only a
            // side with a filter is FILTERED_SIDE; an unfiltered side shares the rule with straight on.
            for (Direction d : Direction.Plane.HORIZONTAL) {
                if (d.getAxis() == movement.getAxis() || !t.sides.contains(d))
                    continue;
                BlockPos out = seg.relative(d);
                if (behaviourAt(out, DirectBeltInputBehaviour.TYPE) == null)
                    continue;
                GraphBuilder.Edge e = edge(GlassRecords.Edge.ADJACENCY, m, acceptAt(out));
                if (e != null) {
                    e.order(order++);
                    if (!brass.flapFilterEmpty(d)) {
                        e.flag(GlassRecords.Edge.FILTERED_SIDE);
                        edgeTests.put(e, new EdgeTest(st -> brass.testFlapFilter(d, st), null));
                    }
                }
            }
            // straight on (the next segment or the belt's end): linked by beltEdges, flagged there
            if (t.sides.contains(movement) && !brass.flapFilterEmpty(movement))
                brassFront.put(m, new TunnelFront(brass, movement));
            bounded(mi.node); // the group's other tunnels (parallel belts) are not merged in G1
        } else {
            mi.node.rule(GlassRecords.Rule.ANDESITE_SPLIT);
            andesiteMovers.add(m);
            // the main line is the first out-edge (§9.3): the belt's own link has order 1, the sides after it
            int order = 2;
            for (Direction d : Direction.Plane.HORIZONTAL) {
                if (d.getAxis() == movement.getAxis() || !t.flaps.containsKey(d))
                    continue;
                BlockPos out = seg.relative(d);
                if (behaviourAt(out, DirectBeltInputBehaviour.TYPE) == null)
                    continue;
                GraphBuilder.Edge e = edge(GlassRecords.Edge.ADJACENCY, m, acceptAt(out));
                if (e != null)
                    e.order(order++);
            }
        }
    }

    // ---- step 4 and 5: goods and recipes --------------------------------------------------------

    int good(ItemStack s) {
        int gid = g.good(s);
        if (!itemProto.containsKey(gid))
            itemProto.put(gid, s.copyWithCount(1));
        return gid;
    }

    int good(FluidStack s) {
        int gid = g.good(s);
        if (!fluidProto.containsKey(gid))
            fluidProto.put(gid, s.copyWithAmount(1000));
        return gid;
    }

    private void seedContents(Info i, int node) {
        BlockEntity be = i == null ? null : i.be;
        if (be == null)
            return;
        IItemHandler items = capAt(Capabilities.ItemHandler.BLOCK, be.getBlockPos(), null);
        if (items != null)
            for (int s = 0; s < Math.min(items.getSlots(), 256); s++) {
                ItemStack st = items.getStackInSlot(s);
                if (!st.isEmpty())
                    infos.get(node).reach.add(good(st));
            }
    }

    private void goods() {
        Int2ObjectOpenHashMap<List<GraphBuilder.Edge>> outEdges = new Int2ObjectOpenHashMap<>();
        for (GraphBuilder.Edge e : g.edges())
            outEdges.computeIfAbsent(e.from, x -> new ArrayList<>()).add(e);
        int n = g.nodeCount();
        IntOpenHashSet[] reach = new IntOpenHashSet[n];
        java.util.ArrayDeque<Integer> work = new java.util.ArrayDeque<>();
        for (int x = 0; x < n; x++) {
            reach[x] = new IntOpenHashSet();
            Info i = infos.get(x);
            GraphBuilder.Node node = g.node(x);
            if (node.kind == GlassRecords.Node.STORE) {
                storeContents(node, reach[x]);
                work.add(x);
            } else if (i != null) {
                if (i.be != null && i.kind != K.BELT && i.kind != K.FAN)
                    seedContents(i, x);
                reach[x].addAll(i.reach);
                reach[x].addAll(i.out);
                // filters name goods the player meant to move
                if (i.be instanceof SmartBlockEntity sbe) {
                    FilteringBehaviour f = sbe.getBehaviour(FilteringBehaviour.TYPE);
                    if (f != null && !f.getFilter().isEmpty() && i.kind != K.SWITCH)
                        reach[x].add(good(f.getFilter()));
                }
                if (i.kind == K.DEPLOYER && i.node.kind == GlassRecords.Node.MACHINE) {
                    ItemStack h = held((DeployerBlockEntity) i.be);
                    if (!h.isEmpty()) {
                        int hg = good(h.copyWithCount(1));
                        reach[x].add(hg);
                        i.consumed.add(hg); // the held item and its refill stay in the deployer
                    }
                }
                if (!reach[x].isEmpty())
                    work.add(x);
            }
        }
        // Items on a belt reach its first mover; it is queued, or a later arrival of the same good (which
        // adds nothing) would never propagate past it.
        for (Belt b : belts)
            for (var t : b.controller.getInventory().getTransportedItems())
                if (!b.movers.isEmpty() && reach[b.movers.getInt(0)].add(good(t.stack)))
                    work.add(b.movers.getInt(0));
        int budget = 200_000;
        IntOpenHashSet[] emitted = new IntOpenHashSet[n];
        for (int x = 0; x < n; x++)
            emitted[x] = new IntOpenHashSet();
        while (!work.isEmpty() && budget-- > 0) {
            int x = work.poll();
            Info i = infos.get(x);
            IntOpenHashSet outs = emits(x, i, reach[x]);
            IntOpenHashSet fresh = new IntOpenHashSet();
            for (int gid : outs)
                if (emitted[x].add(gid))
                    fresh.add(gid);
            if (fresh.isEmpty())
                continue;
            for (GraphBuilder.Edge e : outEdges.getOrDefault(x, List.of())) {
                if (e.kind == GlassRecords.Edge.CONTROL || e.kind == GlassRecords.Edge.OBSERVE || e.kind == GlassRecords.Edge.HEAT)
                    continue;
                EdgeTest t = edgeTests.get(e);
                boolean added = false;
                for (int gid : fresh) {
                    if (t != null && !passes(t, gid))
                        continue;
                    if (reach[e.to].add(gid))
                        added = true;
                }
                if (added)
                    work.add(e.to);
            }
        }
        if (budget <= 0)
            g.notCompilable(GlassRecords.End.REASON_TOO_BIG, "goods propagation did not settle");
        // heat edges carry fuel goods' heat; fuel reaches burners through arms (already in reach)
        for (int x = 0; x < n; x++) {
            Info i = infos.get(x);
            if (i != null)
                i.reach.addAll(reach[x]);
        }
        // What each node hands on: what reaches it and what it emits (a machine's outputs).
        IntOpenHashSet[] handed = new IntOpenHashSet[n];
        for (int x = 0; x < n; x++) {
            handed[x] = new IntOpenHashSet(reach[x]);
            handed[x].addAll(emitted[x]);
        }
        // Materialize edge filters as GOOD lists over what their tails hand on.
        for (var en : edgeTests.entrySet()) {
            GraphBuilder.Edge e = en.getKey();
            if (e.kind == GlassRecords.Edge.OBSERVE) {
                for (int gid : reach[e.from])
                    if (passes(en.getValue(), gid))
                        e.filter(gid);
                continue;
            }
            IntArrayList pass = new IntArrayList(), deny = new IntArrayList();
            for (int gid : handed[e.from])
                (passes(en.getValue(), gid) ? pass : deny).add(gid);
            pass.sort(null);
            deny.sort(null);
            if (deny.isEmpty())
                continue;
            if (!pass.isEmpty()) {
                for (int gid : pass)
                    e.filter(gid);
            } else {
                e.flag(GlassRecords.Edge.DENY);
                for (int gid : deny)
                    e.filter(gid);
            }
        }
        collapseSequenced(handed);
    }

    /** Whether edge {@code e} carries {@code gid} by its materialized filter. */
    private static boolean carries(GraphBuilder.Edge e, int gid) {
        if (e.filter.isEmpty())
            return true;
        return e.filter.contains(gid) != ((e.flags & GlassRecords.Edge.DENY) != 0);
    }

    /**
     * A sequenced assembly becomes one SEQUENCED machine (§9.3, the solver's GB15): the machine doing step
     * 0 carries the whole sequence unrolled (its base input, every step's other inputs summed over the
     * unrolled steps, the last step's result pool), and the line's other machines lose their steps. Their
     * other feeders (a deployer's chute, a spout's pipe: the in-edges carrying none of the line's own
     * goods) move to that machine. Its rate is one product per the busiest machine's work, the sum of the
     * cycles of the steps it does: items on a loop pipeline, so the line runs at its slowest machine, not
     * at the sum of every step (GB40). Every machine of the line is PASS_THROUGH: what it no longer works
     * goes on. A sequence with a step no machine here does stays per step (it never finishes).
     */
    private void collapseSequenced(IntOpenHashSet[] handed) {
        if (seqSteps.isEmpty())
            return;
        TreeMap<String, TreeMap<Integer, Info>> lines = new TreeMap<>();
        for (Info i : infos.values())
            if (i.node != null)
                for (Craft c : i.crafts) {
                    SeqStep st = seqSteps.get(c.recipe());
                    if (st != null)
                        lines.computeIfAbsent(st.id(), k -> new TreeMap<>()).put(i.node.index, i);
                }
        for (var line : lines.entrySet()) {
            String id = line.getKey();
            List<Info> ms = new ArrayList<>(line.getValue().values());
            int total = -1;
            Info host = null;
            IntArrayList firsts = new IntArrayList();
            TreeMap<Integer, Integer> later = new TreeMap<>();
            for (Info m : ms)
                for (Craft c : m.crafts) {
                    SeqStep st = seqSteps.get(c.recipe());
                    if (st == null || !st.id().equals(id))
                        continue;
                    total = st.total();
                    if (st.step() == 0) {
                        if (!firsts.contains(c.recipe()))
                            firsts.add(c.recipe());
                        if (host == null)
                            host = m;
                    } else
                        later.putIfAbsent(st.step(), c.recipe());
                }
            if (host == null || total <= 0 || later.size() != total - 1)
                continue;
            IntOpenHashSet own = new IntOpenHashSet();
            for (int r : firsts)
                own.add(g.recipe(r).inputs.get(0).good());
            for (int r : later.values())
                own.add(g.recipe(r).inputs.get(0).good());
            GraphBuilder.Recipe last = later.isEmpty() ? null : g.recipe(later.lastEntry().getValue());
            IntArrayList unrolled = new IntArrayList();
            for (int r0 : firsts) {
                GraphBuilder.Recipe first = g.recipe(r0);
                int heat = first.heat;
                long duration = first.duration;
                // other inputs by (good, flags): consumed ones summed, catalysts once
                TreeMap<Long, Rat> extra = new TreeMap<>();
                List<GraphBuilder.Recipe> steps = new ArrayList<>();
                steps.add(first);
                for (int r : later.values())
                    steps.add(g.recipe(r));
                for (GraphBuilder.Recipe sr : steps) {
                    heat = Math.max(heat, sr.heat);
                    duration += sr == first ? 0 : sr.duration;
                    for (int k = 1; k < sr.inputs.size(); k++) {
                        GraphBuilder.Term t = sr.inputs.get(k);
                        long key = ((long) t.good() << 32) | (t.flags() & 0xFFFF_FFFFL);
                        Rat amt = Rat.of(t.num(), t.den());
                        boolean catalyst = (t.flags() & GlassRecords.Recipe.TERM_CATALYST) != 0;
                        extra.merge(key, amt, (x, y) -> catalyst ? (x.toDouble() >= y.toDouble() ? x : y) : x.add(y));
                    }
                }
                GraphBuilder.Recipe u = new GraphBuilder.Recipe(GlassRecords.RecipeType.SEQUENCED, heat,
                        (int) Math.min(Integer.MAX_VALUE, duration), g.name(id));
                u.in(first.inputs.get(0).good(), 1, 1);
                for (var e : extra.entrySet())
                    u.in((int) (e.getKey() >> 32), e.getValue().num(), e.getValue().den(), (int) (long) e.getKey());
                for (GraphBuilder.Term t : (last == null ? first : last).outputs)
                    u.out(t.good(), t.num(), t.den(), t.flags());
                int idx = g.recipe(u);
                if (!unrolled.contains(idx))
                    unrolled.add(idx);
            }
            // each machine's work per product: its steps' cycles, one per unrolled step it does
            java.util.IdentityHashMap<Info, List<Cyc>> work = new java.util.IdentityHashMap<>();
            for (Info m : ms) {
                IntOpenHashSet seen = new IntOpenHashSet();
                List<Cyc> cs = new ArrayList<>();
                for (Craft c : m.crafts) {
                    SeqStep st = seqSteps.get(c.recipe());
                    if (st != null && st.id().equals(id) && seen.add(st.step()) && c.cycle() != null)
                        cs.add(c.cycle());
                }
                work.put(m, cs);
            }
            final Info h = host;
            Cyc cycle = (rpm, n) -> {
                Rat worst = null;
                for (Info m : ms) {
                    float r = m == h ? rpm
                            : m.domain != NONE && m.domain == h.domain && !h.ratio.isZero()
                                    ? (float) Math.abs(rpm * m.ratio.toDouble() / h.ratio.toDouble()) : m.rpmNow;
                    if (m.kinetic && r <= 0)
                        return null;
                    Rat sum = Rat.ZERO;
                    for (Cyc c : work.get(m)) {
                        Rat t = c.ticks(r, 1);
                        if (t == null)
                            return null;
                        sum = sum.add(t);
                    }
                    if (worst == null || sum.toDouble() > worst.toDouble())
                        worst = sum;
                }
                return worst == null || worst.isZero() ? null : worst;
            };
            for (Info m : ms) {
                m.crafts.removeIf(c -> {
                    SeqStep st = seqSteps.get(c.recipe());
                    return st != null && st.id().equals(id);
                });
                m.node.flag(GlassRecords.Node.PASS_THROUGH);
                // a line machine left with no work of its own has no entry (unlimited), not a stop
                if (m != host && m.crafts.isEmpty())
                    m.noEntry = true;
            }
            for (int u : unrolled)
                host.crafts.add(new Craft(u, 1, cycle));
            host.node.model = GlassRecords.Model.SEQUENCED;
            for (GraphBuilder.Edge e : new ArrayList<>(g.edges())) {
                Info to = infos.get(e.to);
                if (to == null || to == host || !work.containsKey(to))
                    continue;
                if (e.kind == GlassRecords.Edge.CONTROL || e.kind == GlassRecords.Edge.OBSERVE || e.kind == GlassRecords.Edge.HEAT)
                    continue;
                boolean lineEdge = false;
                for (int gid : handed[e.from])
                    if (own.contains(gid) && carries(e, gid)) {
                        lineEdge = true;
                        break;
                    }
                if (!lineEdge)
                    g.retarget(e, host.node.index);
            }
        }
    }

    private boolean passes(EdgeTest t, int gid) {
        ItemStack is = itemProto.get(gid);
        if (is != null)
            return t.items() == null || t.items().test(is);
        FluidStack fs = fluidProto.get(gid);
        if (fs != null)
            return t.fluids() == null || t.fluids().test(fs);
        return true;
    }

    private void storeContents(GraphBuilder.Node node, IntOpenHashSet into) {
        BlockPos p = node.pos();
        if (!Loaded.at(level, p))
            return; // a capability read would load the chunk (and keep it loaded while compiles repeat)
        IItemHandler items = capAt(Capabilities.ItemHandler.BLOCK, p, null);
        if (items != null)
            for (int s = 0; s < items.getSlots(); s++) {
                ItemStack st = items.getStackInSlot(s);
                if (!st.isEmpty())
                    into.add(good(st));
            }
        IFluidHandler fluids = capAt(Capabilities.FluidHandler.BLOCK, p, null);
        if (fluids != null)
            for (int s = 0; s < fluids.getTanks(); s++) {
                FluidStack fs = fluids.getFluidInTank(s);
                if (!fs.isEmpty())
                    into.add(good(fs));
            }
    }

    /** What node {@code x} hands on given what reaches it; finds recipes for goods new to a machine. */
    private IntOpenHashSet emits(int x, Info i, IntOpenHashSet in) {
        GraphBuilder.Node node = g.node(x);
        if (node.kind == GlassRecords.Node.BLACK_BOX || node.kind == GlassRecords.Node.SINK
                || node.kind == GlassRecords.Node.CONTROLLER)
            return new IntOpenHashSet();
        if (i == null || i.pass == Pass.SAME)
            return in;
        if (i.pass == Pass.NONE_OUT) {
            if (i.kind == K.BURNER)
                for (int gid : in)
                    if (i.tried.add(gid))
                        Recipes.fuel(this, i, gid, itemProto.get(gid));
            return new IntOpenHashSet();
        }
        if (i.pass == Pass.SOURCE)
            return i.out;
        // MACHINE
        if (i.multiInput) {
            Recipes.multi(this, i, in);
        } else {
            for (int gid : in) {
                if (!i.tried.add(gid))
                    continue;
                ItemStack is = itemProto.get(gid);
                if (is != null)
                    Recipes.single(this, i, gid, is, in);
            }
        }
        IntOpenHashSet out = new IntOpenHashSet(i.out);
        if ((node.flags & GlassRecords.Node.PASS_THROUGH) != 0)
            for (int gid : in)
                if (!i.consumed.contains(gid))
                    out.add(gid);
        return out;
    }

    // ---- step 6: rates --------------------------------------------------------------------------

    /**
     * Hand-off loops (choice GB43): a machine that takes its next input only once empty (a saw's or crushing
     * wheels' ProcessingInventory; a fan zone on a depot, which holds one stack), fed by a downward chute
     * directly above it whose next stack waits parked at 0.5, has the loop's hand-offs on top of its own
     * cycle ({@link Cycles#chuteHandOff}). A saw or crusher hands its outputs on in its own tick; a depot
     * station needs the arm that takes them, whose move to the depot is part of the loop. Only when the
     * chute is the machine's one feeder and, for a depot, arms are its only takers; any other feeder or
     * taker (a funnel, a belt) is not modelled and leaves the cycle as it is (UNVERIFIED). A crafter grid
     * filled by one chute is a loop too, by tick order ({@link #crafterLoops}).
     */
    private void handOffs() {
        crafterLoops();
        for (Info i : infos.values()) {
            if (i.node == null || i.crafts.isEmpty())
                continue;
            BlockPos at;
            boolean self;
            if (i.kind == K.SAW || i.kind == K.CRUSHER) {
                at = i.be.getBlockPos();
                self = true;
            } else if (i.kind == K.FAN && i.extra instanceof Object[] x && x[1] == null && x[2] instanceof BlockPos host) {
                at = host;
                self = false;
            } else {
                continue;
            }
            Info chute = null;
            boolean other = false;
            List<Info> takers = new ArrayList<>();
            for (GraphBuilder.Edge e : g.edges()) {
                if (e.kind == GlassRecords.Edge.CONTROL || e.kind == GlassRecords.Edge.OBSERVE || e.kind == GlassRecords.Edge.HEAT)
                    continue;
                if (e.to == i.node.index) {
                    Info f = infos.get(e.from);
                    if (f != null && f.kind == K.CHUTE && chute == null && f.be.getBlockPos().equals(at.above())
                            && f.extra instanceof Float m && m < 0)
                        chute = f;
                    else
                        other = true;
                } else if (e.from == i.node.index && !self) {
                    Info t = infos.get(e.to);
                    if (t != null && t.kind == K.ARM)
                        takers.add(t);
                    else
                        other = true;
                }
            }
            if (chute == null || other || (!self && takers.size() != 1))
                continue;
            float motion = (Float) chute.extra;
            if (self) {
                Cycles.Ratio h = Cycles.chuteHandOff(motion, 0, true);
                Rat r = Rat.of(h.num(), h.den());
                i.handOff = l -> r;
                continue;
            }
            Info arm = takers.get(0);
            i.handOff = l -> {
                float rpm = arm.domain == NONE ? arm.rpmNow
                        : rpmAt(arm, arm.domain == i.domain ? l : Math.max(0, g.domains().get(arm.domain).current));
                if (!(rpm > 0))
                    return null;
                Cycles.Ratio h = Cycles.chuteHandOff(motion, Cycles.armMoveTicks(rpm), false);
                return Rat.of(h.num(), h.den());
            };
        }
    }

    /**
     * Crafter grids whose one feeder is a downward chute directly above one of their crafters (choice GB43):
     * the grid's cycle becomes the loop's ({@link Cycles#crafterChuteLoop}), from the crafters' arrows and
     * the level's tick order ({@link MinecraftGlassAccess#tickOrder}, one pass over the level's tickers for
     * every loop of the compile; unknown places count the mean). The order is the one in force at compile:
     * it changes only when a block entity is replaced or its chunk reloads, which the fingerprint sees
     * (a crafter's identity, GB59). Another feeder (a funnel, an arm, a second chute) leaves the grid's own
     * cycle, which assumes the next batch is there at once (UNVERIFIED).
     */
    private void crafterLoops() {
        record Loop(Info grid, Info chute, List<MechanicalCrafterBlockEntity> chain) {}
        List<Loop> loops = new ArrayList<>();
        LongOpenHashSet wanted = new LongOpenHashSet();
        for (Info i : infos.values()) {
            if (i.node == null || i.kind != K.CRAFTER || i.crafts.isEmpty() || !(i.extra instanceof List<?> raw))
                continue;
            List<MechanicalCrafterBlockEntity> chain = new ArrayList<>();
            LongOpenHashSet above = new LongOpenHashSet();
            for (Object o : raw)
                if (o instanceof MechanicalCrafterBlockEntity m) {
                    chain.add(m);
                    above.add(m.getBlockPos().above().asLong());
                }
            Info chute = null;
            boolean other = false;
            for (GraphBuilder.Edge e : g.edges()) {
                if (e.to != i.node.index || e.kind == GlassRecords.Edge.CONTROL || e.kind == GlassRecords.Edge.OBSERVE
                        || e.kind == GlassRecords.Edge.HEAT)
                    continue;
                Info f = infos.get(e.from);
                if (f != null && f.kind == K.CHUTE && chute == null && above.contains(f.be.getBlockPos().asLong())
                        && f.extra instanceof Float m && m < 0)
                    chute = f;
                else
                    other = true;
            }
            if (chute == null || other || !chain.contains(i.be))
                continue;
            loops.add(new Loop(i, chute, chain));
            for (MechanicalCrafterBlockEntity m : chain)
                wanted.add(m.getBlockPos().asLong());
            wanted.add(chute.be.getBlockPos().asLong());
        }
        if (loops.isEmpty())
            return;
        Long2IntOpenHashMap order = MinecraftGlassAccess.tickOrder(level, wanted);
        for (Loop lp : loops) {
            int n = lp.chain().size();
            Long2IntOpenHashMap index = new Long2IntOpenHashMap();
            for (int k = 0; k < n; k++)
                index.put(lp.chain().get(k).getBlockPos().asLong(), k);
            int[][] preds = new int[n][];
            int[] ord = new int[n];
            for (int k = 0; k < n; k++) {
                MechanicalCrafterBlockEntity m = lp.chain().get(k);
                IntArrayList ps = new IntArrayList();
                for (MechanicalCrafterBlockEntity q : RecipeGridHandler.getPrecedingCrafters(m))
                    if (index.containsKey(q.getBlockPos().asLong()))
                        ps.add(index.get(q.getBlockPos().asLong()));
                preds[k] = ps.toIntArray();
                ord[k] = order == null ? -1 : order.get(m.getBlockPos().asLong());
            }
            int fin = index.get(lp.grid().be.getBlockPos().asLong());
            int chuteOrd = order == null ? -1 : order.get(lp.chute().be.getBlockPos().asLong());
            float motion = (Float) lp.chute().extra;
            lp.grid().crafts.replaceAll(c -> c.cycle() == null ? c : new Craft(c.recipe(), c.batch(), (rpm, b) -> {
                Cycles.Ratio r = Cycles.crafterChuteLoop(preds, fin, ord, chuteOrd, motion, rpm);
                return r == null ? c.cycle().ticks(rpm, b) : Rat.of(r.num(), r.den());
            }));
        }
    }

    private void rates() {
        handOffs();
        // A belt's stack: the smallest batch its feeders hand on (64 when unknown).
        for (Belt b : belts) {
            int s = 64;
            for (GraphBuilder.Edge e : g.edges())
                if (b.movers.contains(e.to)) {
                    Info f = infos.get(e.from);
                    if (f != null && f.kind != K.BELT && f.unitBatch > 0)
                        s = Math.min(s, f.unitBatch);
                }
            b.stack = s;
            for (GraphBuilder.Edge e : g.edges())
                if (andesiteMovers.contains(e.to) && b.movers.contains(e.to) && e.kind != GlassRecords.Edge.CONTROL)
                    e.param(s); // the andesite tunnel's entry size n (§9.3: its first in-edge's param)
            for (int m : b.movers) {
                Info mi = infos.get(m);
                mi.moverBatch = s;
                mi.unitBatch = s;
                mi.mover = (rpm, bb) -> {
                    int moves = Cycles.beltInsertMoves(rpm);
                    return moves <= 0 ? null : Rat.of(moves, 1);
                };
            }
        }
        // A fan zone on a moving belt works in place: a stack of n is done after fanTicks(n) ticks in the
        // zone, so it is BOUNDED only when the largest stack that can reach it leaves sooner (GB44).
        int fanTime = fanProcessingTime();
        for (Info i : infos.values())
            if (i.kind == K.FAN && i.node != null && i.extra instanceof Object[] x && x.length > 1 && x[1] instanceof Belt b
                    && i.crafts.stream().anyMatch(c -> c.cycle() == null)
                    && fanExposure(i) < Cycles.fanTicks(stackAt(b, i.node.index), fanTime))
                bounded(i.node);
        for (Info i : infos.values()) {
            if (i.node == null)
                continue;
            int levels = i.domain == NONE ? 1 : g.domains().get(i.domain).levels.size();
            boolean any = false;
            for (int l = 0; l < levels; l++) {
                float rpm = rpmAt(i, l);
                if (i.kinetic && rpm <= 0)
                    continue;
                if (i.mover != null) {
                    if (i.moverBatch <= 0)
                        continue;
                    Rat t = safe(i.mover, rpm, i.moverBatch);
                    if (t != null) {
                        i.node.rate(l, NONE, i.moverBatch, t, GlassRecords.Node.RATE_SHARED);
                        any = true;
                    }
                }
                for (Craft c : i.crafts) {
                    if (c.cycle() == null) {
                        // in place or endless (a fan zone on a moving belt, a creative source, a water
                        // source): an unlimited entry, so that the recipe is the node's (§9.3: a machine's
                        // recipes are exactly those its entries name)
                        i.node.unlimited(l, c.recipe());
                        any = true;
                        continue;
                    }
                    int n = c.batch() == FEEDER_BATCH ? feederBatch(i) : c.batch();
                    Rat t = safe(c.cycle(), rpm, n);
                    if (t != null && i.handOff != null) {
                        Rat h = i.handOff.apply(l);
                        t = h == null ? null : t.add(h);
                    }
                    if (t != null && n > 0) {
                        i.node.rate(l, c.recipe(), n, t, 0);
                        any = true;
                    }
                }
            }
            boolean limited = !i.noEntry && (i.mover != null || i.crafts.stream().anyMatch(c -> c.cycle() != null)
                    || (i.kinetic && i.node.kind == GlassRecords.Node.MACHINE));
            if (!any && limited)
                i.node.stopped(0);
        }
    }

    private Rat safe(Cyc c, float rpm, int batch) {
        try {
            return c.ticks(rpm, batch);
        } catch (RuntimeException e) {
            return null;
        }
    }

    /**
     * The stack a whole-stack machine is handed: the smallest batch its feeders hand on (a belt's stack, a
     * funnel's amount, a chute's 16), 1 when nothing feeds it. A saw takes one item unless bulkCutting.
     */
    private int feederBatch(Info i) {
        if (i.kind == K.SAW && !AllConfigs.server().recipes.bulkCutting.get())
            return 1;
        int n = Integer.MAX_VALUE;
        for (GraphBuilder.Edge e : g.edges())
            if (e.to == i.node.index && e.kind != GlassRecords.Edge.CONTROL && e.kind != GlassRecords.Edge.HEAT
                    && e.kind != GlassRecords.Edge.OBSERVE) {
                Info f = infos.get(e.from);
                int u = f == null ? 64 : f.unitBatch;
                if (u > 0)
                    n = Math.min(n, u);
            }
        return n == Integer.MAX_VALUE ? 1 : Math.min(64, n);
    }

    private static boolean goodsEdge(GraphBuilder.Edge e) {
        return e.kind != GlassRecords.Edge.CONTROL && e.kind != GlassRecords.Edge.HEAT && e.kind != GlassRecords.Edge.OBSERVE;
    }

    /**
     * The largest stack a whole-stack machine can be handed: the largest batch its feeders hand on, 64 for a
     * feeder whose batch is not a mover's own (a belt, a machine, a drop); a saw takes one unless bulkCutting.
     */
    private int maxFeed(Info i) {
        if (i.kind == K.SAW && !AllConfigs.server().recipes.bulkCutting.get())
            return 1;
        int n = 0;
        for (GraphBuilder.Edge e : g.edges())
            if (e.to == i.node.index && goodsEdge(e)) {
                Info f = infos.get(e.from);
                boolean own = f != null && f.kind != K.BELT && f.node != null && f.node.kind == GlassRecords.Node.MOVER
                        && e.kind != GlassRecords.Edge.DROP && f.unitBatch > 0;
                n = Math.max(n, own ? f.unitBatch : 64);
            }
        return n == 0 ? 1 : Math.min(64, n);
    }

    /**
     * The largest stack that reaches element {@code z} of belt {@code b}'s chain (GB44): the largest any of
     * its entries' feeders hands on ({@link #handOnPeak}), times the most of one good a craft makes at each
     * machine with recipes before {@code z}; 64 at most.
     */
    private int stackAt(Belt b, int z) {
        IntOpenHashSet chain = new IntOpenHashSet();
        for (List<Integer> at : b.elements.values())
            chain.addAll(at);
        List<GraphBuilder.Edge> feeds = new ArrayList<>();
        for (GraphBuilder.Edge e : g.edges())
            if (b.movers.contains(e.to) && !chain.contains(e.from) && goodsEdge(e))
                feeds.add(e);
        long peak = 0;
        for (List<Integer> at : b.elements.values())
            for (int n : at) {
                if (n == z)
                    return (int) Math.max(1, peak);
                if (b.movers.contains(n)) {
                    for (GraphBuilder.Edge e : feeds)
                        if (e.to == n)
                            peak = Math.max(peak, handOnPeak(infos.get(e.from), e, feeds.size() > 1));
                } else {
                    Info s = infos.get(n);
                    int grow = 1;
                    if (s != null)
                        for (Craft c : s.crafts)
                            grow = Math.max(grow, recipePeak.getOrDefault(c.recipe(), 64));
                    peak = Math.min(64, peak * grow);
                }
            }
        return 64;
    }

    /**
     * The largest stack feeder {@code f} hands onto a belt by edge {@code e} (GB44), 64 when unknown: a
     * mover's batch, a machine's largest batch times the most of one good a craft makes. A hand-off that
     * may come as item entities (a DROP, or any machine's output) can wait on the belt and merge with the
     * next of its kind, so its stack counts only when it is the belt's one feeder and the belt takes an
     * operation's stacks before the next operation ends, at every level of both domains.
     */
    private int handOnPeak(Info f, GraphBuilder.Edge e, boolean shared) {
        if (f == null || f.node == null || f.kind == K.BELT)
            return 64;
        boolean mover = f.node.kind == GlassRecords.Node.MOVER;
        if (!mover && (f.node.kind != GlassRecords.Node.MACHINE || (f.node.flags & GlassRecords.Node.PASS_THROUGH) != 0
                || f.crafts.isEmpty()))
            return 64;
        int levels = f.domain == NONE ? 1 : g.domains().get(f.domain).levels.size();
        long peak = 0;
        int stacks = 0;
        double fastest = Double.POSITIVE_INFINITY;
        if (mover) {
            peak = f.unitBatch <= 0 ? 64 : f.unitBatch;
            if (e.kind != GlassRecords.Edge.DROP)
                return (int) Math.min(64, peak); // put down whole: nothing to merge with
            stacks = 1;
            for (int l = 0; l < levels && f.mover != null && f.moverBatch > 0; l++) {
                Rat t = safe(f.mover, rpmAt(f, l), f.moverBatch);
                if (t != null)
                    fastest = Math.min(fastest, t.toDouble());
            }
        } else {
            int fed = maxFeed(f);
            for (Craft c : f.crafts) {
                if (c.cycle() == null)
                    return 64; // in place or endless: no operation to count
                int n = c.batch() == FEEDER_BATCH ? fed : c.batch();
                peak = Math.max(peak, (long) n * recipePeak.getOrDefault(c.recipe(), 64));
                stacks = Math.max(stacks, g.recipe(c.recipe()).outputs.size());
                for (int l = 0; l < levels; l++) {
                    float rpm = rpmAt(f, l);
                    if (f.kinetic && rpm <= 0)
                        continue;
                    Rat t = safe(c.cycle(), rpm, n);
                    if (t != null && f.handOff != null) {
                        Rat h = f.handOff.apply(l);
                        t = h == null ? null : t.add(h);
                    }
                    if (t != null)
                        fastest = Math.min(fastest, t.toDouble());
                }
            }
        }
        if (shared)
            return 64;
        Info m = infos.get(e.to);
        int ml = m.domain == NONE ? 1 : g.domains().get(m.domain).levels.size();
        int intake = 0;
        for (int l = 0; l < ml; l++) {
            float rpm = rpmAt(m, l);
            if (rpm > 0)
                intake = Math.max(intake, Cycles.beltInsertMoves(rpm));
        }
        return (long) stacks * intake > fastest ? 64 : (int) Math.min(64, peak);
    }

    /** The node's speed at its domain's level {@code l}: |ratio x level speed|, as Create's float. */
    float rpmAt(Info i, int l) {
        if (i.domain == NONE)
            return i.kinetic ? i.rpmNow : 0;
        Rat sp = levelSpeeds.get(i.domain).get(l);
        return (float) Math.abs(i.ratio.toDouble() * sp.toDouble());
    }

    // ---- step 7: members, the rest ---------------------------------------------------------------

    private void finishUp() {
        // Domain members: every block entity of each network (outside the district too: its stress
        // counts), with its node when it is one, else merged by ratio and impact.
        Long2IntOpenHashMap nodeAtPos = intMap();
        for (Info i : infos.values())
            if (i.be != null && i.kinetic && i.node != null && i.node.domain != NONE && !nodeAtPos.containsKey(i.be.getBlockPos().asLong()))
                nodeAtPos.put(i.be.getBlockPos().asLong(), i.node.index);
        long[] keys = kin.keySet().toLongArray();
        java.util.Arrays.sort(keys);
        for (long pos : keys) {
            Kin k = kin.get(pos);
            GraphBuilder.Domain d = g.domains().get(k.domain());
            d.member(nodeAtPos.get(pos), k.ratio(), memberImpact.get(pos));
        }
        for (int b : orphanBuffers)
            g.node(b).flag(GlassRecords.Node.BOUNDED);
    }

    // ---- shared with Recipes --------------------------------------------------------------------

    /**
     * Per recipe, per good: how much more than its mean count one craft can hand on (a chance output's
     * count less its share; one of a saw's k averaged recipes). Read only by {@link #peakPerCraft}.
     */
    private final java.util.IdentityHashMap<GraphBuilder.Recipe, it.unimi.dsi.fastutil.ints.Int2DoubleOpenHashMap> outSlack = new java.util.IdentityHashMap<>();

    /** Recipe index to the most of one item good one craft of it hands on (64 at most, 64 when unknown). */
    private final it.unimi.dsi.fastutil.ints.Int2IntOpenHashMap recipePeak = new it.unimi.dsi.fastutil.ints.Int2IntOpenHashMap();

    void outSlack(GraphBuilder.Recipe r, int good, double slack) {
        outSlack.computeIfAbsent(r, x -> new it.unimi.dsi.fastutil.ints.Int2DoubleOpenHashMap()).addTo(good, slack);
    }

    /** The most of one item good a craft of {@code r} hands on: its mean count plus its slack, rounded up. */
    private int peakPerCraft(GraphBuilder.Recipe r) {
        it.unimi.dsi.fastutil.ints.Int2DoubleOpenHashMap slack = outSlack.get(r);
        it.unimi.dsi.fastutil.ints.Int2DoubleOpenHashMap per = new it.unimi.dsi.fastutil.ints.Int2DoubleOpenHashMap();
        for (GraphBuilder.Term t : r.outputs) {
            if (fluidProto.get(t.good()) != null)
                continue; // fluids never make a stack on a belt
            if ((t.flags() & GlassRecords.Recipe.TERM_CHANCE) != 0 && (slack == null || !slack.containsKey(t.good())))
                return 64; // a chance output whose count is not known
            per.addTo(t.good(), t.num() / (double) t.den());
        }
        double peak = 0;
        for (var en : per.int2DoubleEntrySet())
            peak = Math.max(peak, en.getDoubleValue() + (slack == null ? 0 : slack.get(en.getIntKey())));
        return (int) Math.min(64, Math.ceil(peak - 1e-9));
    }

    void addCraft(Info i, GraphBuilder.Recipe r, int batch, Cyc cycle, int consumedGood) {
        int idx = g.recipe(r);
        int peak = peakPerCraft(r);
        if (!recipePeak.containsKey(idx) || recipePeak.get(idx) < peak)
            recipePeak.put(idx, peak);
        for (Craft c : i.crafts)
            if (c.recipe() == idx)
                return;
        i.crafts.add(new Craft(idx, batch, cycle));
        for (GraphBuilder.Term t : r.outputs)
            i.out.add(t.good());
        if (consumedGood != NONE)
            i.consumed.add(consumedGood);
    }

    Cyc cycle(Info i, int duration, int batch) {
        return switch (i.kind) {
            case MILLSTONE -> (rpm, n) -> ticks(Cycles.millstone(duration, rpm));
            case CRUSHER -> (rpm, n) -> ticks(Cycles.crusher(duration, n, rpm));
            case SAW -> (rpm, n) -> ticks(Cycles.saw(duration, n, rpm));
            case PRESS -> {
                if (i.extra instanceof BasinBlockEntity)
                    yield (rpm, n) -> ticks(Cycles.pressBasin(rpm));
                if (i.extra instanceof Belt belt)
                    yield (rpm, n) -> ticks(Cycles.pressBelt(rpm, beltRpm(belt, i, rpm)));
                yield (rpm, n) -> ticks(Cycles.pressDepot(rpm));
            }
            case MIXER -> {
                float medium = AllConfigs.server().kinetics.mediumSpeed.getF();
                yield (rpm, n) -> ticks(Cycles.mixerSteady(duration, rpm, medium));
            }
            case DEPLOYER -> {
                if (i.extra instanceof Belt belt)
                    yield (rpm, n) -> ticks(Cycles.deployerBelt(rpm, beltRpm(belt, i, rpm)));
                yield (rpm, n) -> ticks(Cycles.deployerDepot(rpm));
            }
            case SPOUT -> {
                if (i.extra instanceof Belt belt)
                    yield (rpm, n) -> belt.info.rpmNow > 0 ? ticks(Cycles.spoutBelt(belt.info.rpmNow)) : null;
                yield (rpm, n) -> ticks(Cycles.spout());
            }
            case DRAIN -> (rpm, n) -> ticks(Cycles.drain());
            case CRAFTER -> {
                int depth = crafterDepth((MechanicalCrafterBlockEntity) i.be);
                yield (rpm, n) -> ticks(Cycles.crafter(depth, rpm));
            }
            case FURNACE -> (rpm, n) -> Rat.of(Math.max(1, duration), 1);
            default -> null; // a fan zone on a moving belt: in place, unlimited (Recipes.fan sets a depot's)
        };
    }

    /**
     * The belt's speed when a station on it turns at {@code rpm}: the same domain scales both by their
     * ratios; a belt on another network keeps its current speed.
     */
    private static float beltRpm(Belt belt, Info station, float rpm) {
        if (belt.info.domain != NONE && belt.info.domain == station.domain && !station.ratio.isZero())
            return (float) Math.abs(rpm * belt.info.ratio.toDouble() / station.ratio.toDouble());
        return belt.info.rpmNow;
    }

    private static Rat ticks(int t) {
        return t <= 0 ? null : Rat.of(t, 1);
    }

    int fanProcessingTime() {
        return AllConfigs.server().kinetics.fanProcessingTime.get();
    }

    /** Ticks a stack of up to {@code n} spends in the zones of {@code type} along its belt at the current speed. */
    double fanExposure(Info i) {
        Object[] x = (Object[]) i.extra;
        Belt b = (Belt) x[1];
        if (b == null)
            return Double.POSITIVE_INFINITY; // a depot holds it until something takes it
        FanProcessingType type = (FanProcessingType) x[0];
        int segs = 0;
        for (BlockPos p : b.segments) {
            List<Object[]> fs = fanAt.get(p.asLong());
            if (fs != null && fs.stream().anyMatch(f -> f[1] == type))
                segs++;
        }
        float v = Cycles.beltSpeed(b.info.rpmNow);
        return v <= 0 ? 0 : segs / (double) v;
    }

    ItemStack itemProto(int good) {
        return itemProto.get(good);
    }

    FluidStack fluidProto(int good) {
        return fluidProto.get(good);
    }

    /** A blaze burner under the basin a basin machine works. */
    boolean hasHeater(Info i) {
        return i.extra instanceof BasinBlockEntity basin && blockAt(basin.getBlockPos().below()) instanceof BlazeBurnerBlockEntity;
    }

    int boundedNodes() {
        return boundedNodes;
    }

    void markBounded(Info i) {
        bounded(i.node);
    }

    Collection<Info> infoValues() {
        return infos.values();
    }

    static final Comparator<Integer> BY_INT = Integer::compare;
}
