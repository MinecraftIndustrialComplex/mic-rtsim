package com.minecraftindustrialcomplex.mic_rtsim.gametest;

import com.minecraftindustrialcomplex.mic_rtsim.MicRtsim;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.ConflictRecords;
import com.minecraftindustrialcomplex.mic_rtsim.conflict.Territories;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;

import java.util.ArrayList;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.RtsimTests.check;

/**
 * {@link Territories#crossing} and {@link Territories#anyMinding} (M7, MZB-324), on a structure of their own:
 * pure geometry, no world and no session. (The core's JUnit tests have no Minecraft class on their classpath,
 * and the structure holds {@code ResourceKey}s, so these run as GameTests.)
 */
@GameTestHolder(MicRtsim.MODID)
@PrefixGameTestTemplate(false)
public final class TerritoriesGameTests {
    private TerritoriesGameTests() {}

    private static final ResourceKey<Level> OVERWORLD = Level.OVERWORLD, NETHER = Level.NETHER;
    private static final int RAIL = ConflictRecords.SIGNAL_RAIL, RAIL_BIT = 1 << RAIL;

    private static Territories.Territory circle(long faction, ResourceKey<Level> dim, int mask, int x, int z, int radius) {
        return new Territories.Territory(faction, dim, mask, x, z, radius);
    }

