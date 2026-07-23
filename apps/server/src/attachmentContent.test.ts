import { describe, expect, it } from "vite-plus/test";

import {
  decodeAttachmentText,
  formatInlineAttachmentText,
  formatUnreadableAttachmentNote,
  isPdfAttachment,
  MAX_INLINE_ATTACHMENT_TEXT_CHARS,
} from "./attachmentContent.ts";

describe("attachmentContent", () => {
  it("decodes UTF-8 payloads to text", () => {
    const bytes = new TextEncoder().encode("const value = 'héllo';\n");
    expect(decodeAttachmentText(bytes)).toBe("const value = 'héllo';\n");
  });

  it("rejects invalid UTF-8 payloads", () => {
    expect(decodeAttachmentText(new Uint8Array([0xff, 0xfe, 0x00, 0x81]))).toBeNull();
  });

  it("rejects payloads containing NUL bytes", () => {
    const bytes = new TextEncoder().encode("binary\u0000payload");
    expect(decodeAttachmentText(bytes)).toBeNull();
  });

  it("wraps inline attachment text with file metadata", () => {
    const formatted = formatInlineAttachmentText({
      name: "example.ts",
      mimeType: "application/typescript",
      text: "export const x = 1;",
    });
    expect(formatted).toContain('<attached-file name="example.ts"');
    expect(formatted).toContain("export const x = 1;");
    expect(formatted).toContain("</attached-file>");
  });

  it("truncates oversized inline attachment text", () => {
    const formatted = formatInlineAttachmentText({
      name: "big.txt",
      mimeType: "text/plain",
      text: "a".repeat(MAX_INLINE_ATTACHMENT_TEXT_CHARS + 10),
    });
    expect(formatted).toContain("[... truncated");
    expect(formatted.length).toBeLessThan(MAX_INLINE_ATTACHMENT_TEXT_CHARS + 500);
  });

  it("identifies pdf attachments", () => {
    expect(
      isPdfAttachment({
        type: "file",
        id: "thread-1-a",
        name: "doc.pdf",
        mimeType: "application/PDF",
        sizeBytes: 10,
      }),
    ).toBe(true);
    expect(
      isPdfAttachment({
        type: "image",
        id: "thread-1-b",
        name: "img.png",
        mimeType: "image/png",
        sizeBytes: 10,
      }),
    ).toBe(false);
  });

  it("describes unreadable attachments", () => {
    expect(
      formatUnreadableAttachmentNote({
        name: "app.bin",
        mimeType: "application/octet-stream",
        sizeBytes: 42,
      }),
    ).toContain('"app.bin" (application/octet-stream, 42 bytes)');
  });
});
