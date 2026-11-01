package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.protocol.common.custom.BrandPayload;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MobCategory;
import net.minecraft.world.entity.monster.Zombie;
import net.minecraft.world.level.Level;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.event.entity.EntityAttributeCreationEvent;
import net.neoforged.neoforge.registries.DeferredHolder;
import net.neoforged.neoforge.registries.DeferredRegister;

import java.util.function.Consumer;
import java.util.function.Predicate;

/**
 * GameTests only (MZB-1308): {@code mic_rtsim:test_pairing}, a zombie whose pairing data hold a payload for the players {@link #to}
 * names, as an entity of a mod that sends some players data of their own in its {@code sendPairingData} does. Registered on a
 * GameTest server only, as {@link TestMachines}' blocks are. {@link #to} names nobody but inside a test's own step, so the vanilla
 * tracker's pairings never carry the payload to a test player's connection.
 */
public final class TestPairing {
    private TestPairing() {}

    /** The players the payload is for; nobody outside a test's step. */
    public static volatile Predicate<ServerPlayer> to = p -> false;

    private static final DeferredRegister<EntityType<?>> TYPES = DeferredRegister.create(Registries.ENTITY_TYPE, MicRtsim.MODID);
    public static DeferredHolder<EntityType<?>, EntityType<Body>> BODY;

    /** Called from the mod constructor: registers the type when this is a GameTest server. */
    public static void register(IEventBus modBus) {
        if (!Boolean.getBoolean("mic_rtsim.gametest"))
            return;
        BODY = TYPES.register("test_pairing", () -> EntityType.Builder.<Body>of(Body::new, MobCategory.MONSTER).sized(0.6f, 1.95f)
                .clientTrackingRange(8).build("test_pairing"));
        TYPES.register(modBus);
        modBus.addListener(EntityAttributeCreationEvent.class, e -> e.put(BODY.get(), Zombie.createAttributes().build()));
    }

    /** A zombie that never burns, with a payload of its own for the players {@link #to} names. */
    public static final class Body extends Zombie {
        public Body(EntityType<? extends Zombie> type, Level level) {
            super(type, level);
        }

        @Override
        protected boolean isSunSensitive() {
            return false;
        }

        @Override
        public void sendPairingData(ServerPlayer player, Consumer<CustomPacketPayload> bundleBuilder) {
            super.sendPairingData(player, bundleBuilder);
            if (to.test(player))
                bundleBuilder.accept(new BrandPayload("mic_rtsim test pairing"));
        }
    }
}
