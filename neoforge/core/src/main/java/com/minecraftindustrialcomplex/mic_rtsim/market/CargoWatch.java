package com.minecraftindustrialcomplex.mic_rtsim.market;

import com.minecraftindustrialcomplex.mic_rtsim.boundary.Obs;
import com.minecraftindustrialcomplex.mic_rtsim.bridge.RtsimSession;
import com.minecraftindustrialcomplex.mic_rtsim.puppet.PuppetGroup;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;

import java.nio.ByteBuffer;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Robbery (C24, PLAN.md §8.7): a puppet's own cargo changed by something other than the bridge. The
 * bridge remembers each member's cargo as it last left it (at spawn and after every trade); any other
 * change is journaled as CARGO_CHANGED with the signed deltas, a Java op id and the player the bridge
 * holds responsible.
 *
 * <p>In vanilla an untamed trader llama's chest cannot be opened (its inventory screen needs it tamed,
 * and taming takes it out of the sim first), so this mostly catches other mods: an entity item-handler,
 * an inventory-access mod. Attribution is the nearest player within {@link #REACH} blocks
 * (PLACEHOLDER), flagged BY_PROXIMITY (choice G10).
 */
public final class CargoWatch {
    /** Blocks within which the nearest player is held responsible (PLACEHOLDER). */
    public static final double REACH = 8;

    private final RtsimSession session;
    long reports;

    public CargoWatch(RtsimSession session) {
        this.session = session;
    }

    public long reports() {
        return reports;
    }

    /** Remembers every member's own cargo as it is now (after spawn, after a trade). */
    public static void expectAll(PuppetGroup g) {
        for (PuppetGroup.Member m : g.members)
            expect(g, m);
    }

    public static void expect(PuppetGroup g, PuppetGroup.Member m) {
        if (m.gone || m.entity == null)
            return;
        CargoAccess c = g.kind.slots().get(m.index).adapter().ownCargo(m.entity);
        m.expectedCargo = c == null ? null : new HashMap<>(c.totals());
    }

    /** Compares each member's cargo with what the bridge left there; reports and accepts any change. */
    public void check(PuppetGroup g, long now) {
        boolean changed = false;
        for (PuppetGroup.Member m : g.members) {
            if (m.gone || m.entity == null || m.entity.isRemoved() || m.expectedCargo == null)
                continue;
            CargoAccess c = g.kind.slots().get(m.index).adapter().ownCargo(m.entity);
            if (c == null)
                continue;
            Map<Item, Integer> now2 = c.totals();
            Map<Item, Integer> delta = new LinkedHashMap<>();
            for (Map.Entry<Item, Integer> e : now2.entrySet()) {
                int d = e.getValue() - m.expectedCargo.getOrDefault(e.getKey(), 0);
                if (d != 0)
                    delta.put(e.getKey(), d);
            }
            for (Map.Entry<Item, Integer> e : m.expectedCargo.entrySet())
                if (!now2.containsKey(e.getKey()))
                    delta.put(e.getKey(), -e.getValue());
            if (delta.isEmpty())
                continue;
            report(g, m, delta, now);
            m.expectedCargo = new HashMap<>(now2);
            changed = true;
        }
        if (changed) // what the group's merchants can sell or pay for moved
            for (PuppetGroup.Member m : g.members)
                if (!m.gone && m.offers != null)
                    PuppetOffers.recap(g, m);
    }

    private void report(PuppetGroup g, PuppetGroup.Member m, Map<Item, Integer> delta, long now) {
        int n = Math.min(delta.size(), (0xFFF8 - Obs.CargoChanged.ITEMS) / Obs.Item.SIZE);
        int[] ids = new int[n];
        int k = 0;
        for (Item item : delta.keySet()) {
            if (k == n)
                break;
            ids[k++] = session.itemId(item);
        }
        Entity e = m.entity;
        Player p = e.level().getNearestPlayer(e, REACH);
        ByteBuffer r = session.boundary().beginJournaled(Obs.CARGO_CHANGED, Obs.CargoChanged.size(n), m.index, now);
        r.putLong(Obs.CargoChanged.AGENT, g.agent);
        r.putInt(Obs.CargoChanged.EPOCH, g.epoch);
        r.putInt(Obs.CargoChanged.FLAGS, p != null ? Obs.CargoChanged.FLAG_BY_PROXIMITY : 0);
        if (p != null) {
            r.putLong(Obs.CargoChanged.PLAYER_MOST, p.getUUID().getMostSignificantBits());
            r.putLong(Obs.CargoChanged.PLAYER_LEAST, p.getUUID().getLeastSignificantBits());
        }
        r.putInt(Obs.CargoChanged.ITEM_COUNT, n);
        k = 0;
        for (Map.Entry<Item, Integer> d : delta.entrySet()) {
            if (k == n)
                break;
            int at = Obs.CargoChanged.ITEMS + k * Obs.Item.SIZE;
            r.putInt(at + Obs.Item.SLOT, -1);
            r.putInt(at + Obs.Item.ITEM, ids[k]);
            r.putInt(at + Obs.Item.COUNT, d.getValue());
            k++;
        }
        session.boundary().commitWithOpId(r, Obs.CargoChanged.OP_ID);
        reports++;
    }
}
