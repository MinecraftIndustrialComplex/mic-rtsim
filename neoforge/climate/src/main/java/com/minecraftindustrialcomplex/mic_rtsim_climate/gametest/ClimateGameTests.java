package com.minecraftindustrialcomplex.mic_rtsim_climate.gametest;

import com.minecraftindustrialcomplex.mic_climate.Climate;
import com.minecraftindustrialcomplex.mic_rtsim.industry.IndRecords;
import com.minecraftindustrialcomplex.mic_rtsim_climate.MicRtsimClimate;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTest;
import net.minecraft.gametest.framework.GameTestHelper;
import net.neoforged.neoforge.gametest.GameTestHolder;
import net.neoforged.neoforge.gametest.PrefixGameTestTemplate;
import sereneseasons.api.season.ISeasonState;
import sereneseasons.api.season.SeasonHelper;

import java.nio.ByteBuffer;
import java.util.List;

import static com.minecraftindustrialcomplex.mic_rtsim.gametest.IndustryTestKit.*;

/** mic_rtsim_climate on mic_climate 0.1.0, Serene Seasons 10.1.0.3 and Thermoo 4.8.1. */
@GameTestHolder(MicRtsimClimate.MODID)
@PrefixGameTestTemplate(false)
public final class ClimateGameTests {
    private ClimateGameTests() {}

    /**
     * IND_CONDITIONS carries Serene Seasons' calendar (four seasons of its season duration, season 0 begun
     * its cycle ticks ago, the current season); after the cycle is moved to mid-summer (what
     * {@code /season set mid_summer} does) the next one says summer, with the origin moved by what was
     * skipped; the temperature hook is mic-climate's.
     */
    @GameTest(template = "yard", batch = "climate_1", timeoutTicks = 400)
    public static void conditionsComeFromSereneSeasons(GameTestHelper helper) {
        int[] before = {0};
        helper.startSequence()
                .thenExecute(() -> {
                    ind().conditions().checkSoon();
                    before[0] = received(IndRecords.IND_CONDITIONS, r -> true).size();
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> c = received(IndRecords.IND_CONDITIONS, r -> true);
                    check("an IND_CONDITIONS", !c.isEmpty());
                    ByteBuffer last = c.get(c.size() - 1);
                    ISeasonState s = SeasonHelper.getSeasonState(helper.getLevel().getServer().overworld());
                    record("conditionsComeFromSereneSeasons", "conditions (" + engineName() + ")", "season " + last.getInt(IndRecords.Conditions.SEASON)
                            + " of " + last.getInt(IndRecords.Conditions.SEASON_COUNT) + ", length " + last.getLong(IndRecords.Conditions.SEASON_LENGTH)
                            + ", origin " + last.getLong(IndRecords.Conditions.SEASON_ORIGIN) + "; Serene Seasons: " + s.getSeason() + ", cycle tick "
                            + s.getSeasonCycleTicks() + " of " + s.getCycleDuration());
                    check("four seasons", last.getInt(IndRecords.Conditions.SEASON_COUNT) == 4);
                    check("of Serene Seasons' length", last.getLong(IndRecords.Conditions.SEASON_LENGTH) == s.getSeasonDuration());
                    check("the current season", last.getInt(IndRecords.Conditions.SEASON) == s.getSeason().ordinal());
                    // What /season set mid_summer does: the cycle moved to a summer sub-season, sent to clients.
                    net.minecraft.server.level.ServerLevel ow = helper.getLevel().getServer().overworld();
                    sereneseasons.season.SeasonSavedData data = sereneseasons.season.SeasonHandler.getSeasonSavedData(ow);
                    data.seasonCycleTicks = s.getSeasonDuration() + s.getSubSeasonDuration();
                    data.setDirty();
                    sereneseasons.season.SeasonHandler.sendSeasonUpdate(ow);
                    ind().conditions().checkSoon();
                })
                .thenWaitUntil(() -> {
                    List<ByteBuffer> c = received(IndRecords.IND_CONDITIONS, r -> true);
                    ByteBuffer last = c.get(c.size() - 1);
                    check("summer (season 1): " + last.getInt(IndRecords.Conditions.SEASON), last.getInt(IndRecords.Conditions.SEASON) == 1);
                    BlockPos p = inChunk(helper, 4, 4);
                    float t = ind().conditions().celsius(helper.getLevel(), p);
                    float mc = Climate.celsius(helper.getLevel(), p);
                    record("conditionsComeFromSereneSeasons", "after the move to mid-summer", "origin " + last.getLong(IndRecords.Conditions.SEASON_ORIGIN)
                            + ", temperature " + t + " C (mic-climate " + mc + ")");
                    check("the temperature is mic-climate's", t == mc);
                })
                .thenSucceed();
    }
}
