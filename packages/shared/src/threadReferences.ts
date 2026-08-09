import type { ThreadId } from "@t3tools/contracts";

/**
 * Cross-thread references travel inside message text as ordinary markdown
 * links with an app-internal destination:
 *
 *   [#Fix auth bug](t3code://thread/thr_abc123)
 *
 * The composer inserts them via `serializeComposerThreadReference`, the web
 * renderer turns them into clickable chips, and the server resolves them to
 * inlined transcripts before the turn reaches the provider. Keeping the
 * reference in plain text means it flows through the event store, projections
 * and provider sessions without any schema changes.
 */
export const THREAD_REFERENCE_URL_PREFIX = "t3code://thread/";

export interface ThreadReference {
  readonly threadId: string;
  readonly title: string;
  readonly source: string;
  readonly start: number;
  readonly end: number;
}

const THREAD_REFERENCE_TOKEN_REGEX =
  /(^|\s)\[#((?:\\.|[^\]\\])*)\]\(t3code:\/\/thread\/([^)\s]+)\)(?=\s|$)/g;

function unescapeMarkdownLinkLabel(label: string): string {
  return label.replace(/\\(.)/g, "$1");
}

function escapeMarkdownLinkLabel(label: string): string {
  return label.replaceAll("\\", "\\\\").replaceAll("[", "\\[").replaceAll("]", "\\]");
}

function safeDecodeThreadId(encoded: string): string {
  try {
    return decodeURIComponent(encoded);
  } catch {
    return encoded;
  }
}

export function buildThreadReferenceUrl(threadId: ThreadId | string): string {
  return `${THREAD_REFERENCE_URL_PREFIX}${encodeURIComponent(threadId)}`;
}

/**
 * Parse a markdown link destination as a thread reference. Returns the thread
 * id or null when the href points elsewhere.
 */
export function parseThreadReferenceUrl(href: string | undefined | null): string | null {
  if (!href) return null;
  const trimmed = href.trim();
  if (!trimmed.toLowerCase().startsWith(THREAD_REFERENCE_URL_PREFIX)) return null;
  const encoded = trimmed.slice(THREAD_REFERENCE_URL_PREFIX.length);
  if (encoded.length === 0) return null;
  const threadId = safeDecodeThreadId(encoded);
  return threadId.trim().length > 0 ? threadId : null;
}

/** Serialize a thread reference the way the composer inserts it. */
export function serializeComposerThreadReference(input: {
  readonly threadId: ThreadId | string;
  readonly title: string;
}): string {
  return `[#${escapeMarkdownLinkLabel(input.title)}](${buildThreadReferenceUrl(input.threadId)})`;
}

/** Collect every thread reference token embedded in message text. */
export function collectThreadReferences(text: string): ReadonlyArray<ThreadReference> {
  const references: ThreadReference[] = [];
  for (const match of text.matchAll(THREAD_REFERENCE_TOKEN_REGEX)) {
    const prefix = match[1] ?? "";
    const title = unescapeMarkdownLinkLabel(match[2] ?? "");
    const threadId = safeDecodeThreadId(match[3] ?? "");
    if (threadId.trim().length === 0) continue;
    const start = (match.index ?? 0) + prefix.length;
    const end = start + match[0].length - prefix.length;
    references.push({
      threadId,
      title,
      source: text.slice(start, end),
      start,
      end,
    });
  }
  return references;
}
