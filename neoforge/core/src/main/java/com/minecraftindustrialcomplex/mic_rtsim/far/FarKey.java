package com.minecraftindustrialcomplex.mic_rtsim.far;

import java.util.UUID;

/**
 * Which far mover a record is: the source that wrote it, the object (a Create train's id, later a dragon's)
 * and the part of that object (a carriage's index; 0 for one-part objects).
 */
public record FarKey(String source, UUID object, int part) {
}
