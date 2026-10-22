{
  description = "mic-rtsim development environment (Rust engine + NeoForge 1.21.1 bridge)";

  # Same inputs as Deep Time (mods/deep-time/flake.nix), and flake.lock started as a copy of its
  # lock, so both repos share one nixpkgs and one Rust toolchain in the store.
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    systems.url = "github:nix-systems/default";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = import inputs.systems;
      perSystem = { pkgs, lib, system, ... }: let
        # The one Rust version. rust-toolchain.toml must name the same channel;
        # checks.toolchain-channel fails `nix flake check` when they drift.
        rustChannel = "1.98.1";

        rust = pkgs.rust-bin.stable.${rustChannel}.default.override {
          extensions = [ "rust-src" "rustfmt" "clippy" ];
        };

        # The NeoForge bridge (later) builds with Gradle's wrapper on Java 21.
        java = pkgs.jdk21;
      in {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [ inputs.rust-overlay.overlays.default ];
        };

        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [
            rust
            java
            pkgs.git
            pkgs.jq
            pkgs.python3
            pkgs.util-linux # flock, for the shared heavy-job lock
          ];
          env = {
            JAVA_HOME = "${java.home}";
            RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library";
          };
        };

        # A dev client (`nix develop .#client --command ./gradlew :dh:runClient` in neoforge/): Java and the
        # libraries Minecraft's window, sound and input load at run time (as mic-fixes' shell has them).
        devShells.client = let
          clientLibs = with pkgs; [
            libGL
            xorg.libX11
            xorg.libXext
            xorg.libXcursor
            xorg.libXrandr
            xorg.libXi
            xorg.libXxf86vm
            libpulseaudio
            openal
            flite
            udev
          ];
        in pkgs.mkShell {
          nativeBuildInputs = [ java pkgs.git pkgs.util-linux ];
          buildInputs = clientLibs;
          env = {
            JAVA_HOME = "${java.home}";
            LD_LIBRARY_PATH = lib.makeLibraryPath clientLibs;
          };
        };

        checks.toolchain-channel = let
          toml = builtins.fromTOML (builtins.readFile ./rust-toolchain.toml);
        in
          assert lib.assertMsg (toml.toolchain.channel == rustChannel)
            "rust-toolchain.toml channel ${toml.toolchain.channel} != flake rustChannel ${rustChannel}";
          pkgs.runCommand "toolchain-channel-ok" { } "touch $out";
      };
    };
}
