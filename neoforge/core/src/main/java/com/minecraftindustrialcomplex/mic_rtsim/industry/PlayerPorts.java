package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.item.ItemStack;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

/**
 * Ports through players (INDUSTRY.md §3.3, docs/industry-boundary.md §3.3): what a player moves
 * across a district's boundary is never production.
 * <ul>
 *   <li><b>Container sessions</b> ({@code PlayerContainerEvent.Open} to {@code Close}): the menu is
 *       attributed to the block the player right-clicked just before it opened. If that block is in a
 *       district, the session's port is the negated change of the player's own inventory (what left the
 *       player went in). A session that cannot be attributed while the player stands in a district
 *       discards the open learning window instead (IND_DISCARD reason 1).</li>
 *   <li><b>Tosses</b> ({@code ItemTossEvent}) by a player in a district: + into it.</li>
 *   <li><b>Pickups</b> ({@code ItemEntityPickupEvent.Post}) in a district: - out of it, unless the
 *       item entity is the drop of a stock block the player broke, whose contents already left as that
 *       break's port.</li>
 *   <li><b>A stock block broken by a player</b> ({@code BlockEvent.BreakEvent}): its contents leave the
 *       boundary (kind 6), and the drops are marked so their pickup is not counted twice.</li>
 * </ul>
 */
public final class PlayerPorts {

    private record Click(ServerLevel level, BlockPos pos, long at, District district, int inventory, List<ItemStack> before) {}

    private record Session(District district, int inventory, List<ItemStack> before) {}

    private record Broken(ServerLevel level, BlockPos pos, long at) {}

    private final Industry ind;
    private final Map<UUID, Click> lastClick = new HashMap<>();
    private final Map<UUID, Session> sessions = new HashMap<>();
    private final List<Broken> brokenStock = new ArrayList<>();

    long sessionPorts;
    long clickPorts;
    long tossPorts;
    long pickupPorts;
    long brokenPorts;
    long unattributed;

    PlayerPorts(Industry ind) {
        this.ind = ind;
    }

    void tick(long now) {
        if (!brokenStock.isEmpty())
            brokenStock.removeIf(b -> now - b.at > 1);
        if (lastClick.isEmpty())
            return;
        // A click on boundary stock that opened no menu (a bucket into a tank, a hand into a vault): the
        // player's change over the tick is the port.
        for (var it = lastClick.entrySet().iterator(); it.hasNext(); ) {
            var e = it.next();
            Click c = e.getValue();
            if (now - c.at > 2) {
                it.remove();
                continue;
            }
            if (c.before == null || c.at != now)
                continue;
            ServerPlayer player = ind.server.getPlayerList().getPlayer(e.getKey());
            if (player == null || ind.district(c.district.id) != c.district)
                continue;
            Industry.GoodsCounter port = diff(c.before, snapshot(player));
            if (!port.isEmpty()) {
                ind.port(c.district, IndRecords.Port.PLAYER_SESSION, c.inventory, playerKey(player), 0, port);
                clickPorts++;
            }
            e.setValue(new Click(c.level, c.pos, c.at, c.district, c.inventory, null));
        }
    }

    void rightClicked(Player player, ServerLevel level, BlockPos pos) {
        District d = ind.districtAt(level, pos);
        int inventory = IndRecords.NO_INVENTORY;
        List<ItemStack> before = null;
        if (d != null && player instanceof ServerPlayer sp) {
            District.Inventory inv = stockAt(d, level, pos);
            if (inv != null) {
                inventory = inv.index();
                before = snapshot(sp);
            }
        }
        lastClick.put(player.getUUID(), new Click(level, pos.immutable(), ind.gameTime(), d, inventory, before));
    }

    private static District.Inventory stockAt(District d, ServerLevel level, BlockPos pos) {
        District.Inventory inv = d.inventoryAt(pos);
        if (inv == null) {
            // A double chest or a multiblock: the inventory whose block the click was part of.
            BlockPos canon = Districts.canonicalInventory(level, pos);
            inv = canon == null ? null : d.inventoryAt(canon);
        }
        return inv;
    }

    void opened(ServerPlayer player, AbstractContainerMenu menu) {
        if (menu == player.inventoryMenu)
            return;
        Click c = lastClick.remove(player.getUUID());
        long now = ind.gameTime();
        if (c != null && now - c.at <= 2 && c.level == player.level() && c.district != null && c.inventory != IndRecords.NO_INVENTORY) {
            // A session on boundary stock (§3.3): the player's change between Open and Close is the port.
            sessions.put(player.getUUID(), new Session(c.district, c.inventory, c.before != null ? c.before : snapshot(player)));
            return;
        }
        // A menu on a district block that is not boundary stock (a crafting table, a furnace) or on
        // nothing we can tell: its effect cannot be told apart from production, so the window is
        // discarded (§3.4, IND_DISCARD 1).
        District here = c != null && c.district != null ? c.district : ind.districtAt(player.level(), player.blockPosition());
        if (here != null) {
            ind.districts.discard(here, IndRecords.Discard.UNATTRIBUTED_SESSION);
            unattributed++;
        }
    }

    void closed(ServerPlayer player) {
        Session s = sessions.remove(player.getUUID());
        if (s == null || ind.district(s.district.id) != s.district)
            return;
        Industry.GoodsCounter port = diff(s.before, snapshot(player));
        if (!port.isEmpty()) {
            ind.port(s.district, IndRecords.Port.PLAYER_SESSION, s.inventory, playerKey(player), 0, port);
            sessionPorts++;
        }
    }

