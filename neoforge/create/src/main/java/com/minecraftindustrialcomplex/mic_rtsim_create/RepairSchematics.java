package com.minecraftindustrialcomplex.mic_rtsim_create;

import com.minecraftindustrialcomplex.mic_rtsim.industry.DestroyedBlock;
import com.minecraftindustrialcomplex.mic_rtsim.industry.District;
import com.minecraftindustrialcomplex.mic_rtsim.industry.Industry;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndustryPlugin;
import com.simibubi.create.AllBlocks;
import com.simibubi.create.AllDataComponents;
import com.simibubi.create.content.kinetics.belt.BeltBlock;
import com.simibubi.create.content.kinetics.belt.BeltBlockEntity;
import com.simibubi.create.content.schematics.SchematicItem;
import com.simibubi.create.foundation.utility.BlockHelper;
import com.simibubi.create.foundation.utility.CreatePaths;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Vec3i;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.IntTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Mirror;
import net.minecraft.world.level.block.Rotation;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;

import java.io.IOException;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.function.Consumer;

/**
 * Repair schematics (I1, Ben 2026-10-20; docs/choices-repair.md R10 to R20). After a reconcile applied an
 * unwatched attack's damage, the blocks it destroyed (and only those: never a player's breaks) go into a
 * Create schematic with their states from before, in Create's own format (a gzipped structure template)
 * and Create's own item, deployed and anchored where they stood, so it loads straight into a
 * Schematicannon and prints the holes back. No new item (D3).
 *
 * <ul>
 *   <li><b>File</b> (R12): {@code schematics/uploaded/mic-rtsim/repair_d<district>_<gameTime>.nbt}, under
 *       Create's {@code UPLOADED_SCHEMATICS_DIR}. {@code mic-rtsim} is not a valid player name, so no upload
 *       ever trims the folder (Create deletes only the uploader's own oldest file past
 *       {@code maxSchematics}).</li>
 *   <li><b>Contents</b> (R13): the destroyed positions only (a sparse template: the cannon skips the cells
 *       left out), each with its state before and the block-entity data the cannon would place itself
 *       ({@code BlockHelper.prepareBlockEntityData}); a belt also keeps its length, index, controller and
 *       casing, which the cannon reads to lay the belt again. No inventory is ever written.</li>
 *   <li><b>One schematic per district per wave of damage</b> (R14): what a district loses within
 *       {@link #QUIET_TICKS} of its last loss (all the chunks that load as a player arrives, every attack
 *       while away) is one schematic. Damage after that is a new one.</li>
 *   <li><b>Delivery</b> (R15): the item lies where the damage is, at the destroyed block nearest the
 *       middle of the damage, among the drops, and never despawns.</li>
 * </ul>
 */
public final class RepairSchematics implements IndustryPlugin {

    /** The folder (Create's schematic "owner") the sim's schematics live in. Not a valid player name. */
    public static final String OWNER = "mic-rtsim";

    /** Damage to a district within this many ticks of its last damage goes into the same schematic (R14). */
    static final int QUIET_TICKS = 100;

    /** One delivered repair schematic (GameTests). */
    public record Delivered(long district, String file, Path path, BlockPos anchor, Vec3i size, List<DestroyedBlock> blocks, ItemEntity item) {}

    private static final List<Delivered> DELIVERED = new CopyOnWriteArrayList<>();

    /** Every repair schematic delivered since the server started, in order (GameTests). */
    public static List<Delivered> delivered() {
        return DELIVERED;
    }

    private static final class Pending {
        final ResourceKey<Level> dim;
        final Map<BlockPos, DestroyedBlock> blocks = new LinkedHashMap<>();
        long last;

        Pending(ResourceKey<Level> dim) {
            this.dim = dim;
        }
    }

    private final Map<Long, Pending> pending = new HashMap<>();

    // ---- before the break ---------------------------------------------------------------------

