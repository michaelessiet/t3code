import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { yaml } from "@codemirror/lang-yaml";
import type { Extension } from "@codemirror/state";

export type EditorLanguageId =
  | "javascript"
  | "jsx"
  | "typescript"
  | "tsx"
  | "json"
  | "css"
  | "html"
  | "markdown"
  | "python"
  | "rust"
  | "yaml"
  | "plain";

const LANGUAGE_IDS_BY_EXTENSION: Record<string, EditorLanguageId> = {
  js: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  jsx: "jsx",
  ts: "typescript",
  mts: "typescript",
  cts: "typescript",
  tsx: "tsx",
  json: "json",
  jsonc: "json",
  css: "css",
  html: "html",
  htm: "html",
  md: "markdown",
  mdx: "markdown",
  markdown: "markdown",
  py: "python",
  rs: "rust",
  yaml: "yaml",
  yml: "yaml",
};

/**
 * Language id for a workspace-relative path, derived from its file
 * extension. Unknown or missing extensions resolve to `plain`.
 */
export function editorLanguageIdForPath(relativePath: string): EditorLanguageId {
  const normalizedPath = relativePath.replaceAll("\\", "/");
  const fileName = normalizedPath.slice(normalizedPath.lastIndexOf("/") + 1).toLowerCase();
  const extensionIndex = fileName.lastIndexOf(".");
  if (extensionIndex <= 0 || extensionIndex >= fileName.length - 1) {
    return "plain";
  }
  return LANGUAGE_IDS_BY_EXTENSION[fileName.slice(extensionIndex + 1)] ?? "plain";
}

function languageExtensionForId(languageId: EditorLanguageId): Extension {
  switch (languageId) {
    case "javascript":
      return javascript();
    case "jsx":
      return javascript({ jsx: true });
    case "typescript":
      return javascript({ typescript: true });
    case "tsx":
      return javascript({ typescript: true, jsx: true });
    case "json":
      return json();
    case "css":
      return css();
    case "html":
      return html();
    case "markdown":
      return markdown();
    case "python":
      return python();
    case "rust":
      return rust();
    case "yaml":
      return yaml();
    case "plain":
      return [];
  }
}

/**
 * CodeMirror language support for a workspace-relative path; an empty
 * extension when the file has no recognized language.
 */
export function languageExtensionForPath(relativePath: string): Extension {
  return languageExtensionForId(editorLanguageIdForPath(relativePath));
}
