package com.minecraftindustrialcomplex.mic_rtsim_ribbits;

import com.minecraftindustrialcomplex.mic_rtsim.api.RtsimApi;
import com.minecraftindustrialcomplex.mic_rtsim.body.GroupKind;
import com.minecraftindustrialcomplex.mic_rtsim.village.Residents;
import com.mojang.logging.LogUtils;
import com.yungnickyoung.minecraft.ribbits.entity.RibbitEntity;
import com.yungnickyoung.minecraft.ribbits.module.RibbitProfessionModule;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.ResourceLocation;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.LivingDropsEvent;
import org.slf4j.Logger;

import java.util.List;

/**
 * mic_rtsim_ribbits: Ribbits 4.1.6 in the sim (D5, C7, C23; PLAN.md §8.8). Java adapters and data only:
 * <ul>
 *   <li>ribbit villages ({@code ribbits:ribbit_village}) are adopted as markets of the {@code ribbit}
 *       culture (C22), and ribbits are their residents, counted by the census;</li>
 *   <li>merchant caravans: {@link RibbitMerchantAdapter}, cargo in the merchant's pack
 *       ({@link RibbitCargo}, P31's recommendation);</li>
 *   <li>touring bands: {@link RibbitMusicianAdapter}, the group {@link RibbitBands#BAND}, and the music
 *       goal steered to the band's own master;</li>
 *   <li>sorcerers who remember: the buff goal leaves out players the village dislikes;</li>
 *   <li>village growth: residents for houses from YUNG's own templates (SPAWN_RESIDENT of
 *       {@code ribbits:ribbit}); the houses themselves are the core's PLACE_HOUSE with template
 *       {@code ribbits:houses/...} and processors {@code ribbits:main};</li>
 *   <li>ribbit merchants in adopted villages trade and restock through the treasury (C23): the core's
 *       resident pricing, and {@code RibbitEntityMixin} for the restock.</li>
 * </ul>
 */
@Mod(MicRtsimRibbits.MODID)
public final class MicRtsimRibbits {
    public static final String MODID = "mic_rtsim_ribbits";
    public static final Logger LOGGER = LogUtils.getLogger();

    public static final ResourceKey<Structure> RIBBIT_VILLAGE =
            ResourceKey.create(Registries.STRUCTURE, ResourceLocation.parse("ribbits:ribbit_village"));
    /** rtsim-settle's culture name. */
    public static final String CULTURE = "ribbit";
    /** The template pool whose jigsaw block in a house marks where its resident stands. */
    public static final String RESIDENT_POOL = "ribbits:ribbits";

    public static final RibbitMerchantAdapter MERCHANT = new RibbitMerchantAdapter();
    public static final RibbitMusicianAdapter MUSICIAN = new RibbitMusicianAdapter();

    public MicRtsimRibbits(IEventBus modBus) {
        RibbitCargo.register(modBus);
        RibbitTalker.register(modBus);
        RtsimApi.registerCulture(h -> h.is(RIBBIT_VILLAGE), CULTURE, RESIDENT_POOL);
        RtsimApi.registerBody(MERCHANT);
        RtsimApi.registerBody(MUSICIAN);
        // A band: its master and three musicians, one per instrument Ribbits has (bass, bongo, flute,
        // guitar). Offsets PLACEHOLDER; the layout format is P15.
        RtsimApi.registerGroup(new GroupKind(RibbitBands.BAND, List.of(
                new GroupKind.Slot(MUSICIAN, Vec3.ZERO),
                new GroupKind.Slot(MUSICIAN, new Vec3(2, 0, 0)),
                new GroupKind.Slot(MUSICIAN, new Vec3(-2, 0, 0)),
                new GroupKind.Slot(MUSICIAN, new Vec3(0, 0, 2)))));
        RtsimApi.registerResidentFactory(Ribbits.RIBBIT.toString(), (level, pos, profession) -> {
            var prof = profession == null ? RibbitProfessionModule.NITWIT : RibbitProfessionModule.getProfession(ResourceLocation.tryParse(profession));
            return prof == null ? null : Ribbits.create(level, pos, prof, Ribbits.umbrella(-1));
        });
        // Ribbits talk (TALK.md section 4): always, like a villager, and a talk with a resident credits its village.
        RtsimApi.registerTalker(Ribbits.RIBBIT, new RibbitTalker(), true);
        modBus.addListener(MicRtsimRibbits::setup);
        NeoForge.EVENT_BUS.addListener(MicRtsimRibbits::onDrops);
    }

    /** Ribbits registers its entity type during registration; it is there by common setup. */
    private static void setup(FMLCommonSetupEvent event) {
        event.enqueueWork(() -> {
            EntityType<?> type = Ribbits.type();
            if (type == null) {
                LOGGER.error("[mic_rtsim_ribbits] no entity type {}: ribbits are not residents", Ribbits.RIBBIT);
                return;
            }
            RtsimApi.registerResident(type, new Residents.Kind(
                    e -> Ribbits.professionName((RibbitEntity) e),
                    e -> ((RibbitEntity) e).getHomePosition(),
                    false));
        });
    }

    /**
     * A ribbit carrying a pack drops it when it dies (P31's recommendation): added to the drops before
     * the core sees them (the core listens at the lowest priority), so each item carries the DIED.
     */
    private static void onDrops(LivingDropsEvent event) {
        if (!(event.getEntity() instanceof RibbitEntity r) || r.level().isClientSide() || RibbitCargo.isEmpty(r))
            return;
        for (ItemStack s : r.getData(RibbitCargo.PACK)) {
            if (s.isEmpty())
                continue;
            ItemEntity item = new ItemEntity(r.level(), r.getX(), r.getY() + 0.5, r.getZ(), s.copy());
            item.setDefaultPickUpDelay();
            event.getDrops().add(item);
        }
        RibbitCargo.clear(r);
    }
}
