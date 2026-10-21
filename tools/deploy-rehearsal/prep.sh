#!/usr/bin/env bash
# prep.sh NAME BASE WORLD [rtsim]: a fresh reflink copy of a server base with WORLD as its world.
set -euo pipefail
R=$HOME/mic-livesrv/runs
NAME=$1; BASE=$2; WORLD=$3; RT=${4:-}
D=$R/$NAME
[ -e "$D" ] && { echo "exists: $D"; exit 1; }
cp -a --reflink=always "$R/$BASE" "$D"
cp -a --reflink=always "$WORLD" "$D/world"
rm -f "$D/world/session.lock"
if [ "$RT" = rtsim ]; then
  W=/home/greencheetah/Projects/mic/mods/mic-rtsim-deploy/neoforge
  cp "$W"/*/build/libs/mic_rtsim*.jar "$D/mods/"
  cp /home/greencheetah/Projects/mic/mods/mic-native/neoforge/build/libs/mic_native-1.21.1-0.1.0.jar "$D/mods/"
fi
command ls "$D/mods" | grep -cE '\.jar$'
