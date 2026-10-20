package com.minecraftindustrialcomplex.mic_rtsim_ribbits;

import com.minecraftindustrialcomplex.mic_rtsim.market.CargoAccess;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.SlotAccess;
import net.minecraft.world.item.ItemStack;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.attachment.AttachmentType;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;

/**
 * A ribbit merchant carries its caravan's cargo itself (P31's recommendation, choice G15): ribbits
 * have no pack animals, so the goods ride in a pack of {@link #SLOTS} slots (PLACEHOLDER, the ribbit
 * caravan capacity P18 leaves TUNABLE), the attachment {@code mic_rtsim_ribbits:cargo}. Its offers
 * show it (they sell out of it), capture reads it back, and killing the merchant drops it as items,
 * which carry the DIED like a llama's chest (C21).
 */
public final class RibbitCargo {
    private RibbitCargo() {}

    /** Pack slots (PLACEHOLDER: the same as a strength-3 trader llama's chest, 9, times three). */
    public static final int SLOTS = 27;

    private static final DeferredRegister<AttachmentType<?>> TYPES =
            DeferredRegister.create(NeoForgeRegistries.Keys.ATTACHMENT_TYPES, MicRtsimRibbits.MODID);

    /** The pack: {@link #SLOTS} stacks, empty ones included. Saved with the ribbit (a taken merchant keeps it). */
    public static final Supplier<AttachmentType<List<ItemStack>>> PACK = TYPES.register("cargo",
            () -> AttachmentType.<List<ItemStack>>builder(() -> List.of())
                    .serialize(ItemStack.OPTIONAL_CODEC.listOf(), l -> !l.isEmpty()).build());

    static void register(IEventBus modBus) {
        TYPES.register(modBus);
    }

    /** The pack's slots as cargo: each slot reads and writes the attachment (copy on write). */
    public static CargoAccess of(Entity ribbit) {
        List<SlotAccess> slots = new ArrayList<>(SLOTS);
        for (int i = 0; i < SLOTS; i++) {
            int k = i;
            slots.add(new SlotAccess() {
                @Override
                public ItemStack get() {
                    List<ItemStack> pack = ribbit.getData(PACK);
                    return k < pack.size() ? pack.get(k) : ItemStack.EMPTY;
                }

                @Override
                public boolean set(ItemStack stack) {
                    List<ItemStack> pack = new ArrayList<>(ribbit.getData(PACK));
                    while (pack.size() < SLOTS)
                        pack.add(ItemStack.EMPTY);
                    pack.set(k, stack.copy());
                    ribbit.setData(PACK, List.copyOf(pack));
                    return true;
                }
            });
        }
        return new CargoAccess.Slots(slots);
    }

    public static boolean isEmpty(Entity ribbit) {
        for (ItemStack s : ribbit.getData(PACK))
            if (!s.isEmpty())
                return false;
        return true;
    }

    public static void clear(Entity ribbit) {
        ribbit.removeData(PACK);
    }
}
