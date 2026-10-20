package com.minecraftindustrialcomplex.mic_rtsim.market;

import net.minecraft.world.entity.SlotAccess;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Where a puppet's cargo physically is (C21): a trader llama's chest, a group's llama chests, a ribbit
 * merchant's own pack. A caravan sells out of it and puts what it is paid into it, so goods never come
 * from nowhere. Only whole stacks of plain items (no components) count as cargo, as in capture (B23).
 */
public interface CargoAccess {

    /** Plain items of {@code item} held. */
    int count(Item item);

    /** How many more of {@code item} fit. */
    int room(Item item);

    /** Takes up to {@code n}; returns how many were taken. */
    int take(Item item, int n);

    /** Puts up to {@code n}; returns how many went in. */
    int put(Item item, int n);

    /** Totals by item, for robbery detection. */
    Map<Item, Integer> totals();

    /** Cargo over a list of slots (a chest's slots), filled and emptied in slot order. */
    final class Slots implements CargoAccess {
        private final List<SlotAccess> slots;

        public Slots(List<SlotAccess> slots) {
            this.slots = slots;
        }

        private static boolean plain(ItemStack s, Item item) {
            return s.is(item) && s.getComponentsPatch().isEmpty();
        }

        @Override
        public int count(Item item) {
            int n = 0;
            for (SlotAccess a : slots) {
                ItemStack s = a.get();
                if (plain(s, item))
                    n += s.getCount();
            }
            return n;
        }

        @Override
        public int room(Item item) {
            int max = item.getDefaultMaxStackSize();
            int n = 0;
            for (SlotAccess a : slots) {
                ItemStack s = a.get();
                if (s.isEmpty())
                    n += max;
                else if (plain(s, item))
                    n += Math.max(0, max - s.getCount());
            }
            return n;
        }

        @Override
        public int take(Item item, int n) {
            int taken = 0;
            for (int i = slots.size() - 1; i >= 0 && taken < n; i--) {
                SlotAccess a = slots.get(i);
                ItemStack s = a.get();
                if (!plain(s, item))
                    continue;
                int k = Math.min(n - taken, s.getCount());
                ItemStack rest = s.copy();
                rest.shrink(k);
                a.set(rest.isEmpty() ? ItemStack.EMPTY : rest);
                taken += k;
            }
            return taken;
        }

        @Override
        public int put(Item item, int n) {
            int max = item.getDefaultMaxStackSize();
            int put = 0;
            for (SlotAccess a : slots) {
                if (put >= n)
                    break;
                ItemStack s = a.get();
                if (plain(s, item) && s.getCount() < max) {
                    int k = Math.min(n - put, max - s.getCount());
                    a.set(s.copyWithCount(s.getCount() + k));
                    put += k;
                }
            }
            for (SlotAccess a : slots) {
                if (put >= n)
                    break;
                if (a.get().isEmpty()) {
                    int k = Math.min(n - put, max);
                    a.set(new ItemStack(item, k));
                    put += k;
                }
            }
            return put;
        }

        @Override
        public Map<Item, Integer> totals() {
            Map<Item, Integer> out = new LinkedHashMap<>();
            for (SlotAccess a : slots) {
                ItemStack s = a.get();
                if (!s.isEmpty())
                    out.merge(s.getItem(), s.getCount(), Integer::sum);
            }
            return out;
        }
    }

    /** Several cargos as one (a caravan's llamas): taken from the last first, put into the first first. */
    final class Union implements CargoAccess {
        private final List<CargoAccess> parts;

        public Union(List<CargoAccess> parts) {
            this.parts = new ArrayList<>(parts);
        }

        public boolean isEmpty() {
            return parts.isEmpty();
        }

        @Override
        public int count(Item item) {
            int n = 0;
            for (CargoAccess p : parts)
                n += p.count(item);
            return n;
        }

        @Override
        public int room(Item item) {
            int n = 0;
            for (CargoAccess p : parts)
                n += p.room(item);
            return n;
        }

        @Override
        public int take(Item item, int n) {
            int taken = 0;
            for (int i = parts.size() - 1; i >= 0 && taken < n; i--)
                taken += parts.get(i).take(item, n - taken);
            return taken;
        }

        @Override
        public int put(Item item, int n) {
            int put = 0;
            for (CargoAccess p : parts) {
                if (put >= n)
                    break;
                put += p.put(item, n - put);
            }
            return put;
        }

        @Override
        public Map<Item, Integer> totals() {
            Map<Item, Integer> out = new LinkedHashMap<>();
            for (CargoAccess p : parts)
                p.totals().forEach((k, v) -> out.merge(k, v, Integer::sum));
            return out;
        }
    }
}