    private static List<Territories.Territory> crossing(Territories t, ResourceKey<Level> dim, double x0, double z0, double x1, double z1) {
        List<Territories.Territory> out = new ArrayList<>();
        t.crossing(dim, x0, z0, x1, z1, RAIL, out);
        return out;
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 100, batch = "territories")
    public static void aSegmentMeetsACircleItClipsOrEndsIn(GameTestHelper helper) {
        Territories t = new Territories();
        t.put(circle(1, OVERWORLD, RAIL_BIT, 100, 100, 15));
        // both ends 40 blocks either side of the centre, the line 10 blocks off it: it clips the circle
        check("a clip with both ends outside", crossing(t, OVERWORLD, 60, 110, 140, 110).size() == 1);
        // 16 blocks off: a miss, though the segment's box and cell are the circle's
        check("a miss by a block", crossing(t, OVERWORLD, 60, 116, 140, 116).isEmpty());
        // the nearest point is an end, not the foot of the perpendicular
        check("an end inside", crossing(t, OVERWORLD, 80, 100, 86, 100).size() == 1);
        check("an end just short", crossing(t, OVERWORLD, 80, 100, 84, 100).isEmpty());
        // a segment of no length is a point, and agrees with containing()
        check("a point inside", crossing(t, OVERWORLD, 100, 114, 100, 114).size() == 1);
        check("a point outside", crossing(t, OVERWORLD, 100, 116, 100, 116).isEmpty());
        helper.succeed();
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 100, batch = "territories")
    public static void theMaskTheDimensionAndTheCountAreRespected(GameTestHelper helper) {
        Territories t = new Territories();
        check("nothing minds the rail at first", !t.anyMinding(RAIL));
        t.put(circle(1, OVERWORLD, 1 << ConflictRecords.SIGNAL_BUILDING, 0, 0, 50));
        check("a building territory is no rail's", !t.anyMinding(RAIL) && t.anyMinding(ConflictRecords.SIGNAL_BUILDING));
        check("and is not crossed for the rail", crossing(t, OVERWORLD, -10, 0, 10, 0).isEmpty());

        t.put(circle(2, OVERWORLD, RAIL_BIT, 0, 0, 50));
        t.put(circle(3, NETHER, RAIL_BIT, 0, 0, 50));
        check("the rail is minded", t.anyMinding(RAIL));
        List<Territories.Territory> hit = crossing(t, OVERWORLD, -10, 0, 10, 0);
        check("only the overworld's rail circle", hit.size() == 1 && hit.get(0).faction() == 2);
        List<Territories.Territory> nether = crossing(t, NETHER, -10, 0, 10, 0);
        check("only the nether's", nether.size() == 1 && nether.get(0).faction() == 3);

        // a faction's circle replaced moves its count: faction 2 now minds building only
        t.put(circle(2, OVERWORLD, 1 << ConflictRecords.SIGNAL_BUILDING, 0, 0, 50));
        check("the replaced circle is not crossed", crossing(t, OVERWORLD, -10, 0, 10, 0).isEmpty());
        check("faction 3 still minds the rail", t.anyMinding(RAIL));
        t.remove(3);
        check("removing the last rail circle clears the count", !t.anyMinding(RAIL));
        check("the building count stays", t.anyMinding(ConflictRecords.SIGNAL_BUILDING));
        t.clear();
        check("clear zeroes every count", !t.anyMinding(ConflictRecords.SIGNAL_BUILDING) && !t.anyMinding(-1) && !t.anyMinding(32));
        helper.succeed();
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 100, batch = "territories")
    public static void aCircleOverSeveralCellsIsFoundOnceFromAnyOfThem(GameTestHelper helper) {
        Territories t = new Territories();
        // centred on a cell corner, so it is listed in four cells; a segment across the corner reads all four
        t.put(circle(1, OVERWORLD, RAIL_BIT, 512, 512, 40));
        check("across the corner, once", crossing(t, OVERWORLD, 480, 480, 540, 540).size() == 1);
        check("from the near side of the seam", crossing(t, OVERWORLD, 500, 500, 505, 505).size() == 1);
        check("from the far side", crossing(t, OVERWORLD, 520, 520, 525, 525).size() == 1);
        check("along x from outside", crossing(t, OVERWORLD, 475, 512, 480, 512).size() == 1);
        check("along z from outside", crossing(t, OVERWORLD, 512, 475, 512, 480).size() == 1);
        // negative coordinates floor the right way
        t.put(circle(2, OVERWORLD, RAIL_BIT, -3, -3, 10));
        check("a circle over the origin, from the negative side", crossing(t, OVERWORLD, -12, -3, -8, -3).size() == 1);
        check("and from the positive side", crossing(t, OVERWORLD, 3, 3, 6, 6).size() == 1);
        helper.succeed();
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 100, batch = "territories")
    public static void severalCirclesComeOnceEachAndAWideOneIsFound(GameTestHelper helper) {
        Territories t = new Territories();
        t.put(circle(7, OVERWORLD, RAIL_BIT, 0, 0, 30));
        t.put(circle(5, OVERWORLD, RAIL_BIT, 10, 0, 30));
        t.put(circle(9, OVERWORLD, RAIL_BIT, 0, 0, 20_000)); // over MAX_CELLS cells: the wide list
        List<Territories.Territory> hit = crossing(t, OVERWORLD, -5, 0, 5, 0);
        check("three circles, in put order and the wide one last",
                hit.size() == 3 && hit.get(0).faction() == 7 && hit.get(1).faction() == 5 && hit.get(2).faction() == 9);
        List<Territories.Territory> far = crossing(t, OVERWORLD, 5000, 5000, 5100, 5000);
        check("only the wide one reaches far out", far.size() == 1 && far.get(0).faction() == 9);
        t.remove(9);
        check("and removed it is gone", crossing(t, OVERWORLD, 5000, 5000, 5100, 5000).isEmpty() && t.anyMinding(RAIL));
        helper.succeed();
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 100, batch = "territories")
    public static void aHugeSegmentFallsBackToEveryTerritory(GameTestHelper helper) {
        Territories t = new Territories();
        t.put(circle(1, OVERWORLD, RAIL_BIT, 600_000, 0, 20));
        t.put(circle(2, OVERWORLD, RAIL_BIT, 0, 900_000, 20));
        t.put(circle(3, OVERWORLD, RAIL_BIT, 0, 0, 10_000_000)); // wide
        List<Territories.Territory> hit = crossing(t, OVERWORLD, 0, 0, 600_000, 10);
        check("the one it reaches and the wide one, once each",
                hit.size() == 2 && hit.get(0).faction() == 1 && hit.get(1).faction() == 3);
        helper.succeed();
    }

    @GameTest(template = RtsimTests.TEMPLATE, timeoutTicks = 200, batch = "territories")
    public static void crossingAgreesWithContainingOnPointSegments(GameTestHelper helper) {
        Territories t = new Territories();
        t.put(circle(1, OVERWORLD, RAIL_BIT, 300, 300, 100));
        t.put(circle(2, OVERWORLD, RAIL_BIT, 700, 300, 100));
        t.put(circle(3, OVERWORLD, RAIL_BIT, 500, 600, 150));
        for (int x = 0; x < 1000; x += 17)
            for (int z = 0; z < 800; z += 17) {
                List<Territories.Territory> a = crossing(t, OVERWORLD, x, z, x, z);
                List<Territories.Territory> b = t.containing(OVERWORLD, x, z);
                check("the same circles at " + x + ", " + z, a.size() == b.size() && a.containsAll(b));
            }
        helper.succeed();
    }
}
