#!/bin/sh
# Same as run-vitre.sh but against an isolated, seeded home, so sidebar grouping
# can be exercised without touching the real ~/.vitre.
cd "$(dirname "$0")" || exit 1
PATH="/Users/michaelessiet/.nvm/versions/node/v24.15.0/bin:$PATH"
export PATH
VITRE_SERVER_ENTRY="$PWD/apps/server/dist/bin.mjs"
export VITRE_SERVER_ENTRY
VITRE_HOME="/tmp/vitre-parity-home"
export VITRE_HOME
exec ./target/debug/vitre
