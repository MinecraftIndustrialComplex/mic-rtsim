package com.minecraftindustrialcomplex.mic_rtsim.village;

import com.google.gson.Gson;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Layout;
import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtUtils;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.packs.resources.ResourceManager;
import net.minecraft.server.packs.resources.SimpleJsonResourceReloadListener;
import net.minecraft.util.profiling.ProfilerFiller;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BedPart;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.DoubleBlockHalf;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.neoforged.neoforge.event.AddReloadListenerEvent;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;

/**
 * The works catalogue (SINK.md §4.2): what a village's works purse buys. Works are data, at
 * {@code data/<ns>/mic_rtsim/work/<id>.json}, so a pack or a plugin adds works without code; the
 * core ships vanilla's lamps, taiga decor and the iron golem, the Ribbits plugin the Ribbit lamp.
 *
 * <pre>{@code
 * { "group": "lamp" | "decor" | "golem",
 *   "culture": "human",                       // Cultures' names
 *   "style": "minecraft:village_plains",      // optional: only villages of this structure
 *   "cap": { "per_residents": 3 } | { "fixed": 4 },
 *   "variants": [
 *     { "template": "minecraft:village/plains/plains_lamp_1", "processors": "ribbits:main" },
 *     { "body": "minecraft:iron_golem", "items": { "minecraft:iron_ingot": 36 } } ] }
 * }</pre>
 *
 * A template's blocks and items are read from its palette when the catalogue is sent (WORK_CATALOG,
 * docs/boundary.md §4.1): air, structure voids and jigsaws are not counted (a jigsaw is replaced by its
 * final state, mostly air), a door or a bed counts once, and a block that is no item (water) counts as
 * a block placed but no item. The engine prices the items it trades as goods and the rest as labour.
 */
public final class WorkCatalog {
    private WorkCatalog() {}

    public static final String DIRECTORY = "mic_rtsim/work";

    public record Variant(ResourceLocation template, ResourceLocation processors, String body, Map<String, Integer> items) {}

    public record Kind(ResourceLocation id, int group, String culture, ResourceLocation style, int cap, int capN,
                       List<Variant> variants) {}

    private static volatile List<Kind> KINDS = List.of();
    private static volatile int generation;

    public static List<Kind> kinds() {
        return KINDS;
    }

    /** Bumped by every data reload, so a session sends the catalogue again after {@code /reload}. */
    public static int generation() {
        return generation;
    }

    public static void onAddReloadListeners(AddReloadListenerEvent event) {
        event.addListener(new Loader());
    }

    private static final class Loader extends SimpleJsonResourceReloadListener {
        Loader() {
            super(new Gson(), DIRECTORY);
        }

        @Override
        protected void apply(Map<ResourceLocation, JsonElement> files, ResourceManager resources, ProfilerFiller profiler) {
            List<Kind> kinds = new ArrayList<>();
            for (Map.Entry<ResourceLocation, JsonElement> e : new TreeMap<>(files).entrySet()) {
                try {
                    kinds.add(parse(e.getKey(), e.getValue().getAsJsonObject()));
                } catch (RuntimeException ex) {
                    MicRtsim.LOGGER.warn("[mic_rtsim] work {} is malformed and left out: {}", e.getKey(), ex.getMessage());
                }
            }
            KINDS = List.copyOf(kinds);
            generation++;
            MicRtsim.LOGGER.info("[mic_rtsim] {} works in the catalogue", kinds.size());
        }
    }

