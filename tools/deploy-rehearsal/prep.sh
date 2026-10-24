#!/usr/bin/env bash
# prep.sh NAME BASE WORLD [rtsim]: a fresh reflink copy of a server base with WORLD as its world.
# RUNS (default ~/mic-omegasrv/runs) is where the bases and runs live; JARS, when set, is a folder of the
# rtsim and mic_native jars to install (round 2 stages them there), else the round 1 deploy worktree's.
set -euo pipefail
R=${RUNS:-$HOME/mic-omegasrv/runs}
NAME=$1; BASE=$2; WORLD=$3; RT=${4:-}
D=$R/$NAME
[ -e "$D" ] && { echo "exists: $D"; exit 1; }
cp -a --reflink=always "$R/$BASE" "$D"
cp -a --reflink=always "$WORLD" "$D/world"
rm -f "$D/world/session.lock"
if [ "$RT" = rtsim ] && [ -n "${JARS:-}" ]; then
  cp "$JARS"/mic_rtsim*.jar "$JARS"/mic_native-*.jar "$D/mods/"
elif [ "$RT" = rtsim ]; then
  W=/home/greencheetah/Projects/mic/mods/mic-rtsim-deploy/neoforge
  cp "$W"/*/build/libs/mic_rtsim*.jar "$D/mods/"
  cp /home/greencheetah/Projects/mic/mods/mic-native/neoforge/build/libs/mic_native-1.21.1-0.1.0.jar "$D/mods/"
fi
command ls "$D/mods" | grep -cE '\.jar$'
