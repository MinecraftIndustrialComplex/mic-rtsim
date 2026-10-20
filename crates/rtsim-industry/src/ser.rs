// SPDX-License-Identifier: AGPL-3.0-or-later
//! Serde helpers for kernel types that do not derive `Serialize` (the kernel takes no serde
//! dependency). Results JSON only; pages use the kernel codec.

use rtsim_kernel::clock::Tick;
use serde::Serializer;

pub fn tick<S: Serializer>(t: &Tick, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u64(t.0)
}

pub fn opt_tick<S: Serializer>(t: &Option<Tick>, s: S) -> Result<S::Ok, S::Error> {
    match t {
        Some(t) => s.serialize_some(&t.0),
        None => s.serialize_none(),
    }
}
