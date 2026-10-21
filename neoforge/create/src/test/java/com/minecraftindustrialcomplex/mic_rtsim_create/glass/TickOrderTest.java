package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * The level's ticker lists can hold a ticker with no position: Lithium (in the MIC pack) swaps a sleeping
 * block entity's ticker for {@code SleepingBlockEntity.SLEEPING_BLOCK_ENTITY_TICKER}, which is not removed
 * and whose {@code getPos()} is null. The stress world Mysda "This is Create" hit it: every compile of its
 * crafter district threw a NullPointerException in MinecraftGlassAccess.tickOrder.
 */
class TickOrderTest {

    /** A ticker: its position (null for Lithium's sleeping stand-in) and whether it is removed. */
    record Ticker(Long pos, boolean removed) {}

    @Test
    void sleepingAndRemovedTickersKeepTheirPlaceButGiveNoPosition() {
        long a = 11, b = 22, c = 33;
        Set<Long> wanted = Set.of(a, b, c);
        List<Ticker> tickers = List.of(new Ticker(a, false), new Ticker(null, false), new Ticker(b, true), new Ticker(b, false));
        List<Ticker> pending = List.of(new Ticker(null, false), new Ticker(c, false), new Ticker(a, false));
        Map<Long, Integer> order = TickOrder.places(List.of(tickers, pending), Ticker::removed,
                t -> t.pos() == null ? TickOrder.NO_POS : t.pos(), wanted::contains);
        assertEquals(Map.of(a, 0, b, 3, c, 5), order);
    }
}
