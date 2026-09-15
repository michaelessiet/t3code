#!/bin/sh
set -eu

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$REPO_ROOT"

for tool in cargo codesign ditto hdiutil node plutil pnpm; do
  command -v "$tool" >/dev/null 2>&1 || {
    echo "missing required tool: $tool" >&2
    exit 1
  }
done

ARCH=$(uname -m)
if [ "$ARCH" != "arm64" ]; then
  echo "the current Vitre DMG recipe is verified only for Apple Silicon" >&2
  exit 1
fi

VERSION=$(node -p "require('./apps/server/package.json').version")
NODE_BINARY=${VITRE_NODE:-$(command -v node)}
OUTPUT_DIR=${VITRE_RELEASE_DIR:-$REPO_ROOT/release}
OUTPUT_DMG="$OUTPUT_DIR/Vitre-$VERSION-$ARCH.dmg"
BUILD_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/vitre-dmg.XXXXXX")
APP="$BUILD_ROOT/Vitre.app"
CONTENTS="$APP/Contents"
RESOURCES="$CONTENTS/Resources"
DEPLOY_ROOT="$BUILD_ROOT/server-deploy"
DMG_ROOT="$BUILD_ROOT/dmg"
SELFTEST_HOME="$BUILD_ROOT/selftest-home"

cleanup() {
  if [ "${VITRE_KEEP_BUILD:-0}" = "1" ]; then
    echo "[vitre-dmg] retained build root for inspection: $BUILD_ROOT"
  else
    rm -rf "$BUILD_ROOT"
  fi
}
trap cleanup EXIT HUP INT TERM

echo "[vitre-dmg] building server bundle"
(cd apps/server && vp run build:bundle)

echo "[vitre-dmg] deploying production server dependencies"
pnpm --filter t3 deploy --prod --legacy "$DEPLOY_ROOT"
node scripts/vitre/prune-packaged-node-modules.mjs "$DEPLOY_ROOT/node_modules"

echo "[vitre-dmg] building release binary"
cargo build --release -p vitre-app --bin vitre

mkdir -p "$CONTENTS/MacOS" "$RESOURCES/node/bin" "$RESOURCES/server" "$DMG_ROOT" "$SELFTEST_HOME" "$OUTPUT_DIR"
ditto target/release/vitre "$CONTENTS/MacOS/Vitre"
ditto "$NODE_BINARY" "$RESOURCES/node/bin/node"
ditto "$DEPLOY_ROOT/dist" "$RESOURCES/server"
ditto "$DEPLOY_ROOT/node_modules" "$RESOURCES/server/node_modules"
ditto apps/desktop/resources/icon.icns "$RESOURCES/AppIcon.icns"
ditto scripts/vitre/Info.plist "$CONTENTS/Info.plist"

plutil -replace CFBundleShortVersionString -string "$VERSION" "$CONTENTS/Info.plist"
plutil -replace CFBundleVersion -string "$(git rev-list --count HEAD)" "$CONTENTS/Info.plist"
chmod 755 "$CONTENTS/MacOS/Vitre" "$RESOURCES/node/bin/node"

echo "[vitre-dmg] ad-hoc signing bundle"
# The production pnpm tree contains internal symlinks. Let codesign seal that
# tree as resources instead of recursively following every link as nested code.
# Public distribution will replace this ad-hoc seal with the M6 Developer ID
# signing/notarization pipeline.
codesign --force --sign - --identifier com.t3tools.vitre "$APP"
codesign --verify --strict --verbose=2 "$APP"

echo "[vitre-dmg] running packaged sidecar/auth/RPC self-test without shell PATH"
env -i \
  HOME="$SELFTEST_HOME" \
  PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
  TMPDIR="${TMPDIR:-/tmp}" \
  USER="$(id -un)" \
  VITRE_HOME="$SELFTEST_HOME" \
  VITRE_SELFTEST=1 \
  "$CONTENTS/MacOS/Vitre"

ditto "$APP" "$DMG_ROOT/Vitre.app"
ln -s /Applications "$DMG_ROOT/Applications"
rm -f "$OUTPUT_DMG"

echo "[vitre-dmg] creating $OUTPUT_DMG"
hdiutil create \
  -volname "Vitre $VERSION" \
  -srcfolder "$DMG_ROOT" \
  -ov \
  -format UDZO \
  "$OUTPUT_DMG"
hdiutil verify "$OUTPUT_DMG"
shasum -a 256 "$OUTPUT_DMG"
echo "[vitre-dmg] ready: $OUTPUT_DMG"
