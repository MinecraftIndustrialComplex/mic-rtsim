package com.minecraftindustrialcomplex.mic_rtsim_create;

import net.minecraft.core.BlockPos;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.saveddata.SavedData;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

/**
 * The repair schematic files this world wrote, {@code data/mic_rtsim_create_repairs.dat}, and their retention
 * (docs/choices-repair.md R25, Ben's "For Ben" item 2). The files live in the game directory (R12), which every
 * world of a single-player install shares, so only the files listed here are ever deleted.
 *
 * <p><b>The rule:</b> when a new repair schematic of a district is delivered, each earlier one of the same
 * district is deleted once every block it holds is back (the same block at each position: what R10 counts as
 * destroyed, undone), read in loaded chunks only. Such a schematic prints nothing a player still lacks, so
 * deleting it loses nothing; one with a block still missing, or in a chunk not loaded, stays until the
 * district's next wave. A file someone else deleted leaves the list. The item of a deleted file stays where it
 * is and a cannon reports its file missing.
 */
public final class RepairFiles extends SavedData {
    public static final String FILE = "mic_rtsim_create_repairs";

    /** One delivered file: its district, name (under the uploaded folder's {@code mic-rtsim}), level and anchor. */
    public record Entry(long district, String file, String dim, long anchor) {}

    private final List<Entry> entries = new ArrayList<>();

    public static final Factory<RepairFiles> FACTORY = new Factory<>(RepairFiles::new, RepairFiles::load);

    public static RepairFiles get(MinecraftServer server) {
        return server.overworld().getDataStorage().computeIfAbsent(FACTORY, FILE);
    }

    public List<Entry> entries() {
        return List.copyOf(entries);
    }

    void add(Entry e) {
        entries.add(e);
        setDirty();
    }

    /**
     * Deletes the earlier files of {@code district} whose blocks are all back (the rule above). Returns the names
     * deleted.
     */
    List<String> retire(MinecraftServer server, long district, Path folder) {
        List<String> gone = new ArrayList<>();
        for (Iterator<Entry> it = entries.iterator(); it.hasNext(); ) {
            Entry e = it.next();
            if (e.district() != district)
                continue;
            Path path = folder.resolve(e.file());
            if (!Files.isRegularFile(path)) {
                it.remove();
                setDirty();
                continue;
            }
            ServerLevel level = server.getLevel(ResourceKey.create(Registries.DIMENSION, ResourceLocation.parse(e.dim())));
            if (level == null || !allBack(level, path, BlockPos.of(e.anchor())))
                continue;
            try {
                Files.deleteIfExists(path);
            } catch (IOException ex) {
                MicRtsimCreate.LOGGER.warn("[mic_rtsim_create] repair schematic {} could not be deleted", path, ex);
                continue;
            }
            it.remove();
            setDirty();
            gone.add(e.file());
        }
        return gone;
    }

    /** True when every block of the schematic at {@code path} is back where it stood, in loaded chunks. */
    static boolean allBack(ServerLevel level, Path path, BlockPos anchor) {
        CompoundTag t;
        try {
            t = NbtIo.readCompressed(path, NbtAccounter.unlimitedHeap());
        } catch (IOException ex) {
            return false;
        }
        ListTag palette = t.getList("palette", Tag.TAG_COMPOUND);
        ListTag blocks = t.getList("blocks", Tag.TAG_COMPOUND);
        if (blocks.isEmpty())
            return false;
        for (int i = 0; i < blocks.size(); i++) {
            CompoundTag b = blocks.getCompound(i);
            ListTag p = b.getList("pos", Tag.TAG_INT);
            BlockPos abs = anchor.offset(p.getInt(0), p.getInt(1), p.getInt(2));
            if (level.getChunkSource().getChunkNow(abs.getX() >> 4, abs.getZ() >> 4) == null)
                return false;
            BlockState before = NbtUtils.readBlockState(level.holderLookup(Registries.BLOCK), palette.getCompound(b.getInt("state")));
            if (!level.getBlockState(abs).is(before.getBlock()))
                return false;
        }
        return true;
    }

    @Override
    public CompoundTag save(CompoundTag tag, HolderLookup.Provider registries) {
        ListTag list = new ListTag();
        for (Entry e : entries) {
            CompoundTag c = new CompoundTag();
            c.putLong("district", e.district());
            c.putString("file", e.file());
            c.putString("dim", e.dim());
            c.putLong("anchor", e.anchor());
            list.add(c);
        }
        tag.put("files", list);
        return tag;
    }

    public static RepairFiles load(CompoundTag tag, HolderLookup.Provider registries) {
        RepairFiles r = new RepairFiles();
        ListTag list = tag.getList("files", Tag.TAG_COMPOUND);
        for (int i = 0; i < list.size(); i++) {
            CompoundTag c = list.getCompound(i);
            r.entries.add(new Entry(c.getLong("district"), c.getString("file"), c.getString("dim"), c.getLong("anchor")));
        }
        return r;
    }
}
