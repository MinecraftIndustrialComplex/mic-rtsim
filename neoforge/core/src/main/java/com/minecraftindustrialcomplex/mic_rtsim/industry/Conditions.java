package com.minecraftindustrialcomplex.mic_rtsim.industry;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;

import java.nio.ByteBuffer;

/**
 * IND_CONDITIONS (I12, F28): the world's season calendar, from the first plugin that knows one
 * ({@code mic_rtsim_climate}: Serene Seasons' cycle, the calendar mic-climate's YearClock reads), else
 * vanilla's, which has no seasons: season 0 of 1, length 0, which the engine reads as "no season
 * boundaries" ({@code next_season_boundary} returns none for a count of 1 or a length of 0).
 *
 * <p>Sent at the session's first tick and whenever the calendar changes, checked every
 * {@code conditionsCheckTicks}: a new season, or an origin that moved (sleeping skips Serene Seasons'
 * cycle ahead, which moves the game time its season 0 began).
 */
public final class Conditions {

    /** Ticks an origin may move before the calendar is sent again (PLACEHOLDER: a minute). */
    static final long ORIGIN_TOLERANCE = 1200;

    /** Vanilla: no seasons. */
    static final long[] VANILLA = {0, 1, 0, 0};

    private final Industry ind;
    private long[] last;
    private long nextCheck = Long.MIN_VALUE;
    long sent;

    Conditions(Industry ind) {
        this.ind = ind;
    }

    void tick(long now) {
        if (now < nextCheck)
            return;
        nextCheck = now + ThreatConfig.get(ThreatConfig.CONDITIONS_CHECK);
        long[] c = current();
        // The origin is compared with a tolerance: a calendar that pauses (no daylight cycle) moves it a
        // little at every check; sleeping, which skips half a day, moves it past the tolerance.
        if (last != null && c[0] == last[0] && c[1] == last[1] && c[2] == last[2] && Math.abs(c[3] - last[3]) <= ORIGIN_TOLERANCE)
            return;
        last = c;
        ByteBuffer r = ind.journal(IndRecords.IND_CONDITIONS, IndRecords.Conditions.SIZE, 0);
        r.putInt(IndRecords.Conditions.SEASON, (int) c[0]);
        r.putInt(IndRecords.Conditions.SEASON_COUNT, (int) c[1]);
        r.putLong(IndRecords.Conditions.SEASON_LENGTH, c[2]);
        r.putLong(IndRecords.Conditions.SEASON_ORIGIN, c[3]);
        ind.commit(r);
        sent++;
    }

    /** The calendar now: {season, seasonCount, seasonLength, seasonOrigin}. */
    public long[] current() {
        for (IndustryPlugin p : IndustryApi.PLUGINS) {
            try {
                long[] s = p.season(ind.server);
                if (s != null && s.length == 4 && s[1] > 0)
                    return s.clone();
            } catch (RuntimeException e) {
                ind.fail("season of " + p.getClass().getSimpleName(), e);
            }
        }
        return VANILLA.clone();
    }

    /** The calendar last sent, or null. */
    public long[] lastSent() {
        return last == null ? null : last.clone();
    }

    /** Checks the calendar at the next tick (GameTests, after a plugin's season changed). */
    public void checkSoon() {
        nextCheck = Long.MIN_VALUE;
    }

    /** Sends the calendar at the next tick whether it changed or not (GameTests). */
    public void resend() {
        last = null;
        nextCheck = Long.MIN_VALUE;
    }

    /**
     * The temperature at a position: a plugin's (mic-climate's, in degrees Celsius) when one knows it,
     * else NaN. Vanilla has no temperature in degrees, only the biome's unitless value
     * ({@link #biomeTemperature}).
     */
    public float celsius(ServerLevel level, BlockPos pos) {
        for (IndustryPlugin p : IndustryApi.PLUGINS) {
            try {
                float t = p.temperature(level, pos);
                if (!Float.isNaN(t))
                    return t;
            } catch (RuntimeException e) {
                ind.fail("temperature of " + p.getClass().getSimpleName(), e);
            }
        }
        return Float.NaN;
    }

    /** Vanilla's biome temperature at a position (unitless: plains 0.8, desert 2.0, snowy plains 0.0). */
    public static float biomeTemperature(ServerLevel level, BlockPos pos) {
        return level.getBiome(pos).value().getBaseTemperature();
    }
}