    void tossed(Player player, ItemEntity item) {
        if (!(player.level() instanceof ServerLevel level) || sessions.containsKey(player.getUUID()))
            return; // thrown out of an open menu: the session's diff has it
        District d = ind.districtAt(level, player.blockPosition());
        if (d == null)
            return;
        Industry.GoodsCounter port = ind.counter();
        ind.countStack(item.getItem(), item.getItem().getCount(), port);
        ind.port(d, IndRecords.Port.PLAYER_TOSS, IndRecords.NO_INVENTORY, playerKey(player), 0, port);
        tossPorts++;
    }

    void pickedUp(Player player, ItemEntity item, ItemStack original, ItemStack now) {
        if (!(player.level() instanceof ServerLevel level) || sessions.containsKey(player.getUUID()))
            return; // picked up with a menu open: the session's diff has it
        int taken = original.getCount() - now.getCount();
        if (taken <= 0)
            return;
        if (item.hasData(IndustryAttachments.PORTED.get()) && item.getData(IndustryAttachments.PORTED.get()))
            return; // the contents of a broken stock block: already a port
        District d = ind.districtAt(level, item.blockPosition());
        if (d == null)
            return;
        Industry.GoodsCounter port = ind.counter();
        ind.countStack(original, -taken, port);
        ind.port(d, IndRecords.Port.PLAYER_PICKUP, IndRecords.NO_INVENTORY, playerKey(player), 0, port);
        pickupPorts++;
    }

    /**
     * A player is breaking a block: if it holds a district's boundary stock, what it drops leaves as a
     * port. One block of a multiblock (a vault) drops nothing of the shared inventory, so it is no port; a
     * double chest's half drops its own 27 slots, which is the port.
     */
    void breaking(Player player, ServerLevel level, BlockPos pos) {
        District d = ind.districtAt(level, pos);
        if (d == null)
            return;
        District.Inventory inv = stockAt(d, level, pos);
        if (inv == null)
            return;
        Industry.GoodsCounter port = ind.counter();
        if (level.getBlockEntity(pos) instanceof net.minecraft.world.Container own) {
            // Vanilla containers (a chest half included) drop their own slots.
            for (int i = 0; i < own.getContainerSize(); i++) {
                ItemStack st = own.getItem(i);
                if (!st.isEmpty())
                    ind.countStack(st, -st.getCount(), port);
            }
        } else if (inv.pos().equals(pos)) {
            port.addAll(ind.sampler.peek(level, inv), -1);
        } else {
            return; // a non-controller block of a multiblock
        }
        ind.port(d, IndRecords.Port.STOCK_BROKEN, inv.index(), playerKey(player), 0, port);
        markDropsPorted(level, pos);
        brokenPorts++;
    }

    /** The item entities that join at {@code pos} this tick were counted already (a break's or a reconcile's port). */
    void markDropsPorted(ServerLevel level, BlockPos pos) {
        brokenStock.add(new Broken(level, pos.immutable(), ind.gameTime()));
    }

    /** An item entity joined the level: the drop of a block counted this tick is marked as ported. */
    void itemJoined(ItemEntity item) {
        if (brokenStock.isEmpty() || !(item.level() instanceof ServerLevel level))
            return;
        for (Broken b : brokenStock)
            if (b.level == level && b.pos.distToCenterSqr(item.position()) <= 4.0) {
                item.setData(IndustryAttachments.PORTED.get(), Boolean.TRUE);
                return;
            }
    }

    /** The player's own inventory and carried stack, as copies (named as goods only if they change). */
    private static List<ItemStack> snapshot(ServerPlayer player) {
        List<ItemStack> out = new ArrayList<>(42);
        Inventory inv = player.getInventory();
        for (List<ItemStack> list : List.of(inv.items, inv.armor, inv.offhand))
            for (ItemStack st : list)
                if (!st.isEmpty())
                    out.add(st.copy());
        ItemStack carried = player.containerMenu.getCarried();
        if (!carried.isEmpty())
            out.add(carried.copy());
        return out;
    }

    /**
     * What left the player (before minus after) by good, + into the district. Stacks are compared
     * item and components; only goods whose amount changed are named, so a tool's durability or a named
     * item in the player's pockets never becomes a dictionary entry.
     */
    private Industry.GoodsCounter diff(List<ItemStack> before, List<ItemStack> after) {
        List<ItemStack> kinds = new ArrayList<>();
        List<long[]> net = new ArrayList<>();
        for (int pass = 0; pass < 2; pass++) {
            for (ItemStack st : pass == 0 ? before : after) {
                int k = -1;
                for (int i = 0; i < kinds.size(); i++)
                    if (ItemStack.isSameItemSameComponents(kinds.get(i), st)) {
                        k = i;
                        break;
                    }
                if (k < 0) {
                    kinds.add(st);
                    net.add(new long[] {0});
                    k = kinds.size() - 1;
                }
                net.get(k)[0] += pass == 0 ? st.getCount() : -st.getCount();
            }
        }
        Industry.GoodsCounter out = ind.counter();
        for (int i = 0; i < kinds.size(); i++)
            if (net.get(i)[0] != 0)
                ind.countStack(kinds.get(i), net.get(i)[0], out);
        return out;
    }

    /** A player's counterpart key in IND_PORT: the UUID's two halves folded. */
    static long playerKey(Player player) {
        UUID u = player.getUUID();
        return u.getMostSignificantBits() ^ u.getLeastSignificantBits();
    }
}
