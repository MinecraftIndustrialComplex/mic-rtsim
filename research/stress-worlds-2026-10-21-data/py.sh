#!/usr/bin/env bash
# python env with nbtlib and numpy (gdown etc. only for the downloaders). Pass -I explicitly when reading untrusted data.
exec nix shell --impure --expr '(builtins.getFlake "nixpkgs").legacyPackages.x86_64-linux.python3.withPackages (p: [p.gdown p.nbtlib p.requests p.zstandard p.numpy p.lz4])' -c python3 "$@"
