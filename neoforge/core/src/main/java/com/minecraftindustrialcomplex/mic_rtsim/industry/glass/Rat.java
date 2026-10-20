package com.minecraftindustrialcomplex.mic_rtsim.industry.glass;

/**
 * Small exact rationals for the graph's wire fields (GLASSBOX.md §3.7): speeds and gear ratios as
 * {@code i32 / u32}, periods as {@code u32 / u32}. Floats are dyadic, so a Create speed converts exactly;
 * a value that does not fit is rounded in the safe direction (a period up, a speed toward zero) with a
 * smaller denominator.
 */
public record Rat(long num, long den) {

    public static final Rat ZERO = new Rat(0, 1);
    public static final Rat ONE = new Rat(1, 1);

    public Rat {
        if (den == 0)
            throw new ArithmeticException("zero denominator");
        if (den < 0) {
            num = -num;
            den = -den;
        }
        long g = gcd(Math.abs(num), den);
        if (g > 1) {
            num /= g;
            den /= g;
        }
    }

    public static Rat of(long num, long den) {
        return new Rat(num, den);
    }

    /** The exact value of a float (every finite float is m × 2^e). */
    public static Rat ofFloat(float f) {
        if (f == 0 || Float.isNaN(f) || Float.isInfinite(f))
            return ZERO;
        int bits = Float.floatToIntBits(f);
        int exp = ((bits >> 23) & 0xFF);
        long mant = bits & 0x7FFFFF;
        if (exp == 0)
            exp = 1;
        else
            mant |= 0x800000;
        int e = exp - 150; // value = mant × 2^e
        long sign = f < 0 ? -1 : 1;
        if (e >= 0) {
            if (e > 30)
                return new Rat(sign * Integer.MAX_VALUE, 1);
            return new Rat(sign * (mant << e), 1);
        }
        int shift = -e;
        // Strip trailing zero bits so the denominator stays small.
        int tz = Long.numberOfTrailingZeros(mant);
        int s = Math.min(tz, shift);
        mant >>= s;
        shift -= s;
        if (shift > 62)
            return ZERO;
        return new Rat(sign * mant, 1L << shift);
    }

    public Rat div(Rat o) {
        return mulExact(num, o.den, den, o.num);
    }

    public Rat mul(Rat o) {
        return mulExact(num, o.num, den, o.den);
    }

    private static Rat mulExact(long a, long b, long c, long d) {
        long g1 = gcd(Math.abs(a), Math.abs(d)), g2 = gcd(Math.abs(b), Math.abs(c));
        if (g1 > 1) {
            a /= g1;
            d /= g1;
        }
        if (g2 > 1) {
            b /= g2;
            c /= g2;
        }
        return new Rat(Math.multiplyExact(a, b), Math.multiplyExact(c, d));
    }

    public Rat abs() {
        return num < 0 ? new Rat(-num, den) : this;
    }

    public double toDouble() {
        return (double) num / den;
    }

    public boolean isZero() {
        return num == 0;
    }

    /** Fits {@code i32 / u32}, rounding toward zero when it does not. */
    public Rat fitSigned() {
        if (Math.abs(num) <= Integer.MAX_VALUE && den <= 0xFFFF_FFFFL)
            return this;
        return approx(false, Integer.MAX_VALUE);
    }

    /** Fits {@code u32 / u32} as a period: rounded up when it does not fit (a slower rate, still a floor). */
    public Rat fitPeriod() {
        if (num >= 0 && num <= 0xFFFF_FFFFL && den <= 0xFFFF_FFFFL)
            return this;
        return approx(true, 0xFFFF_FFFFL);
    }

    private Rat approx(boolean up, long maxNum) {
        long n = num, d = den;
        while (Math.abs(n) > maxNum || d > 0xFFFF_FFFFL) {
            boolean neg = n < 0;
            long an = Math.abs(n);
            long q = up && an % 2 != 0 ? an / 2 + 1 : an / 2;
            n = neg ? -q : q;
            d = up ? d / 2 : (d + 1) / 2;
            if (d == 0)
                d = 1;
        }
        return new Rat(n, d);
    }

    static long gcd(long a, long b) {
        while (b != 0) {
            long t = a % b;
            a = b;
            b = t;
        }
        return a == 0 ? 1 : a;
    }

    @Override
    public String toString() {
        return den == 1 ? Long.toString(num) : num + "/" + den;
    }
}
