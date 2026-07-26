import * as NodeServices from "@effect/platform-node/NodeServices";
import { it, describe, expect } from "@effect/vitest";
import * as Effect from "effect/Effect";
import * as FileSystem from "effect/FileSystem";
import * as Layer from "effect/Layer";
import * as Path from "effect/Path";

import { fileContentRevision } from "@t3tools/shared/fileRevision";

import * as ServerConfig from "../config.ts";
import * as VcsDriverRegistry from "../vcs/VcsDriverRegistry.ts";
import * as VcsProcess from "../vcs/VcsProcess.ts";
import * as WorkspaceEntries from "./WorkspaceEntries.ts";
import * as WorkspaceFileSystem from "./WorkspaceFileSystem.ts";
import * as WorkspacePaths from "./WorkspacePaths.ts";

const ProjectLayer = WorkspaceFileSystem.layer.pipe(
  Layer.provide(WorkspacePaths.layer),
  Layer.provide(WorkspaceEntries.layer.pipe(Layer.provide(WorkspacePaths.layer))),
);

const TestLayer = Layer.empty.pipe(
  Layer.provideMerge(ProjectLayer),
  Layer.provideMerge(WorkspaceEntries.layer.pipe(Layer.provide(WorkspacePaths.layer))),
  Layer.provideMerge(WorkspacePaths.layer),
  Layer.provideMerge(VcsDriverRegistry.layer.pipe(Layer.provide(VcsProcess.layer))),
  Layer.provide(
    ServerConfig.ServerConfig.layerTest(process.cwd(), {
      prefix: "t3-workspace-files-test-",
    }),
  ),
  Layer.provideMerge(NodeServices.layer),
);

const makeTempDir = Effect.gen(function* () {
  const fileSystem = yield* FileSystem.FileSystem;
  return yield* fileSystem.makeTempDirectoryScoped({
    prefix: "t3code-workspace-files-",
  });
});

const writeTextFile = Effect.fn("writeTextFile")(function* (
  cwd: string,
  relativePath: string,
  contents = "",
) {
  const fileSystem = yield* FileSystem.FileSystem;
  const path = yield* Path.Path;
  const absolutePath = path.join(cwd, relativePath);
  yield* fileSystem
    .makeDirectory(path.dirname(absolutePath), { recursive: true })
    .pipe(Effect.orDie);
  yield* fileSystem.writeFileString(absolutePath, contents).pipe(Effect.orDie);
});

