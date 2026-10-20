package com.minecraftindustrialcomplex.mic_rtsim.village;

import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.entity.ai.gossip.GossipType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.structure.BoundingBox;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

/**
 * One adopted village (C22, PLAN.md §8.2): a market node whose residents stay vanilla entities (C2).
 * The bridge keeps only what it needs to find the village and act in it: its box, meeting point,
 * building footprints (so growth never builds over them), the houses the sim added, and gossip still
 * waiting for residents who were not loaded when it arrived.
 */
public final class Village {

    /** Gossip waiting for residents not loaded when it arrived (C20, PLAN.md §8.7). */
    public record PendingGossip(long op, UUID player, GossipType type, int amount, long writtenAt, long seq) {}

    /** A house the sim placed (C7 growth): its op, footprint and resident slot. */
    public record House(long op, BoundingBox box, BlockPos slot) {}

    public final long id;
    public final ResourceKey<Level> dimension;
    public final ResourceLocation structure;
    public final String culture;
    public final long adoptedAt;
    public BoundingBox box;
    /** How far outside its box the village still counts; {@link VillageRegistry#MARGIN} unless set. */
    public int margin = VillageRegistry.MARGIN;
    public BlockPos meeting;
    public boolean meetingFromPoi;
    /** The start piece's floor: the level a rigid village's houses stand on. */
    public int baseY;
    public final List<BoundingBox> pieces = new ArrayList<>();
    public final List<House> houses = new ArrayList<>();
    public final List<PendingGossip> gossip = new ArrayList<>();
    /** The last sequence given to a pending gossip entry; entries are kept in sequence order. */
    public long gossipSeq;

    public Village(long id, ResourceKey<Level> dimension, ResourceLocation structure, String culture, long adoptedAt,
                   BoundingBox box, BlockPos meeting, int baseY) {
        this.id = id;
        this.dimension = dimension;
        this.structure = structure;
        this.culture = culture;
        this.adoptedAt = adoptedAt;
        this.box = box;
        this.meeting = meeting;
        this.baseY = baseY;
    }

    /** True when {@code pos} is in the village's box widened by {@code margin} blocks. */
    public boolean contains(BlockPos pos, int margin) {
        return pos.getX() >= box.minX() - margin && pos.getX() <= box.maxX() + margin
                && pos.getZ() >= box.minZ() - margin && pos.getZ() <= box.maxZ() + margin
                && pos.getY() >= box.minY() - margin - 16 && pos.getY() <= box.maxY() + margin + 16;
    }

    /** The house the sim placed for {@code op}, or null. */
    public House house(long op) {
        for (House h : houses)
            if (h.op() == op)
                return h;
        return null;
    }

    // ---- NBT ------------------------------------------------------------------------------------

    static int[] boxArray(BoundingBox b) {
        return new int[] {b.minX(), b.minY(), b.minZ(), b.maxX(), b.maxY(), b.maxZ()};
    }

    static BoundingBox box(int[] a) {
        return new BoundingBox(a[0], a[1], a[2], a[3], a[4], a[5]);
    }

    CompoundTag save() {
        CompoundTag t = new CompoundTag();
        t.putLong("id", id);
        t.putString("dim", dimension.location().toString());
        t.putString("structure", structure.toString());
        t.putString("culture", culture);
        t.putLong("adoptedAt", adoptedAt);
        t.putIntArray("box", boxArray(box));
        t.putLong("meeting", meeting.asLong());
        t.putBoolean("meetingFromPoi", meetingFromPoi);
        t.putInt("baseY", baseY);
        t.putInt("margin", margin);
        t.putLong("gossipSeq", gossipSeq);
        ListTag p = new ListTag();
        for (BoundingBox b : pieces)
            p.add(new net.minecraft.nbt.IntArrayTag(boxArray(b)));
        t.put("pieces", p);
        ListTag h = new ListTag();
        for (House house : houses) {
            CompoundTag ht = new CompoundTag();
            ht.putLong("op", house.op());
            ht.putIntArray("box", boxArray(house.box()));
            ht.putLong("slot", house.slot().asLong());
            h.add(ht);
        }
        t.put("houses", h);
        ListTag g = new ListTag();
        for (PendingGossip pg : gossip) {
            CompoundTag gt = new CompoundTag();
            gt.putLong("op", pg.op());
            gt.putUUID("player", pg.player());
            gt.putString("type", pg.type().getSerializedName());
            gt.putInt("amount", pg.amount());
            gt.putLong("at", pg.writtenAt());
            gt.putLong("seq", pg.seq());
            g.add(gt);
        }
        t.put("gossip", g);
        return t;
    }

    public static GossipType gossipType(String name) {
        for (GossipType t : GossipType.values())
            if (t.getSerializedName().equals(name))
                return t;
        return null;
    }

    static Village load(CompoundTag t) {
        Village v = new Village(t.getLong("id"),
                ResourceKey.create(Registries.DIMENSION, ResourceLocation.parse(t.getString("dim"))),
                ResourceLocation.parse(t.getString("structure")), t.getString("culture"), t.getLong("adoptedAt"),
                box(t.getIntArray("box")), BlockPos.of(t.getLong("meeting")), t.getInt("baseY"));
        v.meetingFromPoi = t.getBoolean("meetingFromPoi");
        if (t.contains("margin"))
            v.margin = t.getInt("margin");
        v.gossipSeq = t.getLong("gossipSeq");
        for (Tag p : t.getList("pieces", Tag.TAG_INT_ARRAY))
            v.pieces.add(box(((net.minecraft.nbt.IntArrayTag) p).getAsIntArray()));
        for (Tag h : t.getList("houses", Tag.TAG_COMPOUND)) {
            CompoundTag ht = (CompoundTag) h;
            v.houses.add(new House(ht.getLong("op"), box(ht.getIntArray("box")), BlockPos.of(ht.getLong("slot"))));
        }
        for (Tag g : t.getList("gossip", Tag.TAG_COMPOUND)) {
            CompoundTag gt = (CompoundTag) g;
            GossipType type = gossipType(gt.getString("type"));
            if (type != null)
                v.gossip.add(new PendingGossip(gt.getLong("op"), gt.getUUID("player"), type, gt.getInt("amount"), gt.getLong("at"), gt.getLong("seq")));
        }
        return v;
    }
}