    static Kind parse(ResourceLocation id, JsonObject o) {
        int group = switch (o.get("group").getAsString()) {
            case "lamp" -> Obs.WorkCatalog.GROUP_LAMP;
            case "decor" -> Obs.WorkCatalog.GROUP_DECOR;
            case "golem" -> Obs.WorkCatalog.GROUP_GOLEM;
            default -> throw new IllegalArgumentException("unknown group " + o.get("group"));
        };
        String culture = o.get("culture").getAsString();
        ResourceLocation style = o.has("style") ? ResourceLocation.parse(o.get("style").getAsString()) : null;
        JsonObject cap = o.getAsJsonObject("cap");
        int capKind;
        int capN;
        if (cap.has("per_residents")) {
            capKind = Obs.WorkCatalog.CAP_PER_RESIDENTS;
            capN = Math.max(1, cap.get("per_residents").getAsInt());
        } else if (cap.has("fixed")) {
            capKind = Obs.WorkCatalog.CAP_FIXED;
            capN = Math.max(0, cap.get("fixed").getAsInt());
        } else {
            throw new IllegalArgumentException("cap is neither per_residents nor fixed");
        }
        List<Variant> variants = new ArrayList<>();
        for (JsonElement ve : o.getAsJsonArray("variants")) {
            JsonObject v = ve.getAsJsonObject();
            Map<String, Integer> items = new TreeMap<>();
            if (v.has("items"))
                for (Map.Entry<String, JsonElement> i : v.getAsJsonObject("items").entrySet())
                    items.put(ResourceLocation.parse(i.getKey()).toString(), i.getValue().getAsInt());
            if (v.has("template")) {
                ResourceLocation processors = v.has("processors") ? ResourceLocation.parse(v.get("processors").getAsString()) : null;
                variants.add(new Variant(ResourceLocation.parse(v.get("template").getAsString()), processors, null, items));
            } else if (v.has("body")) {
                variants.add(new Variant(null, null, ResourceLocation.parse(v.get("body").getAsString()).toString(), items));
            } else {
                throw new IllegalArgumentException("a variant names neither a template nor a body");
            }
        }
        if (variants.isEmpty())
            throw new IllegalArgumentException("no variants");
        return new Kind(id, group, culture, style, capKind, capN, List.copyOf(variants));
    }

    // ---- the record -------------------------------------------------------------------------------

    /** What a variant uses, read from its template's palette (or its items alone, for a body). */
    record Bill(int blocks, Map<String, Integer> items) {}

    /** A template's placed blocks and their items, or null when the template is missing. */
    public static Bill bill(MinecraftServer server, ResourceLocation template) {
        Optional<StructureTemplate> t = server.getStructureManager().get(template);
        if (t.isEmpty())
            return null;
        CompoundTag tag = t.get().save(new CompoundTag());
        ListTag palette = tag.contains("palettes", Tag.TAG_LIST)
                ? tag.getList("palettes", Tag.TAG_LIST).getList(0)
                : tag.getList("palette", Tag.TAG_COMPOUND);
        BlockState[] states = new BlockState[palette.size()];
        for (int i = 0; i < states.length; i++)
            states[i] = NbtUtils.readBlockState(BuiltInRegistries.BLOCK.asLookup(), palette.getCompound(i));
        int blocks = 0;
        Map<String, Integer> items = new TreeMap<>();
        ListTag list = tag.getList("blocks", Tag.TAG_COMPOUND);
        for (int i = 0; i < list.size(); i++) {
            int s = list.getCompound(i).getInt("state");
            if (s < 0 || s >= states.length)
                continue;
            BlockState state = states[s];
            if (state.isAir() || state.is(Blocks.STRUCTURE_VOID) || state.is(Blocks.JIGSAW) || state.is(Blocks.STRUCTURE_BLOCK))
                continue;
            if (state.hasProperty(BlockStateProperties.DOUBLE_BLOCK_HALF)
                    && state.getValue(BlockStateProperties.DOUBLE_BLOCK_HALF) == DoubleBlockHalf.UPPER)
                continue;
            if (state.hasProperty(BlockStateProperties.BED_PART) && state.getValue(BlockStateProperties.BED_PART) == BedPart.HEAD)
                continue;
            blocks++;
            Item item = state.getBlock().asItem();
            if (item != Items.AIR)
                items.merge(BuiltInRegistries.ITEM.getKey(item).toString(), 1, Integer::sum);
        }
        return new Bill(blocks, items);
    }