it.layer(TestLayer, { excludeTestServices: true })("WorkspaceFileSystemLive", (it) => {
  describe("readFile", () => {
    it.effect("reads UTF-8 files relative to the workspace root", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/index.ts", "export const answer = 42;\n");

        const result = yield* workspaceFileSystem.readFile({
          cwd,
          relativePath: "src/index.ts",
        });

        expect(result).toEqual({
          relativePath: "src/index.ts",
          contents: "export const answer = 42;\n",
          byteLength: 26,
          truncated: false,
          revision: fileContentRevision("export const answer = 42;\n"),
        });
      }),
    );

    it.effect("rejects reads outside the workspace root", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;

        const error = yield* workspaceFileSystem
          .readFile({ cwd, relativePath: "../escape.md" })
          .pipe(Effect.flip);

        expect(error.message).toContain(
          "Workspace file path must be relative to the project root: ../escape.md",
        );
      }),
    );

    it.effect("rejects symlinks that resolve outside the workspace root", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;
        const outsideDir = yield* makeTempDir;
        yield* writeTextFile(outsideDir, "secret.txt", "outside\n");
        yield* fileSystem.symlink(
          path.join(outsideDir, "secret.txt"),
          path.join(cwd, "linked-secret.txt"),
        );

        const error = yield* workspaceFileSystem
          .readFile({ cwd, relativePath: "linked-secret.txt" })
          .pipe(Effect.flip);
        const resolvedWorkspaceRoot = yield* fileSystem.realPath(cwd);
        const resolvedPath = yield* fileSystem.realPath(path.join(outsideDir, "secret.txt"));

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspaceFilePathEscapeError);
        expect(error).toMatchObject({
          workspaceRoot: cwd,
          relativePath: "linked-secret.txt",
          resolvedWorkspaceRoot,
          resolvedPath,
        });
        expect("cause" in error).toBe(false);
      }),
    );

    it.effect("rejects directories without manufacturing an I/O cause", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;
        yield* fileSystem.makeDirectory(path.join(cwd, "src"));

        const error = yield* workspaceFileSystem
          .readFile({ cwd, relativePath: "src" })
          .pipe(Effect.flip);
        const resolvedPath = yield* fileSystem.realPath(path.join(cwd, "src"));

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspacePathNotFileError);
        expect(error).toMatchObject({
          workspaceRoot: cwd,
          relativePath: "src",
          resolvedPath,
        });
        expect("cause" in error).toBe(false);
      }),
    );

    it.effect("rejects binary files without leaking their contents into the error", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;
        const absolutePath = path.join(cwd, "asset.bin");
        yield* fileSystem.writeFile(absolutePath, Uint8Array.from([0x61, 0, 0x62]));

        const error = yield* workspaceFileSystem
          .readFile({ cwd, relativePath: "asset.bin" })
          .pipe(Effect.flip);
        const resolvedPath = yield* fileSystem.realPath(absolutePath);

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspaceBinaryFileError);
        expect(error).toMatchObject({
          workspaceRoot: cwd,
          relativePath: "asset.bin",
          resolvedPath,
        });
        expect("cause" in error).toBe(false);
        expect("contents" in error).toBe(false);
      }),
    );

    it.effect("preserves the real cause and path for I/O failures", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;
        const resolvedPath = path.join(cwd, "missing.txt");

        const error = yield* workspaceFileSystem
          .readFile({ cwd, relativePath: "missing.txt" })
          .pipe(Effect.flip);

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspaceFileSystemOperationError);
        expect(error).toMatchObject({
          workspaceRoot: cwd,
          relativePath: "missing.txt",
          resolvedPath,
          operationPath: resolvedPath,
          operation: "realpath-target",
        });
        expect(error.cause).toBeInstanceOf(Error);
        expect((error.cause as NodeJS.ErrnoException).code).toBe("ENOENT");
      }),
    );
  });

  describe("writeFile", () => {
    it.effect("writes files relative to the workspace root", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const result = yield* workspaceFileSystem.writeFile({
          cwd,
          relativePath: "plans/effect-rpc.md",
          contents: "# Plan\n",
        });
        const saved = yield* fileSystem
          .readFileString(path.join(cwd, "plans/effect-rpc.md"))
          .pipe(Effect.orDie);

        expect(result).toEqual({
          relativePath: "plans/effect-rpc.md",
          revision: fileContentRevision("# Plan\n"),
        });
        expect(saved).toBe("# Plan\n");
      }),
    );

    it.effect("invalidates workspace entry search cache after writes", () =>
      Effect.gen(function* () {
        const workspaceEntries = yield* WorkspaceEntries.WorkspaceEntries;
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/existing.ts", "export {};\n");

        const beforeWrite = yield* workspaceEntries.list({ cwd });
        expect(beforeWrite.entries.some((entry) => entry.path === "plans/effect-rpc.md")).toBe(
          false,
        );

        yield* workspaceFileSystem.writeFile({
          cwd,
          relativePath: "plans/effect-rpc.md",
          contents: "# Plan\n",
        });

        const afterWrite = yield* workspaceEntries.list({ cwd });
        expect(afterWrite.entries).toEqual(
          expect.arrayContaining([expect.objectContaining({ path: "plans/effect-rpc.md" })]),
        );
        expect(afterWrite.truncated).toBe(false);
      }),
    );

    it.effect("accepts writes whose baseRevision matches the disk contents", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/index.ts", "const a = 1;\n");

        const loaded = yield* workspaceFileSystem.readFile({ cwd, relativePath: "src/index.ts" });
        const result = yield* workspaceFileSystem.writeFile({
          cwd,
          relativePath: "src/index.ts",
          contents: "const a = 2;\n",
          baseRevision: loaded.revision,
        });

        expect(result).toEqual({
          relativePath: "src/index.ts",
          revision: fileContentRevision("const a = 2;\n"),
        });
      }),
    );

    it.effect("rejects writes whose baseRevision is stale", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/index.ts", "const a = 1;\n");

        const loaded = yield* workspaceFileSystem.readFile({ cwd, relativePath: "src/index.ts" });
        // Simulate an agent/terminal rewriting the file behind the buffer.
        yield* writeTextFile(cwd, "src/index.ts", "const a = 99;\n");

        const error = yield* workspaceFileSystem
          .writeFile({
            cwd,
            relativePath: "src/index.ts",
            contents: "const a = 2;\n",
            baseRevision: loaded.revision,
          })
          .pipe(Effect.flip);

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspaceFileStaleRevisionError);
        expect(error).toMatchObject({
          workspaceRoot: cwd,
          relativePath: "src/index.ts",
          expectedRevision: loaded.revision,
          actualRevision: fileContentRevision("const a = 99;\n"),
        });

        const saved = yield* fileSystem
          .readFileString(path.join(cwd, "src/index.ts"))
          .pipe(Effect.orDie);
        expect(saved).toBe("const a = 99;\n");
      }),
    );

    it.effect("rejects baseRevision writes when the file was deleted externally", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/index.ts", "const a = 1;\n");

        const loaded = yield* workspaceFileSystem.readFile({ cwd, relativePath: "src/index.ts" });
        yield* fileSystem.remove(path.join(cwd, "src/index.ts")).pipe(Effect.orDie);

        const error = yield* workspaceFileSystem
          .writeFile({
            cwd,
            relativePath: "src/index.ts",
            contents: "const a = 2;\n",
            baseRevision: loaded.revision,
          })
          .pipe(Effect.flip);

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspaceFileStaleRevisionError);
        expect(error).toMatchObject({
          workspaceRoot: cwd,
          relativePath: "src/index.ts",
          expectedRevision: loaded.revision,
        });
        expect("actualRevision" in error && error.actualRevision !== undefined).toBe(false);
      }),
    );

    it.effect("writes unconditionally when no baseRevision is supplied", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/index.ts", "const a = 1;\n");

        yield* workspaceFileSystem.writeFile({
          cwd,
          relativePath: "src/index.ts",
          contents: "const a = 2;\n",
        });

        const saved = yield* fileSystem
          .readFileString(path.join(cwd, "src/index.ts"))
          .pipe(Effect.orDie);
        expect(saved).toBe("const a = 2;\n");
      }),
    );

    it.effect("rejects writes outside the workspace root", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;
        const path = yield* Path.Path;
        const fileSystem = yield* FileSystem.FileSystem;

        const error = yield* workspaceFileSystem
          .writeFile({
            cwd,
            relativePath: "../escape.md",
            contents: "# nope\n",
          })
          .pipe(Effect.flip);

        expect(error.message).toContain(
          "Workspace file path must be relative to the project root: ../escape.md",
        );

        const escapedPath = path.resolve(cwd, "..", "escape.md");
        const escapedStat = yield* fileSystem
          .stat(escapedPath)
          .pipe(Effect.orElseSucceed(() => null));
        expect(escapedStat).toBeNull();
      }),
    );
  });

  describe("mutateEntry", () => {
    it.effect("creates empty files and nested directories", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;

        const created = yield* workspaceFileSystem.mutateEntry({
          _tag: "create",
          cwd,
          relativePath: "src/lib/util.ts",
          kind: "file",
        });
        expect(created.relativePath).toBe("src/lib/util.ts");
        expect(yield* fileSystem.readFileString(path.join(cwd, "src/lib/util.ts"))).toBe("");

        const directory = yield* workspaceFileSystem.mutateEntry({
          _tag: "create",
          cwd,
          relativePath: "docs/guides",
          kind: "directory",
        });
        expect(directory.relativePath).toBe("docs/guides");
        const stat = yield* fileSystem.stat(path.join(cwd, "docs/guides"));
        expect(stat.type).toBe("Directory");
      }),
    );

    it.effect("rejects creating an entry that already exists", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "notes.md", "existing\n");

        const error = yield* workspaceFileSystem
          .mutateEntry({ _tag: "create", cwd, relativePath: "notes.md", kind: "file" })
          .pipe(Effect.flip);

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspaceEntryExistsError);
      }),
    );

    it.effect("renames entries and creates missing destination directories", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "src/old.ts", "contents\n");

        const renamed = yield* workspaceFileSystem.mutateEntry({
          _tag: "rename",
          cwd,
          fromRelativePath: "src/old.ts",
          toRelativePath: "src/nested/new.ts",
        });

        expect(renamed.relativePath).toBe("src/nested/new.ts");
        expect(yield* fileSystem.readFileString(path.join(cwd, "src/nested/new.ts"))).toBe(
          "contents\n",
        );
        expect(yield* fileSystem.exists(path.join(cwd, "src/old.ts"))).toBe(false);
      }),
    );

    it.effect("rejects renames whose source is missing or destination exists", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "a.txt", "a\n");
        yield* writeTextFile(cwd, "b.txt", "b\n");

        const missing = yield* workspaceFileSystem
          .mutateEntry({
            _tag: "rename",
            cwd,
            fromRelativePath: "nope.txt",
            toRelativePath: "c.txt",
          })
          .pipe(Effect.flip);
        expect(missing).toBeInstanceOf(WorkspaceFileSystem.WorkspaceEntryNotFoundError);

        const collision = yield* workspaceFileSystem
          .mutateEntry({ _tag: "rename", cwd, fromRelativePath: "a.txt", toRelativePath: "b.txt" })
          .pipe(Effect.flip);
        expect(collision).toBeInstanceOf(WorkspaceFileSystem.WorkspaceEntryExistsError);
      }),
    );

    it.effect("deletes files and directories recursively", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const cwd = yield* makeTempDir;
        yield* writeTextFile(cwd, "trash/inner/file.txt", "x\n");

        const deleted = yield* workspaceFileSystem.mutateEntry({
          _tag: "delete",
          cwd,
          relativePath: "trash",
        });

        expect(deleted.relativePath).toBe("trash");
        expect(yield* fileSystem.exists(path.join(cwd, "trash"))).toBe(false);
      }),
    );

    it.effect("rejects deleting a missing entry", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;

        const error = yield* workspaceFileSystem
          .mutateEntry({ _tag: "delete", cwd, relativePath: "ghost.txt" })
          .pipe(Effect.flip);

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspaceEntryNotFoundError);
      }),
    );

    it.effect("rejects mutations that escape the workspace root", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const cwd = yield* makeTempDir;

        const create = yield* workspaceFileSystem
          .mutateEntry({ _tag: "create", cwd, relativePath: "../escape.txt", kind: "file" })
          .pipe(Effect.flip);
        expect(create.message).toContain(
          "Workspace file path must be relative to the project root: ../escape.txt",
        );

        const renameOut = yield* workspaceFileSystem
          .mutateEntry({
            _tag: "rename",
            cwd,
            fromRelativePath: "a.txt",
            toRelativePath: "../stolen.txt",
          })
          .pipe(Effect.flip);
        expect(renameOut.message).toContain(
          "Workspace file path must be relative to the project root: ../stolen.txt",
        );

        // The workspace root itself is never a valid mutation target.
        const deleteRoot = yield* workspaceFileSystem
          .mutateEntry({ _tag: "delete", cwd, relativePath: "." })
          .pipe(Effect.flip);
        expect(deleteRoot.message).toContain(
          "Workspace file path must be relative to the project root: .",
        );
      }),
    );
  });

  describe("copyEntry", () => {
    it.effect("copies a file from one workspace root to another", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const fromCwd = yield* makeTempDir;
        const toCwd = yield* makeTempDir;
        yield* writeTextFile(fromCwd, "src/config.ts", "export const port = 3000;\n");

        const result = yield* workspaceFileSystem.copyEntry({
          fromCwd,
          fromRelativePath: "src/config.ts",
          toCwd,
          toRelativePath: "config.ts",
        });

        expect(result).toEqual({ relativePath: "config.ts" });
        const copied = yield* fileSystem
          .readFileString(path.join(toCwd, "config.ts"))
          .pipe(Effect.orDie);
        expect(copied).toBe("export const port = 3000;\n");
        // Source is left untouched.
        const original = yield* fileSystem
          .readFileString(path.join(fromCwd, "src/config.ts"))
          .pipe(Effect.orDie);
        expect(original).toBe("export const port = 3000;\n");
      }),
    );

    it.effect("copies a directory recursively across roots", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const fromCwd = yield* makeTempDir;
        const toCwd = yield* makeTempDir;
        yield* writeTextFile(fromCwd, "assets/a.txt", "a\n");
        yield* writeTextFile(fromCwd, "assets/nested/b.txt", "b\n");

        const result = yield* workspaceFileSystem.copyEntry({
          fromCwd,
          fromRelativePath: "assets",
          toCwd,
          toRelativePath: "assets",
        });

        expect(result).toEqual({ relativePath: "assets" });
        expect(
          yield* fileSystem.readFileString(path.join(toCwd, "assets/a.txt")).pipe(Effect.orDie),
        ).toBe("a\n");
        expect(
          yield* fileSystem
            .readFileString(path.join(toCwd, "assets/nested/b.txt"))
            .pipe(Effect.orDie),
        ).toBe("b\n");
      }),
    );

    it.effect("creates missing destination parent directories", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const fromCwd = yield* makeTempDir;
        const toCwd = yield* makeTempDir;
        yield* writeTextFile(fromCwd, "note.md", "hi\n");

        yield* workspaceFileSystem.copyEntry({
          fromCwd,
          fromRelativePath: "note.md",
          toCwd,
          toRelativePath: "docs/deep/note.md",
        });

        expect(
          yield* fileSystem
            .readFileString(path.join(toCwd, "docs/deep/note.md"))
            .pipe(Effect.orDie),
        ).toBe("hi\n");
      }),
    );

    it.effect("fails when the source does not exist", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fromCwd = yield* makeTempDir;
        const toCwd = yield* makeTempDir;

        const error = yield* workspaceFileSystem
          .copyEntry({
            fromCwd,
            fromRelativePath: "missing.ts",
            toCwd,
            toRelativePath: "missing.ts",
          })
          .pipe(Effect.flip);

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspaceEntryNotFoundError);
      }),
    );

    it.effect("fails when the destination exists without overwrite", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fromCwd = yield* makeTempDir;
        const toCwd = yield* makeTempDir;
        yield* writeTextFile(fromCwd, "dup.ts", "source\n");
        yield* writeTextFile(toCwd, "dup.ts", "existing\n");

        const error = yield* workspaceFileSystem
          .copyEntry({ fromCwd, fromRelativePath: "dup.ts", toCwd, toRelativePath: "dup.ts" })
          .pipe(Effect.flip);

        expect(error).toBeInstanceOf(WorkspaceFileSystem.WorkspaceEntryExistsError);
      }),
    );

    it.effect("replaces the destination when overwrite is set", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fileSystem = yield* FileSystem.FileSystem;
        const path = yield* Path.Path;
        const fromCwd = yield* makeTempDir;
        const toCwd = yield* makeTempDir;
        yield* writeTextFile(fromCwd, "dup.ts", "source\n");
        yield* writeTextFile(toCwd, "dup.ts", "existing\n");

        const result = yield* workspaceFileSystem.copyEntry({
          fromCwd,
          fromRelativePath: "dup.ts",
          toCwd,
          toRelativePath: "dup.ts",
          overwrite: true,
        });

        expect(result).toEqual({ relativePath: "dup.ts" });
        expect(
          yield* fileSystem.readFileString(path.join(toCwd, "dup.ts")).pipe(Effect.orDie),
        ).toBe("source\n");
      }),
    );

    it.effect("rejects destinations outside the workspace root", () =>
      Effect.gen(function* () {
        const workspaceFileSystem = yield* WorkspaceFileSystem.WorkspaceFileSystem;
        const fromCwd = yield* makeTempDir;
        const toCwd = yield* makeTempDir;
        yield* writeTextFile(fromCwd, "secret.txt", "secret\n");

        const error = yield* workspaceFileSystem
          .copyEntry({
            fromCwd,
            fromRelativePath: "secret.txt",
            toCwd,
            toRelativePath: "../stolen.txt",
          })
          .pipe(Effect.flip);

        expect(error.message).toContain(
          "Workspace file path must be relative to the project root: ../stolen.txt",
        );
      }),
    );
  });
});
