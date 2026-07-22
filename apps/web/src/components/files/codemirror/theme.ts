import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import type { Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { tags } from "@lezer/highlight";

/**
 * Editor colors resolve through the app's CSS custom properties (see
 * `index.css` and the DiffPanel token mapping) instead of baked-in values, so
 * the surface stays in sync with the design tokens at runtime. Light/dark is
 * handled by `light-dark()`: the app toggles `color-scheme` alongside the
 * `.dark` class, which flips every value below without reconfiguring the
 * editor.
 */
function themedColor(light: string, dark: string): string {
  return `light-dark(${light}, ${dark})`;
}

function mixedColor(token: string, percentage: number, base = "transparent"): string {
  return `color-mix(in srgb, var(${token}) ${percentage}%, ${base})`;
}

const SELECTION_BACKGROUND = themedColor(mixedColor("--primary", 18), mixedColor("--primary", 30));
const INACTIVE_SELECTION_BACKGROUND = themedColor(
  mixedColor("--foreground", 8),
  mixedColor("--foreground", 12),
);
const REVEAL_LINE_BACKGROUND = themedColor(
  mixedColor("--primary", 10),
  mixedColor("--primary", 16),
);
const REVIEW_SELECTION_BACKGROUND = themedColor(
  mixedColor("--warning", 14),
  mixedColor("--warning", 18),
);

const editorChrome = EditorView.theme({
  "&": {
    height: "100%",
    backgroundColor: "var(--background)",
    color: "var(--foreground)",
    fontSize: "12px",
  },
  "&.cm-focused": {
    outline: "none",
  },
  ".cm-scroller": {
    fontFamily: "var(--font-mono)",
    lineHeight: "1.6",
  },
  ".cm-content": {
    caretColor: "var(--foreground)",
    paddingBlock: "8px",
  },
  ".cm-line": {
    paddingInline: "12px",
  },
  ".cm-cursor, .cm-dropCursor": {
    borderLeftColor: "var(--foreground)",
  },
  ".cm-selectionBackground": {
    backgroundColor: INACTIVE_SELECTION_BACKGROUND,
  },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
    backgroundColor: SELECTION_BACKGROUND,
  },
  ".cm-activeLine": {
    backgroundColor: mixedColor("--foreground", 3),
  },
  ".cm-gutters": {
    backgroundColor: "var(--background)",
    color: mixedColor("--muted-foreground", 75),
    border: "none",
  },
  ".cm-lineNumbers .cm-gutterElement": {
    paddingInline: "12px 10px",
    minWidth: "40px",
    cursor: "pointer",
    userSelect: "none",
  },
  ".cm-activeLineGutter": {
    backgroundColor: mixedColor("--foreground", 4),
    color: "var(--muted-foreground)",
  },
  ".cm-matchingBracket, &.cm-focused .cm-matchingBracket": {
    backgroundColor: mixedColor("--primary", 14),
    outline: `1px solid ${mixedColor("--primary", 35)}`,
  },
  ".cm-nonmatchingBracket, &.cm-focused .cm-nonmatchingBracket": {
    backgroundColor: mixedColor("--destructive", 14),
  },
  ".cm-searchMatch": {
    backgroundColor: mixedColor("--warning", 25),
    outline: `1px solid ${mixedColor("--warning", 40)}`,
  },
  ".cm-searchMatch.cm-searchMatch-selected": {
    backgroundColor: mixedColor("--warning", 45),
  },
  ".cm-selectionMatch": {
    backgroundColor: mixedColor("--primary", 10),
  },
  ".cm-reveal-line": {
    backgroundColor: REVEAL_LINE_BACKGROUND,
  },
  ".cm-review-selected-line": {
    backgroundColor: REVIEW_SELECTION_BACKGROUND,
  },
  ".cm-review-selected-gutter": {
    backgroundColor: REVIEW_SELECTION_BACKGROUND,
    color: "var(--foreground)",
  },
  ".cm-review-annotation": {
    fontFamily: "var(--font-sans)",
    whiteSpace: "normal",
    userSelect: "text",
  },
  ".cm-panels": {
    backgroundColor: "var(--card)",
    color: "var(--card-foreground)",
    fontFamily: "var(--font-sans)",
    fontSize: "12px",
  },
  ".cm-panels.cm-panels-top": {
    borderBottom: "1px solid var(--border)",
  },
  ".cm-panels.cm-panels-bottom": {
    borderTop: "1px solid var(--border)",
  },
  ".cm-panel.cm-search": {
    padding: "6px 10px",
  },
  ".cm-panel.cm-search input, .cm-panel.cm-search button, .cm-panel.cm-search label": {
    fontSize: "12px",
    fontFamily: "var(--font-sans)",
  },
  ".cm-panel.cm-search input": {
    backgroundColor: "var(--background)",
    color: "var(--foreground)",
    border: "1px solid var(--input)",
    borderRadius: "calc(var(--radius) - 4px)",
    outline: "none",
    padding: "2px 6px",
  },
  ".cm-panel.cm-search input:focus-visible": {
    borderColor: "var(--ring)",
  },
  ".cm-panel.cm-search button": {
    backgroundColor: "var(--secondary)",
    backgroundImage: "none",
    color: "var(--secondary-foreground)",
    border: "1px solid var(--border)",
    borderRadius: "calc(var(--radius) - 4px)",
    padding: "2px 8px",
    cursor: "pointer",
  },
  ".cm-panel.cm-search button:hover": {
    backgroundColor: "var(--accent)",
  },
  ".cm-panel.cm-search [name='close']": {
    color: "var(--muted-foreground)",
    border: "none",
    backgroundColor: "transparent",
    fontSize: "14px",
    padding: "0 4px",
  },
  ".cm-tooltip": {
    backgroundColor: "var(--popover)",
    color: "var(--popover-foreground)",
    border: "1px solid var(--border)",
    borderRadius: "calc(var(--radius) - 2px)",
  },
  ".cm-tooltip .cm-tooltip-arrow:before": {
    borderTopColor: "var(--border)",
    borderBottomColor: "var(--border)",
  },
  ".cm-tooltip .cm-tooltip-arrow:after": {
    borderTopColor: "var(--popover)",
    borderBottomColor: "var(--popover)",
  },
  // LSP diagnostic (lint) tooltips: bound width and wrap long messages.
  ".cm-tooltip-lint": {
    maxWidth: "min(44rem, 80vw)",
    maxHeight: "20rem",
    overflow: "auto",
  },
  ".cm-tooltip-lint .cm-diagnostic": {
    padding: "6px 10px 6px 10px",
    fontFamily: "var(--font-sans)",
    fontSize: "12px",
    lineHeight: "1.5",
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
  },
  // LSP hover / signature tooltips: bounded, scrollable, app typography.
  ".cm-lsp-hover, .cm-lsp-signature": {
    maxWidth: "min(44rem, 80vw)",
    maxHeight: "20rem",
    overflow: "auto",
    padding: "8px 10px",
    fontFamily: "var(--font-sans)",
    fontSize: "12px",
    lineHeight: "1.5",
  },
  ".cm-lsp-signature": {
    maxHeight: "12rem",
  },
  ".cm-lsp-markdown .cm-lsp-md-paragraph": {
    margin: "4px 0",
    overflowWrap: "anywhere",
  },
  ".cm-lsp-markdown .cm-lsp-md-code": {
    margin: "6px 0",
    padding: "6px 8px",
    borderRadius: "calc(var(--radius) - 4px)",
    backgroundColor: "color-mix(in srgb, var(--muted) 55%, transparent)",
    fontFamily: "var(--font-mono)",
    fontSize: "12px",
    lineHeight: "1.5",
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
  },
  ".cm-lsp-markdown code": {
    fontFamily: "var(--font-mono)",
    fontSize: "11.5px",
    padding: "1px 4px",
    borderRadius: "4px",
    backgroundColor: "color-mix(in srgb, var(--muted) 55%, transparent)",
  },
  ".cm-lsp-markdown .cm-lsp-md-link": {
    color: "var(--primary)",
    textDecoration: "underline dotted",
    overflowWrap: "anywhere",
  },
  ".cm-lsp-signature-label": {
    fontFamily: "var(--font-mono)",
    fontSize: "12px",
    whiteSpace: "pre-wrap",
    overflowWrap: "anywhere",
  },
  ".cm-lsp-signature-label .cm-lsp-active-param": {
    fontWeight: "700",
    textDecoration: "underline",
    textUnderlineOffset: "3px",
  },
  ".cm-lsp-signature-docs": {
    marginTop: "6px",
    paddingTop: "6px",
    borderTop: "1px solid var(--border)",
    color: "var(--muted-foreground)",
  },
});