    /**
     * A Create belt goes as a whole: breaking one segment turns the others to air and its pulleys to
     * shafts ({@code BeltBlock.onRemove}). Its segments, walked from the controller in loaded chunks only.
     */
    @Override
    public void alsoBroken(ServerLevel level, BlockPos pos, BlockState state, Consumer<BlockPos> out) {
        if (!AllBlocks.BELT.has(state) || !(level.getBlockEntity(pos) instanceof BeltBlockEntity belt) || belt.getController() == null)
            return;
        BlockPos at = belt.getController();
        for (int i = 0; i < 1000 && at != null && loaded(level, at); i++) {
            BlockState s = level.getBlockState(at);
            if (!AllBlocks.BELT.has(s))
                break;
            out.accept(at);
            at = BeltBlock.nextSegmentPosition(s, at, true);
        }
    }

    /**
     * What the cannon places for this block ({@code prepareBlockEntityData}: the data of Create's safe-NBT
     * blocks and the safe behaviours of its smart block entities), and for a belt the fields the cannon reads
     * to lay it again. Never an inventory.
     */
    @Override
    public CompoundTag rebuildData(ServerLevel level, BlockPos pos, BlockState state, BlockEntity be) {
        CompoundTag data = BlockHelper.prepareBlockEntityData(level, state, be);
        if (be instanceof BeltBlockEntity belt) {
            CompoundTag full = belt.saveWithoutMetadata(level.registryAccess());
            CompoundTag keep = data == null ? new CompoundTag() : data.copy();
            for (String k : new String[] {"Controller", "IsController", "Length", "Index", "Casing", "Covered", "Dye"})
                if (full.contains(k))
                    keep.put(k, full.get(k).copy());
            data = keep;
        }
        if (data != null) {
            data.remove("x");
            data.remove("y");
            data.remove("z");
        }
        return data;
    }

    // ---- after the break ----------------------------------------------------------------------

    @Override
    public void destroyed(Industry industry, ServerLevel level, District district, List<DestroyedBlock> blocks, long now) {
        Pending p = pending.computeIfAbsent(district.id, k -> new Pending(level.dimension()));
        for (DestroyedBlock b : blocks)
            p.blocks.putIfAbsent(b.pos(), b);
        p.last = now;
    }

    @Override
    public void tick(Industry industry, long now) {
        if (pending.isEmpty())
            return;
        for (Iterator<Map.Entry<Long, Pending>> it = pending.entrySet().iterator(); it.hasNext(); ) {
            Map.Entry<Long, Pending> e = it.next();
            Pending p = e.getValue();
            if (now - p.last < QUIET_TICKS && now >= p.last)
                continue;
            ServerLevel level = industry.server().getLevel(p.dim);
            if (level == null) {
                it.remove();
                continue;
            }
            BlockPos drop = dropAt(level, p.blocks.keySet());
            if (drop == null)
                continue; // no destroyed block in a loaded chunk: wait until there is one
            it.remove();
            deliver(level, e.getKey(), p, drop, now);
        }
    }

    /** The destroyed position in a loaded chunk nearest the middle of all of them, or null. */
    private static BlockPos dropAt(ServerLevel level, java.util.Set<BlockPos> positions) {
        double x = 0, y = 0, z = 0;
        for (BlockPos q : positions) {
            x += q.getX();
            y += q.getY();
            z += q.getZ();
        }
        int n = positions.size();
        x /= n;
        y /= n;
        z /= n;
        BlockPos best = null;
        double bestD = Double.MAX_VALUE;
        for (BlockPos q : positions) {
            if (!loaded(level, q))
                continue;
            double d = q.distToCenterSqr(x + 0.5, y + 0.5, z + 0.5);
            if (d < bestD) {
                bestD = d;
                best = q;
            }
        }
        return best;
    }

