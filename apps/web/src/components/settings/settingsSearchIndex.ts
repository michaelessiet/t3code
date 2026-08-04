import type { SettingsSectionPath } from "./SettingsSidebarNav";

export interface SettingsSearchEntry {
  readonly title: string;
  readonly keywords: ReadonlyArray<string>;
  readonly to: SettingsSectionPath;
  readonly section: string;
}

interface SettingsSearchSection {
  readonly to: SettingsSectionPath;
  readonly section: string;
  readonly entries: ReadonlyArray<{ title: string; keywords?: ReadonlyArray<string> }>;
}

/**
 * Static index behind the settings search box. Titles should match the
 * `SettingsRow`/section titles rendered on each page so results read the way
 * the destination does; keywords catch the synonyms people actually type.
 */
const SETTINGS_SEARCH_SECTIONS: ReadonlyArray<SettingsSearchSection> = [
  {
    to: "/settings/general",
    section: "General",
    entries: [
      { title: "Theme", keywords: ["dark", "light", "appearance", "color scheme"] },
      { title: "Time format", keywords: ["timestamp", "clock", "24 hour", "relative"] },
      { title: "Vim mode", keywords: ["editor", "keybindings", "modal"] },
      { title: "Word wrap", keywords: ["editor", "lines", "wrap"] },
      {
        title: "Auto save",
        keywords: ["autosave", "save", "editor", "manual save", "cmd+s", "ctrl+s", "dirty"],
      },
      {
        title: "Auto save trigger",
        keywords: ["autosave", "focus", "blur", "delay", "on focus change", "save mode"],
      },
      {
        title: "Auto save delay",
        keywords: ["autosave", "debounce", "milliseconds", "delay", "save timing"],
      },
      {
        title: "Open files in external editor",
        keywords: ["cursor", "vs code", "vscode", "zed", "open in", "third-party", "file links"],
      },
      {
        title: "File conflict warning",
        keywords: ["changed on disk", "reload", "banner", "editor", "external change"],
      },
      { title: "Hide whitespace changes", keywords: ["diff", "whitespace", "ignore"] },
      { title: "Assistant output", keywords: ["streaming", "tokens", "response"] },
      { title: "Provider update checks", keywords: ["agents", "cli", "version"] },
      { title: "Auto-open task panel", keywords: ["plan", "sidebar", "proposed"] },
      { title: "New threads", keywords: ["worktree", "local", "environment", "mode"] },
      { title: "Start from origin", keywords: ["worktree", "branch", "git"] },
      { title: "Add project starts in", keywords: ["base directory", "folder", "browse"] },
      { title: "Archive confirmation", keywords: ["confirm", "thread"] },
      { title: "Delete confirmation", keywords: ["confirm", "thread"] },
      { title: "Text generation model", keywords: ["git", "commit message", "titles", "model"] },
      { title: "Update track", keywords: ["stable", "canary", "app updates"] },
      { title: "Diagnostics", keywords: ["logs", "traces", "debug"] },
    ],
  },
  {
    to: "/settings/keybindings",
    section: "Keybindings",
    entries: [
      {
        title: "Keyboard shortcuts",
        keywords: ["keybindings", "hotkeys", "keymap", "rebind", "shortcut", "when"],
      },
    ],
  },
  {
    to: "/settings/providers",
    section: "Providers",
    entries: [
      {
        title: "Agent providers",
        keywords: ["codex", "claude", "cursor", "opencode", "api key", "models", "instances"],
      },
    ],
  },
  {
    to: "/settings/language-servers",
    section: "Language Servers",
    entries: [
      {
        title: "Language servers",
        keywords: ["lsp", "intellisense", "completions", "custom server", "typescript"],
      },
      { title: "Built-in servers", keywords: ["typescript", "rust", "python", "go", "status"] },
    ],
  },
  {
    to: "/settings/knowledge-graph",
    section: "Knowledge Graph",
    entries: [
      {
        title: "Knowledge graph",
        keywords: ["graphify", "graph", "python", "codebase map", "enable", "install"],
      },
      {
        title: "Graph storage",
        keywords: ["retention", "cache", "disk", "cleanup", "size budget"],
      },
    ],
  },
  {
    to: "/settings/source-control",
    section: "Source Control",
    entries: [
      { title: "Version Control", keywords: ["git", "fetch interval", "branches"] },
      { title: "Source Control Providers", keywords: ["github", "gitlab", "tokens", "remote"] },
    ],
  },
  {
    to: "/settings/connections",
    section: "Connections",
    entries: [
      { title: "T3 Connect", keywords: ["relay", "cloud", "publish activity"] },
      { title: "Network access", keywords: ["pairing", "devices", "remote", "tailscale", "wsl"] },
    ],
  },
  {
    to: "/settings/archived",
    section: "Archive",
    entries: [{ title: "Archived threads", keywords: ["restore", "deleted", "history"] }],
  },
];

export const SETTINGS_SEARCH_ENTRIES: ReadonlyArray<SettingsSearchEntry> =
  SETTINGS_SEARCH_SECTIONS.flatMap((section) =>
    section.entries.map((entry) => ({
      title: entry.title,
      keywords: entry.keywords ?? [],
      to: section.to,
      section: section.section,
    })),
  );

export function searchSettings(query: string): ReadonlyArray<SettingsSearchEntry> {
  const normalized = query.trim().toLowerCase();
  if (normalized.length === 0) return [];
  return SETTINGS_SEARCH_ENTRIES.filter(
    (entry) =>
      entry.title.toLowerCase().includes(normalized) ||
      entry.section.toLowerCase().includes(normalized) ||
      entry.keywords.some((keyword) => keyword.includes(normalized)),
  );
}
