import type rehypeSanitize from "rehype-sanitize";
import { defaultSchema } from "rehype-sanitize";

import { FILE_PATH_AUTOLINK_PROPERTY } from "./markdown-file-path-autolink";

/**
 * Sanitize rules for rendered chat markdown. Any custom attribute a remark
 * plugin attaches has to be allowlisted here or it is dropped before the
 * renderer ever sees it.
 */
export const CHAT_MARKDOWN_SANITIZE_SCHEMA: NonNullable<Parameters<typeof rehypeSanitize>[0]> = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    "*": (defaultSchema.attributes?.["*"] ?? []).filter((attribute) => attribute !== "title"),
    a: [...(defaultSchema.attributes?.a ?? []), FILE_PATH_AUTOLINK_PROPERTY],
    code: [...(defaultSchema.attributes?.code ?? []), "dataCodeMeta"],
  },
  protocols: {
    ...defaultSchema.protocols,
    href: [...(defaultSchema.protocols?.href ?? []), "file"],
  },
} satisfies Parameters<typeof rehypeSanitize>[0];
