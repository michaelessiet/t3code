import type { ChatAttachment } from "@t3tools/contracts";

// Keep a single inlined file comfortably below the model context limit while
// still covering large source files and logs.
export const MAX_INLINE_ATTACHMENT_TEXT_CHARS = 200_000;

export function isPdfAttachment(attachment: ChatAttachment): boolean {
  return attachment.type === "file" && attachment.mimeType.toLowerCase() === "application/pdf";
}

// Strict UTF-8 decode; returns null for binary payloads so callers can fall
// back to an "unreadable attachment" note instead of sending mojibake.
export function decodeAttachmentText(bytes: Uint8Array): string | null {
  try {
    const decoder = new TextDecoder("utf-8", { fatal: true });
    const text = decoder.decode(bytes);
    // NUL bytes decode fine but indicate a binary format.
    return text.includes("\u0000") ? null : text;
  } catch {
    return null;
  }
}

export function formatInlineAttachmentText(input: {
  readonly name: string;
  readonly mimeType: string;
  readonly text: string;
}): string {
  const truncated = input.text.length > MAX_INLINE_ATTACHMENT_TEXT_CHARS;
  const body = truncated ? input.text.slice(0, MAX_INLINE_ATTACHMENT_TEXT_CHARS) : input.text;
  const suffix = truncated
    ? `\n[... truncated: file exceeds ${MAX_INLINE_ATTACHMENT_TEXT_CHARS} characters]`
    : "";
  return `<attached-file name=${JSON.stringify(input.name)} mime-type=${JSON.stringify(
    input.mimeType,
  )}>\n${body}${suffix}\n</attached-file>`;
}

export function formatUnreadableAttachmentNote(input: {
  readonly name: string;
  readonly mimeType: string;
  readonly sizeBytes: number;
}): string {
  return `[Attached file ${JSON.stringify(input.name)} (${input.mimeType}, ${input.sizeBytes} bytes) could not be included: unsupported binary format.]`;
}