    /**
     * The WORK_CATALOG body (the record after its header) for the session's dictionary. Kinds whose
     * templates are all missing are left out (logged).
     */
    public static byte[] body(RtsimSession session, List<Kind> kinds) {
        MinecraftServer server = session.server();
        record Out(Kind kind, List<Variant> variants, List<Bill> bills) {}
        List<Out> out = new ArrayList<>();
        int size = Obs.WorkCatalog.DEFS;
        for (Kind k : kinds) {
            List<Variant> vs = new ArrayList<>();
            List<Bill> bills = new ArrayList<>();
            int kindSize = Obs.WorkCatalog.DEF;
            for (Variant v : k.variants()) {
                Bill bill;
                if (v.template() != null) {
                    bill = bill(server, v.template());
                    if (bill == null) {
                        MicRtsim.LOGGER.warn("[mic_rtsim] work {}: template {} is missing; the variant is left out", k.id(), v.template());
                        continue;
                    }
                    if (!v.items().isEmpty()) {
                        Map<String, Integer> items = new TreeMap<>(bill.items());
                        v.items().forEach((i, n) -> items.merge(i, n, Integer::sum));
                        bill = new Bill(bill.blocks(), items);
                    }
                } else {
                    bill = new Bill(1, v.items());
                }
                vs.add(v);
                bills.add(bill);
                kindSize += Obs.WorkCatalog.VARIANT + bill.items().size() * Obs.WorkCatalog.ITEM;
            }
            if (vs.isEmpty())
                continue;
            if (size + kindSize > 0xFFF8) {
                MicRtsim.LOGGER.warn("[mic_rtsim] the works catalogue is too large for one record; {} and later works are left out", k.id());
                break;
            }
            size += kindSize;
            out.add(new Out(k, vs, bills));
        }
        int base = Layout.RECORD_HEADER;
        ByteBuffer b = ByteBuffer.allocate(size - base).order(ByteOrder.LITTLE_ENDIAN);
        b.putInt(Obs.WorkCatalog.N - base, out.size());
        int at = Obs.WorkCatalog.DEFS - base;
        for (Out o : out) {
            Kind k = o.kind();
            b.putInt(at + Obs.WorkCatalog.D_ID, session.dictId(Obs.DICT_WORK, k.id().toString()));
            b.putInt(at + Obs.WorkCatalog.D_GROUP, k.group());
            b.putInt(at + Obs.WorkCatalog.D_CULTURE, session.dictId(Obs.DICT_CULTURE, k.culture()));
            b.putInt(at + Obs.WorkCatalog.D_STYLE, k.style() == null ? 0 : session.dictId(Obs.DICT_STRUCTURE, k.style().toString()));
            b.putInt(at + Obs.WorkCatalog.D_CAP, k.cap());
            b.putInt(at + Obs.WorkCatalog.D_CAP_N, k.capN());
            b.putInt(at + Obs.WorkCatalog.D_VARIANTS, o.variants().size());
            at += Obs.WorkCatalog.DEF;
            for (int i = 0; i < o.variants().size(); i++) {
                Variant v = o.variants().get(i);
                Bill bill = o.bills().get(i);
                if (v.template() != null) {
                    b.putInt(at + Obs.WorkCatalog.V_PLACE, Obs.WorkCatalog.PLACE_TEMPLATE);
                    b.putInt(at + Obs.WorkCatalog.V_WHAT, session.dictId(Obs.DICT_TEMPLATE, v.template().toString()));
                    b.putInt(at + Obs.WorkCatalog.V_PROCESSORS,
                            v.processors() == null ? 0 : session.dictId(Obs.DICT_PROCESSORS, v.processors().toString()));
                } else {
                    b.putInt(at + Obs.WorkCatalog.V_PLACE, Obs.WorkCatalog.PLACE_BODY);
                    b.putInt(at + Obs.WorkCatalog.V_WHAT, session.dictId(Obs.DICT_BODY, v.body()));
                    b.putInt(at + Obs.WorkCatalog.V_PROCESSORS, 0);
                }
                b.putInt(at + Obs.WorkCatalog.V_BLOCKS, bill.blocks());
                b.putInt(at + Obs.WorkCatalog.V_ITEMS, bill.items().size());
                at += Obs.WorkCatalog.VARIANT;
                for (Map.Entry<String, Integer> e : bill.items().entrySet()) {
                    b.putInt(at, session.dictId(Obs.DICT_ITEM, e.getKey()));
                    b.putInt(at + 4, e.getValue());
                    at += Obs.WorkCatalog.ITEM;
                }
            }
        }
        return b.array();
    }

    /** Sends the catalogue (a sticky latest-wins record, aux 0) as the session's next hand-over's preamble. */
    public static int send(RtsimSession session) {
        List<Kind> kinds = KINDS;
        session.boundary().preamble(Obs.WORK_CATALOG, 0, body(session, kinds));
        return kinds.size();
    }
}
