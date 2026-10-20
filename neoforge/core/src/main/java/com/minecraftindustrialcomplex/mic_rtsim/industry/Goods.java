package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.HolderLookup;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.ListTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.fluids.FluidStack;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * GOOD names (docs/industry-boundary.md §2.2, F2): {@code <class>/<max stack>/<registry id>[/<component hash>]}.
 * {@code i} items (counted in items), {@code f} fluids (millibuckets), {@code e} energy ({@code e/0/fe}),
 * {@code d} deposits. A good is an item or fluid together with its data components, so a stack's
 * {@code DataComponentPatch} is part of its name, as a hash.
 *
 * <p><b>The hash</b> (choice H2) is FNV-1a 64 over a canonical text of the patch (NBT through the
 * registry-aware codec, compound keys sorted), in 16 hex digits. Only equality matters. The patch
 * behind each hash is kept in the world's industry data, so a good named in an engine command can be
 * rebuilt after a restart.
 */
public final class Goods {

    public static final String ENERGY_FE = "e/0/fe";

    /** A parsed GOOD name. {@code hash} is null when the stack has no component patch. */
    public record Name(char cls, int maxStack, ResourceLocation id, String hash) {}

    /** Patches by hash: the world's (persisted in {@link IndustryData}). */
    private final Map<String, DataComponentPatch> patches;
    private final IndustryData data;
    private final HolderLookup.Provider registries;
    /** Hashes by patch, the hot path of every sweep. Bounded (PLACEHOLDER size). */
    private final LinkedHashMap<DataComponentPatch, String> hashes = new LinkedHashMap<>(256, 0.75f, true) {
        @Override
        protected boolean removeEldestEntry(Map.Entry<DataComponentPatch, String> eldest) {
            return size() > HASH_CACHE;
        }
    };
    private static final int HASH_CACHE = 4096;
    /** Names by (item, empty patch): no allocation for the common case. */
    private final Map<Item, String> plainItemNames = new java.util.IdentityHashMap<>();
    private final Map<Fluid, String> plainFluidNames = new java.util.IdentityHashMap<>();

    Goods(IndustryData data, HolderLookup.Provider registries) {
        this.data = data;
        this.patches = data.patches;
        this.registries = registries;
    }

    // ---- naming ---------------------------------------------------------------------------------

    public String name(ItemStack stack) {
        if (stack.isComponentsPatchEmpty()) {
            Item item = stack.getItem();
            String n = plainItemNames.get(item);
            if (n == null) {
                n = "i/" + item.getDefaultMaxStackSize() + "/" + BuiltInRegistries.ITEM.getKey(item);
                plainItemNames.put(item, n);
            }
            return n;
        }
        return "i/" + stack.getMaxStackSize() + "/" + BuiltInRegistries.ITEM.getKey(stack.getItem()) + "/" + hash(stack.getComponentsPatch());
    }

    public String name(FluidStack stack) {
        DataComponentPatch patch = stack.getComponentsPatch();
        if (patch.isEmpty()) {
            Fluid f = stack.getFluid();
            String n = plainFluidNames.get(f);
            if (n == null) {
                n = "f/0/" + BuiltInRegistries.FLUID.getKey(f);
                plainFluidNames.put(f, n);
            }
            return n;
        }
        return "f/0/" + BuiltInRegistries.FLUID.getKey(stack.getFluid()) + "/" + hash(patch);
    }

    /** The hash of a non-empty patch, remembering the patch behind it. */
    public String hash(DataComponentPatch patch) {
        String h = hashes.get(patch);
        if (h != null)
            return h;
        String text = canonical(patch);
        long x = 0xcbf29ce484222325L;
        for (byte b : text.getBytes(StandardCharsets.UTF_8)) {
            x ^= (b & 0xFF);
            x *= 0x100000001b3L;
        }
        h = String.format("%016x", x);
        hashes.put(patch, h);
        if (!patches.containsKey(h)) {
            patches.put(h, patch);
            data.setDirty();
        }
        return h;
    }

    private String canonical(DataComponentPatch patch) {
        Tag tag = DataComponentPatch.CODEC.encodeStart(RegistryOps.create(NbtOps.INSTANCE, registries), patch)
                .result().orElse(null);
        if (tag == null)
            return patch.toString(); // a component without a codec: equal patches still print equally
        StringBuilder sb = new StringBuilder();
        write(tag, sb);
        return sb.toString();
    }

    private static void write(Tag tag, StringBuilder sb) {
        if (tag instanceof CompoundTag c) {
            List<String> keys = new ArrayList<>(c.getAllKeys());
            java.util.Collections.sort(keys);
            sb.append('{');
            for (String k : keys) {
                sb.append(k).append(':');
                write(c.get(k), sb);
                sb.append(',');
            }
            sb.append('}');
        } else if (tag instanceof ListTag l) {
            sb.append('[');
            for (Tag t : l) {
                write(t, sb);
                sb.append(',');
            }
            sb.append(']');
        } else {
            sb.append(tag.toString());
        }
    }

    // ---- parsing (engine commands name goods by these strings) ----------------------------------

    public static Name parse(String name) {
        if (name == null)
            return null;
        if (name.equals(ENERGY_FE))
            return new Name('e', 0, ResourceLocation.fromNamespaceAndPath("neoforge", "fe"), null);
        String[] head = name.split("/", 3);
        if (head.length < 3 || head[0].length() != 1)
            return null;
        int max;
        try {
            max = Integer.parseInt(head[1]);
        } catch (NumberFormatException e) {
            return null;
        }
        // A registry path may hold '/' itself: the last segment is the hash only when it is one.
        String rest = head[2];
        String hash = null;
        int slash = rest.lastIndexOf('/');
        if (slash > 0 && isHash(rest.substring(slash + 1))) {
            hash = rest.substring(slash + 1);
            rest = rest.substring(0, slash);
        }
        ResourceLocation id = ResourceLocation.tryParse(rest);
        if (id == null)
            return null;
        return new Name(head[0].charAt(0), max, id, hash);
    }

    private static boolean isHash(String s) {
        if (s.length() != 16)
            return false;
        for (int i = 0; i < 16; i++) {
            char c = s.charAt(i);
            if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')))
                return false;
        }
        return true;
    }

    /** A one-item prototype of an item good, or EMPTY when it names no item (or an unknown patch). */
    public ItemStack itemPrototype(Name n) {
        if (n == null || n.cls() != 'i')
            return ItemStack.EMPTY;
        Item item = BuiltInRegistries.ITEM.getOptional(n.id()).orElse(null);
        if (item == null || item == net.minecraft.world.item.Items.AIR)
            return ItemStack.EMPTY;
        ItemStack stack = new ItemStack(item, 1);
        if (n.hash() != null) {
            DataComponentPatch patch = patches.get(n.hash());
            if (patch == null)
                return ItemStack.EMPTY;
            stack.applyComponents(patch);
        }
        return stack;
    }

    /** A one-millibucket prototype of a fluid good, or EMPTY. */
    public FluidStack fluidPrototype(Name n) {
        if (n == null || n.cls() != 'f')
            return FluidStack.EMPTY;
        Fluid fluid = BuiltInRegistries.FLUID.getOptional(n.id()).orElse(null);
        if (fluid == null || fluid == Fluids.EMPTY)
            return FluidStack.EMPTY;
        FluidStack stack = new FluidStack(fluid, 1);
        if (n.hash() != null) {
            DataComponentPatch patch = patches.get(n.hash());
            if (patch == null)
                return FluidStack.EMPTY;
            stack.applyComponents(patch);
        }
        return stack;
    }
}
