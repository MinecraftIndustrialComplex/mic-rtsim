// SPDX-License-Identifier: AGPL-3.0-or-later
//! Records the compiler and the bevy_ecs version the bench was built with, so the results file
//! names them even when the binary runs outside the dev shell.

use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "unknown".to_string(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
        );
    println!("cargo:rustc-env=P4_RUSTC_VERSION={version}");

    // The locked bevy_ecs version: the line after `name = "bevy_ecs"` in Cargo.lock.
    let lock = std::fs::read_to_string("Cargo.lock").unwrap_or_default();
    let mut lines = lock.lines();
    let mut bevy = "unknown".to_string();
    while let Some(line) = lines.next() {
        if line.trim() == "name = \"bevy_ecs\"" {
            if let Some(v) = lines
                .next()
                .and_then(|l| l.trim().strip_prefix("version = "))
            {
                bevy = v.trim_matches('"').to_string();
            }
            break;
        }
    }
    println!("cargo:rustc-env=P4_BEVY_ECS_VERSION={bevy}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");
}
