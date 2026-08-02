import { getBuiltInSpriteSheet } from "@pierre/trees";
import { assert, describe, it } from "vite-plus/test";

import {
  hasSpecificPierreIconForFileName,
  resolvePierreIconForEntry,
  syntheticFileNameForLanguageId,
  T3_PIERRE_ICONS,
} from "./pierre-icons";

describe("Pierre file icons", () => {
  it("uses Pierre exact filename and complete-set extension mappings", () => {
    assert.equal(resolvePierreIconForEntry("Dockerfile", "file")?.token, "docker");
    assert.equal(resolvePierreIconForEntry("src/Button.tsx", "file")?.token, "react");
    assert.equal(resolvePierreIconForEntry("vite.config.ts", "file")?.token, "vite");
  });

  it("extends Pierre with T3-specific exact filename icons", () => {
    assert.equal(
      resolvePierreIconForEntry("package.json", "file")?.name,
      "t3-file-icon-package-json",
    );
    assert.equal(
      resolvePierreIconForEntry("config/tsconfig.json", "file")?.name,
      "t3-file-icon-tsconfig",
    );
    assert.equal(resolvePierreIconForEntry("AGENTS.md", "file")?.name, "t3-file-icon-agents");
    assert.equal(resolvePierreIconForEntry("CLAUDE.md", "file")?.name, "t3-file-icon-claude");
    assert.equal(resolvePierreIconForEntry("README.md", "file")?.name, "t3-file-icon-readme");
    assert.equal(resolvePierreIconForEntry("pnpm-lock.yaml", "file")?.name, "t3-file-icon-pnpm");
    assert.equal(
      resolvePierreIconForEntry("pnpm-workspace.yaml", "file")?.name,
      "t3-file-icon-pnpm",
    );
  });

  it("gives the file types Pierre's built-in set misses a specific icon", () => {
    const iconNameFor = (fileName: string) => resolvePierreIconForEntry(fileName, "file")?.name;
    assert.equal(iconNameFor("src/Main.java"), "t3-file-icon-java");
    assert.equal(iconNameFor("App.kt"), "t3-file-icon-kotlin");
    // The longest extension candidate wins, so Gradle build scripts stay Gradle.
    assert.equal(iconNameFor("build.gradle.kts"), "t3-file-icon-gradle");
    assert.equal(iconNameFor("settings.gradle"), "t3-file-icon-gradle");
    assert.equal(iconNameFor("gradlew"), "t3-file-icon-gradle");
    assert.equal(iconNameFor("pom.xml"), "t3-file-icon-java");
    assert.equal(iconNameFor("Program.cs"), "t3-file-icon-csharp");
    assert.equal(iconNameFor("public/index.php"), "t3-file-icon-php");
    assert.equal(iconNameFor("lib/main.dart"), "t3-file-icon-dart");
    assert.equal(iconNameFor("config.toml"), "t3-file-icon-toml");
    assert.equal(iconNameFor("data.xml"), "t3-file-icon-xml");
    assert.equal(iconNameFor("deploy.ps1"), "t3-file-icon-powershell");
    assert.equal(iconNameFor("notes.pdf"), "t3-file-icon-pdf");
    assert.equal(iconNameFor("demo.mp4"), "t3-file-icon-media");
    assert.equal(iconNameFor("Makefile"), "file-tree-builtin-text");
    assert.equal(iconNameFor("Gemfile"), "file-tree-builtin-ruby");
    assert.equal(iconNameFor("go.mod"), "file-tree-builtin-go");
    assert.equal(iconNameFor("fix.patch"), "file-tree-builtin-git");
    for (const fileName of ["Main.java", "App.kt", "config.toml", "Makefile"]) {
      assert.isTrue(hasSpecificPierreIconForFileName(fileName));
    }
  });

  it("keeps Pierre's own filename icons that a remapped extension would shadow", () => {
    const iconNameFor = (fileName: string) => resolvePierreIconForEntry(fileName, "file")?.name;
    assert.equal(iconNameFor("Cargo.toml"), "file-tree-builtin-rust");
    assert.equal(iconNameFor("bunfig.toml"), "file-tree-builtin-bun");
    assert.equal(iconNameFor(".prettierrc.toml"), "file-tree-builtin-prettier");
    assert.equal(iconNameFor("pyproject.toml"), "file-tree-builtin-python");
  });

  it("ships every custom icon referenced by the extended resolver", () => {
    const builtInSprite = getBuiltInSpriteSheet("complete");
    const iconNames = new Set([
      ...Object.values(T3_PIERRE_ICONS.byFileName),
      ...Object.values(T3_PIERRE_ICONS.byFileExtension),
    ]);
    assert.isAbove(iconNames.size, 20);
    for (const iconName of iconNames) {
      const sprite = iconName.startsWith("t3-file-icon-")
        ? T3_PIERRE_ICONS.spriteSheet
        : builtInSprite;
      assert.include(sprite, `id="${iconName}"`, `missing sprite symbol for ${iconName}`);
    }
  });

  it("uses the Pierre default icon for unknown file types", () => {
    assert.equal(resolvePierreIconForEntry("artifact.unknown-ext", "file")?.token, "default");
    assert.isFalse(hasSpecificPierreIconForFileName("artifact.unknown-ext"));
  });

  it("leaves directory rendering to the shared folder fallback", () => {
    assert.isNull(resolvePierreIconForEntry("packages/client-runtime", "directory"));
  });

  it("normalizes common markdown fence language aliases", () => {
    assert.equal(syntheticFileNameForLanguageId("typescript"), "file.ts");
    assert.equal(syntheticFileNameForLanguageId("shellscript"), "file.sh");
    assert.equal(syntheticFileNameForLanguageId("python"), "file.py");
  });
});
