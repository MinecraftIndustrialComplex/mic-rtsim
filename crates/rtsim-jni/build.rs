// SPDX-License-Identifier: AGPL-3.0-or-later
//! Gives the macOS dylib a relocatable install name (`@rpath/librtsim_jni.dylib`), as mic_native's
//! host does, so the library never names the build machine's path.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/librtsim_jni.dylib");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
