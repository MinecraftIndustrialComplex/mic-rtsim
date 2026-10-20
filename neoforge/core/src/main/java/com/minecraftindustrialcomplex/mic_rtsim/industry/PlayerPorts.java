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

    private record Click(ServerLevel level, BlockPos pos, long at) {}

    private record Session(District district, int inventory, Industry.GoodsCounter before) {}

    private record Broken(ServerLevel level, BlockPos pos, long at) {}

    private final Industry ind;
    private final Map<UUID, Click> lastClick = new HashMap<>();
    private final Map<UUID, Session> sessions = new HashMap<>();
    private final List<Broken> brokenStock = new ArrayList<>();

    long sessionPorts;
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
    }

    void rightClicked(Player player, ServerLevel level, BlockPos pos) {
        lastClick.put(player.getUUID(), new Click(level, pos.immutable(), ind.gameTime()));
    }

    void opened(ServerPlayer player, AbstractContainerMenu menu) {
        if (menu == player.inventoryMenu)
            return;
        Click c = lastClick.remove(player.getUUID());
        long now = ind.gameTime();
        District d = null;
        int inventory = IndRecords.NO_INVENTORY;
        if (c != null && now - c.at <= 2 && c.level == player.level()) {
            d = ind.districtAt(c.level, c.pos);
            if (d != null) {
                District.Inventory inv = d.inventoryAt(c.pos);
                if (inv == null) {
                    // A double chest or a multiblock: any inventory whose block the click was part of.
                    BlockPos canon = canonicalOf(c.level, c.pos);
                    inv = canon == null ? null : d.inventoryAt(canon);
                }
                if (inv != null)
                    inventory = inv.index();
            }
        }
        if (d == null) {
            District here = ind.districtAt(player.level(), player.blockPosition());
            if (here != null) {
                // A menu not on a district block (a portable one, another mod's): its effect on stock
                // cannot be told apart from production, so the window is discarded (§3.4).
                ind.districts.discard(here, IndRecords.Discard.UNATTRIBUTED_SESSION);
                unattributed++;
            }
            return;
        }
        sessions.put(player.getUUID(), new Session(d, inventory, snapshot(player)));
    }

    void closed(ServerPlayer player) {
        Session s = sessions.remove(player.getUUID());
        if (s == null || ind.district(s.district.id) != s.district)
            return;
        Industry.GoodsCounter after = snapshot(player);
        Industry.GoodsCounter port = ind.counter();
        port.addAll(s.before, 1);   // what the player had
        port.addAll(after, -1);     // minus what the player has: + into the district
        if (!port.isEmpty()) {
            ind.port(s.district, IndRecords.Port.PLAYER_SESSION, s.inventory, playerKey(player), 0, port);
            sessionPorts++;
        }
    }

    void tossed(Player player, ItemEntity item) {
        if (!(player.level() instanceof ServerLevel level))
            return;
        District d = ind.districtAt(level, player.blockPosition());
        if (d == null)
            return;
        Industry.GoodsCounter port = ind.counter();
        ind.countStack(item.getItem(), item.getItem().getCount(), port);
        ind.port(d, IndRecords.Port.PLAYER_TOSS, IndRecords.NO_INVENTORY, playerKey(player), 0, port);
        tossPorts++;
    }

    void pickedUp(Player player, ItemEntity item, ItemStack original, ItemStack now) {
        if (!(player.level() instanceof ServerLevel level))
            return;
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

    /** A player is breaking a block: if it is a district's stock inventory, its contents leave as a port. */
    void breaking(Player player, ServerLevel level, BlockPos pos) {
        District d = ind.districtAt(level, pos);
        if (d == null)
            return;
        District.Inventory inv = d.inventoryAt(pos);
        if (inv == null) {
            BlockPos canon = canonicalOf(level, pos);
            inv = canon == null ? null : d.inventoryAt(canon);
            // A multiblock (a vault) keeps its contents in its remaining blocks: only a whole one leaves.
            if (inv != null && !canon.equals(pos))
                return;
        }
        if (inv == null)
            return;
        Industry.GoodsCounter goods = ind.sampler.peek(level, inv);
        Industry.GoodsCounter port = ind.counter();
        port.addAll(goods, -1);
        ind.port(d, IndRecords.Port.STOCK_BROKEN, inv.index(), playerKey(player), 0, port);
        brokenStock.add(new Broken(level, pos.immutable(), ind.gameTime()));
        brokenPorts++;
    }

    /** An item entity joined the level: the drop of a stock block broken this tick is marked as ported. */
    void itemJoined(ItemEntity item) {
        if (brokenStock.isEmpty() || !(item.level() instanceof ServerLevel level))
            return;
        for (Broken b : brokenStock)
            if (b.level == level && b.pos.distToCenterSqr(item.position()) <= 4.0) {
                item.setData(IndustryAttachments.PORTED.get(), Boolean.TRUE);
                return;
            }
    }

    private BlockPos canonicalOf(ServerLevel level, BlockPos pos) {
        net.minecraft.world.level.block.entity.BlockEntity be = level.getBlockEntity(pos);
        if (be == null)
            return null;
        for (IndustryPlugin p : IndustryApi.PLUGINS) {
            BlockPos c = p.canonicalInventory(be);
            if (c != null)
                return c;
        }
        net.minecraft.world.level.block.state.BlockState s = be.getBlockState();
        if (s.getBlock() instanceof net.minecraft.world.level.block.ChestBlock && s.hasProperty(net.minecraft.world.level.block.ChestBlock.TYPE)
                && s.getValue(net.minecraft.world.level.block.ChestBlock.TYPE) != net.minecraft.world.level.block.state.properties.ChestType.SINGLE) {
            BlockPos other = pos.relative(net.minecraft.world.level.block.ChestBlock.getConnectedDirection(s));
            return other.asLong() < pos.asLong() ? other : pos;
        }
        return pos;
    }

    /** The player's own inventory and carried stack, by good, packages expanded. */
    private Industry.GoodsCounter snapshot(ServerPlayer player) {
        Industry.GoodsCounter out = ind.counter();
        Inventory inv = player.getInventory();
        for (List<ItemStack> list : List.of(inv.items, inv.armor, inv.offhand))
            for (ItemStack st : list)
                if (!st.isEmpty())
                    ind.countStack(st, st.getCount(), out);
        ItemStack carried = player.containerMenu.getCarried();
        if (!carried.isEmpty())
            ind.countStack(carried, carried.getCount(), out);
        return out;
    }

    /** A player's counterpart key in IND_PORT: the UUID's two halves folded. */
    static long playerKey(Player player) {
        UUID u = player.getUUID();
        return u.getMostSignificantBits() ^ u.getLeastSignificantBits();
    }
}
