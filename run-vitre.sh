#!/bin/sh
# Dev launcher for manual testing: real ~/.vitre home, repo-local server bundle.
# Node must be on PATH for the sidecar (nvm installs are not in a GUI PATH).
cd "$(dirname "$0")" || exit 1
PATH="/Users/michaelessiet/.nvm/versions/node/v24.15.0/bin:$PATH"
export PATH
VITRE_SERVER_ENTRY="$PWD/apps/server/dist/bin.mjs"
export VITRE_SERVER_ENTRY
cargo build -p vitre-app --bin vitre || exit 1
exec ./target/debug/vitre
