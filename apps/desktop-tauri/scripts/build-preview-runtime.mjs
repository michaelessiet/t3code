// Builds shim/dist/preview-runtime.js: the initialization script the Rust
// shell injects into every preview child webview.
//
// Layout: [Playwright InjectedScript install expression][our preview runtime]
// [element-picker bundle].
// The Playwright source extraction is a port of
// apps/desktop/src/preview/PlaywrightInjectedRuntime.ts (same marker slicing
// of playwright-core's coreBundle, same install options), resolved from the
// Electron app's node_modules so both shells inject the identical version.
// The picker bundle is apps/desktop's PickPreload.ts compiled UNCHANGED with
// `electron` aliased to shim/picker/electron-adapter.ts (react-grab and the
// generated annotation CSS come along from the Electron app's sources), so
// the annotation studio stays byte-identical across shells.
import * as NodeChildProcess from "node:child_process";
import * as NodeFS from "node:fs";
import * as NodeModule from "node:module";
import * as NodePath from "node:path";
import * as NodeURL from "node:url";
import * as NodeVM from "node:vm";

const packageDir = NodePath.dirname(NodePath.dirname(NodeURL.fileURLToPath(import.meta.url)));
const repoRoot = NodePath.resolve(packageDir, "../..");

const SOURCE_MARKER = "source3 = ";
const SOURCE_TERMINATOR = ";\n  }\n});";
const SOURCE_MINIMUM_LENGTH = 100_000;

function extractPlaywrightSource() {
  const desktopRequire = NodeModule.createRequire(
    NodePath.join(repoRoot, "apps/desktop/package.json"),
  );
  const packageJsonPath = desktopRequire.resolve("playwright-core/package.json");
  const bundlePath = NodePath.join(NodePath.dirname(packageJsonPath), "lib/coreBundle.js");
  const coreBundle = NodeFS.readFileSync(bundlePath, "utf8");
  const start = coreBundle.indexOf(SOURCE_MARKER);
  if (start < 0) {
    throw new Error(`Playwright source marker not found in ${bundlePath}`);
  }
  const literalStart = start + SOURCE_MARKER.length;
  const literalEnd = coreBundle.indexOf(SOURCE_TERMINATOR, literalStart);
  if (literalEnd < 0) {
    throw new Error(`Playwright source terminator not found in ${bundlePath}`);
  }
  const literal = coreBundle.slice(literalStart, literalEnd);
  const source = NodeVM.runInNewContext(literal, Object.create(null), { timeout: 1_000 });
  if (typeof source !== "string" || source.length < SOURCE_MINIMUM_LENGTH) {
    throw new Error(
      `Playwright injected runtime from ${bundlePath} was ${typeof source} of length ` +
        `${typeof source === "string" ? source.length : "n/a"}; expected >= ${SOURCE_MINIMUM_LENGTH}`,
    );
  }
  return source;
}

// Same options as playwrightInjectedRuntimeInstallExpression in the Electron app.
const installOptions = JSON.stringify({
  isUnderTest: false,
  sdkLanguage: "javascript",
  testIdAttributeName: "data-testid",
  stableRafCount: 1,
  browserName: "chromium",
  shouldPrependErrorPrefix: false,
  isUtilityWorld: false,
  customEngines: [],
});

const installExpression = `(() => {
  if (globalThis.__t3PlaywrightInjected) return true;
  const module = { exports: {} };
  ${extractPlaywrightSource()}
  globalThis.__t3PlaywrightInjected = new (module.exports.InjectedScript())(globalThis, ${installOptions});
  return true;
})();`;

function buildPickerBundle() {
  const esbuildBin = NodePath.join(repoRoot, "node_modules", ".bin", "esbuild");
  const entry = NodePath.join(repoRoot, "apps/desktop/src/preview/PickPreload.ts");
  const adapter = NodePath.join(packageDir, "shim/picker/electron-adapter.ts");
  const outfile = NodePath.join(packageDir, "shim/dist/preview-picker.js");
  const result = NodeChildProcess.spawnSync(
    esbuildBin,
    [
      entry,
      "--bundle",
      "--format=iife",
      "--platform=browser",
      "--target=es2022",
      `--alias:electron=${adapter}`,
      `--outfile=${outfile}`,
    ],
    { stdio: "inherit" },
  );
  if (result.status !== 0) {
    throw new Error(`preview picker bundle failed (esbuild exit ${result.status})`);
  }
  return NodeFS.readFileSync(outfile, "utf8");
}

const runtime = NodeFS.readFileSync(
  NodePath.join(packageDir, "shim/preview-runtime.src.js"),
  "utf8",
);
const picker = buildPickerBundle();
const outPath = NodePath.join(packageDir, "shim/dist/preview-runtime.js");
NodeFS.mkdirSync(NodePath.dirname(outPath), { recursive: true });
NodeFS.writeFileSync(outPath, `${installExpression}\n${runtime}\n${picker}`);
console.log(
  `[desktop-tauri] preview runtime built: ${outPath} (${Math.round(
    (installExpression.length + runtime.length + picker.length) / 1024,
  )}KB, picker ${Math.round(picker.length / 1024)}KB)`,
);
