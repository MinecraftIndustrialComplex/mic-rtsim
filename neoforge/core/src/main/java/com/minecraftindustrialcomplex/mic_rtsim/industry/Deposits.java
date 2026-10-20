package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.level.Level;

import java.nio.charset.StandardCharsets;

/**
 * GOOD names of finite deposits (I11, F15; choice Q3). The engine keys a district's deposits by good
 * ({@code District::deposits}), so each deposit is a good of its own: {@code d/0/<kind id>/<place hash>},
 * where the kind id names what it is (an Ore Excavation vein recipe, {@code destroy:crude_oil}) and the
 * place hash, 16 hex digits of FNV-1a 64 over the dimension and chunk (or another key the plugin picks),
 * keeps two chunks' veins apart. {@link Goods#parse} reads the hash as the name's component hash, and
 * {@link #owns} checks it against the chunk a command names.
 */
public final class Deposits {
    private Deposits() {}

    /** The deposit good of {@code kind} in chunk (cx, cz) of {@code dim}. */
    public static String name(ResourceLocation kind, ResourceKey<Level> dim, int cx, int cz) {
        return "d/0/" + kind + "/" + placeHash(dim.location() + "|" + cx + "|" + cz);
    }

    /** A deposit good keyed by any text (a district's attributed pollution: its district id). */
    public static String name(ResourceLocation kind, String key) {
        return "d/0/" + kind + "/" + placeHash(key);
    }

    /** 16 hex digits of FNV-1a 64 over the key. */
    public static String placeHash(String key) {
        long x = 0xcbf29ce484222325L;
        for (byte b : key.getBytes(StandardCharsets.UTF_8)) {
            x ^= (b & 0xFF);
            x *= 0x100000001b3L;
        }
        return String.format("%016x", x);
    }

    /** True when {@code good} is the deposit good of some kind in chunk (cx, cz) of {@code dim}: its kind id, or null. */
    public static ResourceLocation kindIn(String good, ResourceKey<Level> dim, int cx, int cz) {
        Goods.Name n = Goods.parse(good);
        if (n == null || n.cls() != 'd' || n.hash() == null)
            return null;
        return n.hash().equals(placeHash(dim.location() + "|" + cx + "|" + cz)) ? n.id() : null;
    }

    /** The kind id of a deposit good keyed by {@code key}, or null when it is not that key's. */
    public static ResourceLocation kindFor(String good, String key) {
        Goods.Name n = Goods.parse(good);
        if (n == null || n.cls() != 'd' || n.hash() == null)
            return null;
        return n.hash().equals(placeHash(key)) ? n.id() : null;
    }
}
