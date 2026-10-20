#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# The scale check (SCALE.md §10): runs the quick sweeps and fails when a measured growth slope
# exceeds the slope its sweep declares (crates/rtsim-cli/src/bench/scale.rs, `sweeps`).
#
# Run it inside the dev shell, under the shared heavy-job lock on Ben's laptop:
#   nix develop -c flock /tmp/mic-rtsim-heavy.lock nice -n 10 tools/ci/scale-check.sh
# Results go to bench/scratch/ (gitignored); committed results live in bench/results/.
set -euo pipefail
cd "$(dirname "$0")/../.."
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" cargo build --release --quiet -p rtsim-cli
exec ./target/release/rtsim-cli bench scale --quick --check --out bench/scratch "$@"
