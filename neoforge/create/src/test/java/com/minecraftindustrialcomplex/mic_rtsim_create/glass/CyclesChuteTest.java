package com.minecraftindustrialcomplex.mic_rtsim_create.glass;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

/**
 * Chute timing in both directions. The upward case is the stress-world bug (Mysda's "This is Create",
 * docs/stress-worlds-results.md): a chute lifted by an encased fan below has positive motion, and the
 * compile of its district threw "downward motion must be negative: 0.075" on every attempt.
 */
class CyclesChuteTest {

    @Test
    void gravityChuteKeepsItsCalibratedTiming() {
        float g = Cycles.chuteMotion(0.0F, 0.0F);
        assertEquals(-0.2F, g);
        assertEquals(6, Cycles.chuteTicksPerBlock(g));
        assertEquals(7, Cycles.chuteGrabPeriod(g));
        assertEquals(3, Cycles.chuteParkedSteps(g));
    }

    @Test
    void upwardChuteFromAFanBelow() {
        // A fan at 96 RPM below the chute: push 96, (96 / 64) / 20 = 0.075 blocks a tick, no gravity term.
        float up = Cycles.chuteMotion(96.0F, 0.0F);
        assertEquals(0.075F, up);
        // ChuteBlockEntity.tick, upward branch: a stack pulled from below starts at 0.0, moves by motion
        // each tick and is handed on in the step that takes it above 1.0 (13 steps reach 0.975, 14 pass 1.0).
        assertEquals(14, Cycles.chuteTicksPerBlock(up));
        assertEquals(15, Cycles.chuteGrabPeriod(up));
    }

    @Test
    void fastestUpwardChute() {
        // motion clamps at 20 / 20 = 1.0: the first step lands on exactly 1.0, which is not above it.
        float up = Cycles.chuteMotion(4096.0F, 0.0F);
        assertEquals(1.0F, up);
        assertEquals(2, Cycles.chuteTicksPerBlock(up));
    }

    @Test
    void stillChuteHasNoTiming() {
        assertThrows(IllegalArgumentException.class, () -> Cycles.chuteTicksPerBlock(0.0F));
        // A parked stack is a downward state only (an upward chute parks at 0.5 too, but no hand-off loop
        // reads it).
        assertThrows(IllegalArgumentException.class, () -> Cycles.chuteParkedSteps(0.075F));
    }
}