/**
 * Syntax palette tuned against the app's neutral light/dark backgrounds.
 * Values are `light-dark()` pairs so a single HighlightStyle serves both
 * themes; token categories without a natural app token use fixed accents.
 */
const editorHighlightStyle = HighlightStyle.define([
  {
    tag: [tags.keyword, tags.moduleKeyword, tags.operatorKeyword, tags.controlKeyword],
    color: themedColor("#cf222e", "#ff7b72"),
  },
  {
    tag: [tags.string, tags.special(tags.string), tags.character, tags.docString],
    color: themedColor("#0a3069", "#a5d6ff"),
  },
  {
    tag: [tags.regexp, tags.escape],
    color: themedColor("#116329", "#7ee787"),
  },
  {
    tag: tags.comment,
    color: "var(--muted-foreground)",
    fontStyle: "italic",
  },
  {
    tag: [tags.number, tags.bool, tags.atom, tags.null, tags.unit],
    color: themedColor("#0550ae", "#79c0ff"),
  },
  {
    tag: [tags.function(tags.variableName), tags.function(tags.propertyName)],
    color: themedColor("#8250df", "#d2a8ff"),
  },
  {
    tag: [tags.typeName, tags.className, tags.namespace],
    color: themedColor("#953800", "#ffa657"),
  },
  {
    tag: [tags.propertyName, tags.attributeName, tags.definition(tags.propertyName)],
    color: themedColor("#0550ae", "#79c0ff"),
  },
  {
    tag: [tags.tagName, tags.angleBracket],
    color: themedColor("#116329", "#7ee787"),
  },
  {
    tag: [tags.meta, tags.processingInstruction, tags.annotation],
    color: "var(--muted-foreground)",
  },
  {
    tag: tags.invalid,
    color: "var(--destructive)",
  },
  { tag: tags.heading, fontWeight: "600" },
  { tag: tags.strong, fontWeight: "600" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  {
    tag: [tags.link, tags.url],
    color: "var(--primary)",
    textDecoration: "underline",
  },
]);

/**
 * Exported for tooltip content rendering (hover docs, signature help): code
 * highlighted outside the editor document reuses the exact same style, so
 * tooltip code matches the buffer's coloring.
 */
export { editorHighlightStyle };

/** App-token-driven CodeMirror theme: editor chrome plus syntax highlighting. */
export const editorTheme: Extension = [editorChrome, syntaxHighlighting(editorHighlightStyle)];
