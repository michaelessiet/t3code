import { trimLinkTrailingDelimiters } from "./terminal-links";

/**
 * Marks anchors this plugin created, so the renderer can tell an agent's bare
 * prose mention from a link the author wrote by hand and keep copy round-trips
 * faithful to the original text.
 */
export const FILE_PATH_AUTOLINK_PROPERTY = "dataFilePathAutolink";

/**
 * Path-shaped tokens agents write in prose without markdown link syntax:
 * `apps/web/src/foo.ts:12`, `./scripts/dev.ts`, `/Users/me/project/AGENTS.md`,
 * or a bare `package.json`. Deliberately permissive — every candidate is
 * verified against the workspace file list before it becomes a link, so an
 * over-eager match costs nothing but a failed lookup.
 */
const BARE_FILE_PATH_PATTERN = new RegExp(
  [
    // Absolute, home-relative, or dot-relative paths.
    String.raw`(?:~\/|\.{1,2}\/|\/|[A-Za-z]:[\\/]|\\\\)[A-Za-z0-9._-]+(?:[\\/][A-Za-z0-9._-]+)*(?::\d+){0,2}`,
    // Multi-segment relative paths.
    String.raw`[A-Za-z0-9._-]+(?:[\\/][A-Za-z0-9._-]+)+(?::\d+){0,2}`,
    // Bare filenames carrying an extension, which only resolve at the workspace root.
    String.raw`[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)+(?::\d+){0,2}`,
  ].join("|"),
  "g",
);

/**
 * URLs are excluded wholesale: a repository link such as
 * `github.com/org/repo/blob/main/apps/web/src/foo.ts` contains a genuine
 * workspace path, and linkifying that fragment would split the URL apart.
 */
const URL_LIKE_PATTERN = /(?:[A-Za-z][A-Za-z0-9+.-]*:\/\/|www\.)[^\s"'`<>]+/g;

/**
 * Node types whose text is not prose. Fenced and inline code carry their
 * content as a value rather than child text nodes, so they are skipped
 * naturally; links and images are listed because their labels *are* text
 * nodes and must be left alone.
 */
const SKIPPED_NODE_TYPES = new Set([
  "code",
  "definition",
  "html",
  "image",
  "imageReference",
  "inlineCode",
  "link",
  "linkReference",
  "yaml",
]);

interface MarkdownAstNode {
  type?: string;
  value?: unknown;
  url?: string;
  data?: {
    hProperties?: Record<string, unknown>;
  };
  children?: MarkdownAstNode[];
}

interface TextSpan {
  readonly start: number;
  readonly end: number;
}

export interface RemarkLinkifyFilePathsOptions {
  /**
   * Maps a path-shaped token to the absolute path it should open, or null when
   * the token cannot be confirmed to name a real file.
   */
  readonly resolve: (candidate: string) => string | null;
}

function urlSpans(value: string): TextSpan[] {
  const spans: TextSpan[] = [];
  for (const match of value.matchAll(URL_LIKE_PATTERN)) {
    if (match.index === undefined) continue;
    spans.push({ start: match.index, end: match.index + match[0].length });
  }
  return spans;
}

function linkifyText(value: string, resolve: (candidate: string) => string | null) {
  const excludedSpans = urlSpans(value);
  const nodes: MarkdownAstNode[] = [];
  let cursor = 0;

  for (const match of value.matchAll(BARE_FILE_PATH_PATTERN)) {
    const raw = match[0];
    const start = match.index;
    if (start === undefined || start < cursor) continue;
    if (excludedSpans.some((span) => start < span.end && span.start < start + raw.length)) continue;

    const candidate = trimLinkTrailingDelimiters(raw);
    if (candidate.length === 0) continue;

    const targetPath = resolve(candidate);
    if (!targetPath) continue;

    if (start > cursor) {
      nodes.push({ type: "text", value: value.slice(cursor, start) });
    }
    nodes.push({
      type: "link",
      url: targetPath,
      // The path the agent wrote is the label — rewriting it would edit prose.
      children: [{ type: "text", value: candidate }],
      data: { hProperties: { [FILE_PATH_AUTOLINK_PROPERTY]: "true" } },
    });
    cursor = start + candidate.length;
  }

  if (nodes.length === 0) return null;
  if (cursor < value.length) {
    nodes.push({ type: "text", value: value.slice(cursor) });
  }
  return nodes;
}

/**
 * Turns bare file paths in chat prose into links, so a path an agent mentions
 * is clickable without the agent having to write markdown link syntax. Only
 * paths the `resolve` option confirms are linked; everything else stays as
 * written.
 */
export function remarkLinkifyFilePaths({ resolve }: RemarkLinkifyFilePathsOptions) {
  const transform = (node: MarkdownAstNode) => {
    if (!node.children) return;
    node.children = node.children.flatMap((child) => {
      if (SKIPPED_NODE_TYPES.has(child.type ?? "")) return [child];
      if (child.type === "text" && typeof child.value === "string") {
        return linkifyText(child.value, resolve) ?? [child];
      }
      transform(child);
      return [child];
    });
  };

  return (tree: MarkdownAstNode) => {
    transform(tree);
  };
}