    private void deliver(ServerLevel level, long district, Pending p, BlockPos drop, long now) {
        List<DestroyedBlock> blocks = new ArrayList<>(p.blocks.values());
        int minX = Integer.MAX_VALUE, minY = Integer.MAX_VALUE, minZ = Integer.MAX_VALUE;
        int maxX = Integer.MIN_VALUE, maxY = Integer.MIN_VALUE, maxZ = Integer.MIN_VALUE;
        for (DestroyedBlock b : blocks) {
            BlockPos q = b.pos();
            minX = Math.min(minX, q.getX());
            minY = Math.min(minY, q.getY());
            minZ = Math.min(minZ, q.getZ());
            maxX = Math.max(maxX, q.getX());
            maxY = Math.max(maxY, q.getY());
            maxZ = Math.max(maxZ, q.getZ());
        }
        BlockPos anchor = new BlockPos(minX, minY, minZ);
        Vec3i size = new Vec3i(maxX - minX + 1, maxY - minY + 1, maxZ - minZ + 1);
        String file = "repair_d" + district + "_" + now + ".nbt";
        Path path = CreatePaths.UPLOADED_SCHEMATICS_DIR.resolve(OWNER).resolve(file);
        try {
            Files.createDirectories(path.getParent());
            try (OutputStream out = Files.newOutputStream(path)) {
                NbtIo.writeCompressed(template(blocks, anchor, size), out);
            }
        } catch (IOException ex) {
            MicRtsimCreate.LOGGER.error("[mic_rtsim_create] repair schematic {} could not be written", path, ex);
            return;
        }
        ItemStack stack = SchematicItem.create(level, file, OWNER);
        stack.set(AllDataComponents.SCHEMATIC_DEPLOYED, true);
        stack.set(AllDataComponents.SCHEMATIC_ANCHOR, anchor);
        stack.set(AllDataComponents.SCHEMATIC_ROTATION, Rotation.NONE);
        stack.set(AllDataComponents.SCHEMATIC_MIRROR, Mirror.NONE);
        ItemEntity item = new ItemEntity(level, drop.getX() + 0.5, drop.getY() + 0.5, drop.getZ() + 0.5, stack, 0, 0, 0);
        item.setUnlimitedLifetime();
        level.addFreshEntity(item);
        DELIVERED.add(new Delivered(district, file, path, anchor, size, List.copyOf(blocks), item));
        MicRtsimCreate.LOGGER.info("[mic_rtsim_create] repair schematic {}: {} blocks of district {}, anchored at {}, left at {}",
                file, blocks.size(), district, anchor.toShortString(), drop.toShortString());
    }

    /** A structure template (Create reads it with {@code StructureTemplate.load}) of just these blocks. */
    static CompoundTag template(List<DestroyedBlock> blocks, BlockPos anchor, Vec3i size) {
        ListTag palette = new ListTag();
        Map<BlockState, Integer> ids = new HashMap<>();
        ListTag list = new ListTag();
        for (DestroyedBlock b : blocks) {
            Integer id = ids.get(b.state());
            if (id == null) {
                id = palette.size();
                ids.put(b.state(), id);
                palette.add(NbtUtils.writeBlockState(b.state()));
            }
            CompoundTag e = new CompoundTag();
            BlockPos q = b.pos().subtract(anchor);
            e.put("pos", ints(q.getX(), q.getY(), q.getZ()));
            e.putInt("state", id);
            if (b.rebuildData() != null)
                e.put("nbt", b.rebuildData().copy());
            list.add(e);
        }
        CompoundTag t = new CompoundTag();
        t.putInt("DataVersion", SharedConstants.getCurrentVersion().getDataVersion().getVersion());
        t.put("size", ints(size.getX(), size.getY(), size.getZ()));
        t.put("palette", palette);
        t.put("blocks", list);
        t.put("entities", new ListTag());
        return t;
    }

    private static ListTag ints(int... v) {
        ListTag l = new ListTag();
        for (int i : v)
            l.add(IntTag.valueOf(i));
        return l;
    }

    private static boolean loaded(ServerLevel level, BlockPos pos) {
        return level.getChunkSource().getChunkNow(pos.getX() >> 4, pos.getZ() >> 4) != null;
    }
}
