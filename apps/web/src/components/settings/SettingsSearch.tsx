import { useNavigate } from "@tanstack/react-router";
import { SearchIcon } from "lucide-react";
import { useMemo, useRef, useState } from "react";

import { cn } from "~/lib/utils";

import { searchSettings, type SettingsSearchEntry } from "./settingsSearchIndex";

/**
 * Search box in the settings header: matches setting rows and sections across
 * every settings page and navigates to the owning page on selection.
 */
export function SettingsSearch() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [focused, setFocused] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const results = useMemo(() => searchSettings(query).slice(0, 8), [query]);
  const open = focused && results.length > 0;
  const highlightedIndex = Math.min(selectedIndex, Math.max(0, results.length - 1));

  const activate = (entry: SettingsSearchEntry) => {
    setQuery("");
    setSelectedIndex(0);
    inputRef.current?.blur();
    void navigate({ to: entry.to });
  };

  return (
    <div className="relative">
      <SearchIcon className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <input
        ref={inputRef}
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setSelectedIndex(0);
        }}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onKeyDown={(event) => {
          // The settings layout closes on Escape; keep it local while typing.
          if (event.key === "Escape") {
            if (query.length > 0) {
              event.preventDefault();
              event.stopPropagation();
              setQuery("");
            }
            return;
          }
          if (event.key === "ArrowDown") {
            event.preventDefault();
            setSelectedIndex((index) => Math.min(index + 1, results.length - 1));
            return;
          }
          if (event.key === "ArrowUp") {
            event.preventDefault();
            setSelectedIndex((index) => Math.max(0, index - 1));
            return;
          }
          if (event.key === "Enter") {
            const entry = results[highlightedIndex];
            if (entry !== undefined) {
              event.preventDefault();
              activate(entry);
            }
          }
        }}
        placeholder="Search settings…"
        aria-label="Search settings"
        spellCheck={false}
        className="h-7 w-44 rounded-md border border-border bg-background pl-7 pr-2 text-xs outline-none transition-[width] duration-150 placeholder:text-muted-foreground focus:w-64 focus:border-ring"
      />
      {open ? (
        <div className="absolute right-0 top-full z-50 mt-1 w-72 rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md">
          {results.map((entry, index) => (
            <button
              key={`${entry.to}:${entry.title}`}
              type="button"
              className={cn(
                "flex w-full items-baseline justify-between gap-2 rounded-sm px-2 py-1.5 text-left text-xs",
                index === highlightedIndex
                  ? "bg-accent text-accent-foreground"
                  : "hover:bg-accent/60",
              )}
              // Fire before the input's blur unmounts the dropdown.
              onMouseDown={(event) => {
                event.preventDefault();
                activate(entry);
              }}
              onMouseMove={() => setSelectedIndex(index)}
            >
              <span className="truncate">{entry.title}</span>
              <span className="shrink-0 text-[10px] text-muted-foreground">{entry.section}</span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
