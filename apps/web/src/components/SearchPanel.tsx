import type { EnvironmentId, ProjectSearchContentInput } from "@t3tools/contracts";
import * as Option from "effect/Option";
import { AsyncResult } from "effect/unstable/reactivity";
import {
  CaseSensitive,
  ChevronDown,
  ChevronRight,
  Regex,
  Replace,
  ReplaceAll,
  SlidersHorizontal,
  WholeWord,
} from "lucide-react";
import { useCallback, useMemo, useState } from "react";

import {
  AlertDialog,
  AlertDialogClose,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogPopup,
  AlertDialogTitle,
} from "~/components/ui/alert-dialog";
import { Badge } from "~/components/ui/badge";
import { Button } from "~/components/ui/button";
import { Input } from "~/components/ui/input";
import { ScrollArea } from "~/components/ui/scroll-area";
import { Spinner } from "~/components/ui/spinner";
import { stackedThreadToast, toastManager } from "~/components/ui/toast";
import { Toggle } from "~/components/ui/toggle";
import { Tooltip, TooltipPopup, TooltipTrigger } from "~/components/ui/tooltip";
import { useDebouncedValue } from "~/hooks/useDebouncedValue";
import { cn } from "~/lib/utils";
import { appAtomRegistry } from "~/rpc/atomRegistry";
import { projectEnvironment } from "~/state/projects";
import { useEnvironmentQuery } from "~/state/query";
import { useAtomCommand } from "~/state/use-atom-command";
import { useAtomQueryRunner } from "~/state/use-atom-query-runner";

import { isStaleRevisionWriteFailure } from "./files/fileBufferConflict";
import { getProjectFileQueryAtom } from "./files/projectFilesQueryState";
import {
  type SearchMatchFileGroup,
  buildSearchRegExp,
  computeReplacements,
  groupMatchesByFile,
  matchLineSegments,
  splitSearchResultPath,
} from "./SearchPanel.logic";

interface SearchPanelProps {
  environmentId: EnvironmentId;
  cwd: string;
  onOpenFile: (relativePath: string, line?: number) => void;
}

const SEARCH_DEBOUNCE_MS = 300;
const SEARCH_MAX_RESULTS = 1000;

interface PendingReplace {
  readonly scope: "file" | "all";
  readonly paths: ReadonlyArray<string>;
  readonly matchCount: number;
}

function pluralize(count: number, singular: string, plural: string): string {
  return `${count.toLocaleString()} ${count === 1 ? singular : plural}`;
}

function SearchOptionToggle(props: {
  pressed: boolean;
  onPressedChange: (pressed: boolean) => void;
  label: string;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Toggle
            size="xs"
            variant="ghost"
            pressed={props.pressed}
            onPressedChange={props.onPressedChange}
            aria-label={props.label}
          >
            {props.children}
          </Toggle>
        }
      />
      <TooltipPopup side="bottom">{props.label}</TooltipPopup>
    </Tooltip>
  );
}

function SearchResultGroup(props: {
  group: SearchMatchFileGroup;
  collapsed: boolean;
  replaceEnabled: boolean;
  replacing: boolean;
  onToggleCollapsed: (path: string) => void;
  onOpenMatch: (relativePath: string, line: number) => void;
  onReplaceInFile: (group: SearchMatchFileGroup) => void;
}) {
  const { group } = props;
  const { name, directory } = splitSearchResultPath(group.path);
  const ChevronIcon = props.collapsed ? ChevronRight : ChevronDown;
  return (
    <div data-search-result-file={group.path}>
      <div className="group flex h-6 items-center gap-1.5 rounded-md pr-1 hover:bg-accent/60">
        <button
          type="button"
          className="flex h-full min-w-0 flex-1 items-center gap-1 pl-1 text-left"
          onClick={() => props.onToggleCollapsed(group.path)}
          aria-expanded={!props.collapsed}
        >
          <ChevronIcon className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="truncate text-xs font-medium text-foreground">{name}</span>
          {directory.length > 0 ? (
            <span className="truncate text-[10px] text-muted-foreground">{directory}</span>
          ) : null}
        </button>
        {props.replaceEnabled ? (
          <button
            type="button"
            className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground opacity-0 hover:bg-muted hover:text-foreground focus-visible:opacity-100 group-hover:opacity-100"
            aria-label={`Replace all in ${group.path}`}
            disabled={props.replacing}
            onClick={() => props.onReplaceInFile(group)}
          >
            <Replace className="size-3.5" />
          </button>
        ) : null}
        <Badge variant="secondary" size="sm">
          {group.matches.length}
        </Badge>
      </div>
      {props.collapsed
        ? null
        : group.matches.map((match) => {
            const segments = matchLineSegments(match);
            return (
              <button
                key={`${match.line}:${match.matchStart}`}
                type="button"
                className="flex w-full items-center gap-2 rounded-md py-0.5 pl-5 pr-2 text-left hover:bg-accent/60"
                onClick={() => props.onOpenMatch(group.path, match.line)}
              >
                <span className="w-8 shrink-0 text-right font-mono text-[10px] leading-4 text-muted-foreground">
                  {match.line}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground">
                  {segments.beforeClipped ? "…" : ""}
                  {segments.before}
                  <span className="rounded-xs bg-warning/24 text-foreground">
                    {segments.matched}
                  </span>
                  {segments.after}
                </span>
              </button>
            );
          })}
    </div>
  );
}

export default function SearchPanel({ environmentId, cwd, onOpenFile }: SearchPanelProps) {
  const [queryText, setQueryText] = useState("");
  const [includeGlob, setIncludeGlob] = useState("");
  const [excludeGlob, setExcludeGlob] = useState("");
  const [regex, setRegex] = useState(false);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [replaceText, setReplaceText] = useState("");
  const [collapsedPaths, setCollapsedPaths] = useState<ReadonlySet<string>>(new Set());
  const [pendingReplace, setPendingReplace] = useState<PendingReplace | null>(null);
  const [replacing, setReplacing] = useState(false);

  const typed = useMemo(
    () => ({ query: queryText, includeGlob: includeGlob.trim(), excludeGlob: excludeGlob.trim() }),
    [excludeGlob, includeGlob, queryText],
  );
  const debouncedTyped = useDebouncedValue(typed, SEARCH_DEBOUNCE_MS);

  const searchInput = useMemo<ProjectSearchContentInput | null>(() => {
    if (debouncedTyped.query.length === 0) return null;
    return {
      cwd,
      query: debouncedTyped.query,
      ...(regex ? { regex: true } : {}),
      ...(caseSensitive ? { caseSensitive: true } : {}),
      ...(wholeWord ? { wholeWord: true } : {}),
      ...(debouncedTyped.includeGlob.length > 0 ? { includeGlob: debouncedTyped.includeGlob } : {}),
      ...(debouncedTyped.excludeGlob.length > 0 ? { excludeGlob: debouncedTyped.excludeGlob } : {}),
      maxResults: SEARCH_MAX_RESULTS,
    };
  }, [caseSensitive, cwd, debouncedTyped, regex, wholeWord]);

  const search = useEnvironmentQuery(
    searchInput === null
      ? null
      : projectEnvironment.searchContent({ environmentId, input: searchInput }),
  );
  const refreshSearch = search.refresh;
  const result = search.data;
  const groups = useMemo(
    () => (result === null ? [] : groupMatchesByFile(result.matches)),
    [result],
  );

  // Replacement runs client-side on a JS RegExp mirror of the server pattern;
  // regex-mode queries that JS cannot parse can still search but not replace.
  const clientPattern = searchInput === null ? null : buildSearchRegExp(searchInput);
  const replaceEnabled =
    !replacing && searchInput !== null && clientPattern !== null && groups.length > 0;

  const runReadFile = useAtomQueryRunner(projectEnvironment.readFile, { reportFailure: false });
  const writeFile = useAtomCommand(projectEnvironment.writeFile, { reportFailure: false });

  const toggleCollapsed = useCallback((path: string) => {
    setCollapsedPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  const executeReplace = useCallback(
    async (paths: ReadonlyArray<string>) => {
      if (searchInput === null) return;
      setReplacing(true);
      let replacedMatchCount = 0;
      let replacedFileCount = 0;
      const staleFiles: string[] = [];
      const failedFiles: string[] = [];
      try {
        for (const relativePath of paths) {
          const fileAtom = getProjectFileQueryAtom(environmentId, cwd, relativePath);
          // Re-read from disk so the write's base revision reflects the file
          // as it is now, not as it was when the search ran.
          appAtomRegistry.refresh(fileAtom);
          const read = await runReadFile({ environmentId, input: { cwd, relativePath } });
          const file = read._tag === "Success" ? Option.getOrNull(AsyncResult.value(read)) : null;
          if (file === null || file.truncated) {
            failedFiles.push(relativePath);
            continue;
          }
          const computed = computeReplacements(file.contents, searchInput, replaceText);
          if (computed === null || computed.replacedCount === 0) continue;
          const write = await writeFile({
            environmentId,
            input: {
              cwd,
              relativePath,
              contents: computed.contents,
              ...(file.revision === undefined ? {} : { baseRevision: file.revision }),
            },
          });
          if (write._tag === "Success") {
            replacedMatchCount += computed.replacedCount;
            replacedFileCount += 1;
            appAtomRegistry.refresh(fileAtom);
          } else if (isStaleRevisionWriteFailure(write)) {
            staleFiles.push(relativePath);
          } else {
            failedFiles.push(relativePath);
          }
        }
      } finally {
        setReplacing(false);
        refreshSearch();
      }
      if (replacedFileCount > 0) {
        toastManager.add(
          stackedThreadToast({
            type: "success",
            title: `Replaced ${pluralize(replacedMatchCount, "match", "matches")} in ${pluralize(replacedFileCount, "file", "files")}.`,
          }),
        );
      }
      if (staleFiles.length > 0) {
        toastManager.add(
          stackedThreadToast({
            type: "warning",
            title: "Skipped files that changed on disk",
            description: `${pluralize(staleFiles.length, "file", "files")} changed while replacing and ${staleFiles.length === 1 ? "was" : "were"} skipped. Search results were refreshed.`,
          }),
        );
      }
      if (failedFiles.length > 0) {
        toastManager.add(
          stackedThreadToast({
            type: "error",
            title: "Some files could not be updated",
            description: `Failed to replace in ${pluralize(failedFiles.length, "file", "files")}.`,
          }),
        );
      }
    },
    [cwd, environmentId, refreshSearch, replaceText, runReadFile, searchInput, writeFile],
  );

  const requestReplaceAll = useCallback(() => {
    if (groups.length === 0) return;
    setPendingReplace({
      scope: "all",
      paths: groups.map((group) => group.path),
      matchCount: groups.reduce((count, group) => count + group.matches.length, 0),
    });
  }, [groups]);

  const requestReplaceInFile = useCallback((group: SearchMatchFileGroup) => {
    setPendingReplace({
      scope: "file",
      paths: [group.path],
      matchCount: group.matches.length,
    });
  }, []);

  const confirmPendingReplace = useCallback(() => {
    if (pendingReplace === null) return;
    setPendingReplace(null);
    void executeReplace(pendingReplace.paths);
  }, [executeReplace, pendingReplace]);

  const hasQuery = typed.query.length > 0;
  const isSearching = hasQuery && (search.isPending || typed.query !== debouncedTyped.query);
  const matchCount = result?.matches.length ?? 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-background" data-search-panel>
      <div className="shrink-0 space-y-1.5 border-b border-border/60 p-2">
        <div className="flex items-center gap-1">
          <Input
            type="search"
            size="sm"
            placeholder="Search"
            aria-label="Search query"
            value={queryText}
            onChange={(event) => setQueryText(event.target.value)}
          />
          <SearchOptionToggle
            pressed={caseSensitive}
            onPressedChange={setCaseSensitive}
            label="Match case"
          >
            <CaseSensitive />
          </SearchOptionToggle>
          <SearchOptionToggle
            pressed={wholeWord}
            onPressedChange={setWholeWord}
            label="Match whole word"
          >
            <WholeWord />
          </SearchOptionToggle>
          <SearchOptionToggle
            pressed={regex}
            onPressedChange={setRegex}
            label="Use regular expression"
          >
            <Regex />
          </SearchOptionToggle>
        </div>
        <div className="flex items-center gap-1">
          <Input
            size="sm"
            placeholder="Replace"
            aria-label="Replace with"
            value={replaceText}
            onChange={(event) => setReplaceText(event.target.value)}
          />
          <Tooltip>
            <TooltipTrigger
              render={
                <Button
                  variant="ghost"
                  size="icon-xs"
                  aria-label="Replace all"
                  disabled={!replaceEnabled}
                  onClick={requestReplaceAll}
                >
                  <ReplaceAll />
                </Button>
              }
            />
            <TooltipPopup side="bottom">Replace all</TooltipPopup>
          </Tooltip>
          <SearchOptionToggle
            pressed={filtersOpen}
            onPressedChange={setFiltersOpen}
            label="Include and exclude files"
          >
            <SlidersHorizontal />
          </SearchOptionToggle>
        </div>
        {filtersOpen ? (
          <div className="space-y-1.5">
            <Input
              size="sm"
              placeholder="Files to include (e.g. src/**/*.ts)"
              aria-label="Files to include"
              value={includeGlob}
              onChange={(event) => setIncludeGlob(event.target.value)}
            />
            <Input
              size="sm"
              placeholder="Files to exclude"
              aria-label="Files to exclude"
              value={excludeGlob}
              onChange={(event) => setExcludeGlob(event.target.value)}
            />
          </div>
        ) : null}
        {hasQuery ? (
          <div className="flex items-center gap-1.5 px-0.5 text-[11px] leading-4 text-muted-foreground">
            {isSearching ? <Spinner className="size-3" /> : null}
            {search.error !== null && result === null ? (
              <span className="text-destructive">{search.error}</span>
            ) : result !== null ? (
              <span>
                {pluralize(matchCount, "result", "results")} in{" "}
                {pluralize(result.fileCount, "file", "files")}
              </span>
            ) : (
              <span>Searching…</span>
            )}
            {regex && searchInput !== null && clientPattern === null ? (
              <span className="text-warning-foreground">
                Pattern is not a valid JS regex; replace is disabled.
              </span>
            ) : null}
          </div>
        ) : null}
        {result?.truncated ? (
          <div className="px-0.5 text-[11px] leading-4 text-warning-foreground">
            Results were truncated. Narrow the search to see everything.
          </div>
        ) : null}
      </div>
      <ScrollArea className={cn("min-h-0 flex-1", replacing && "pointer-events-none opacity-64")}>
        {groups.length > 0 ? (
          <div className="space-y-0.5 p-2">
            {groups.map((group) => (
              <SearchResultGroup
                key={group.path}
                group={group}
                collapsed={collapsedPaths.has(group.path)}
                replaceEnabled={replaceEnabled}
                replacing={replacing}
                onToggleCollapsed={toggleCollapsed}
                onOpenMatch={onOpenFile}
                onReplaceInFile={requestReplaceInFile}
              />
            ))}
          </div>
        ) : (
          <div className="p-4 text-xs leading-relaxed text-muted-foreground">
            {!hasQuery
              ? "Search across files in this workspace."
              : isSearching
                ? null
                : search.error === null
                  ? "No results found."
                  : null}
          </div>
        )}
      </ScrollArea>
      <AlertDialog
        open={pendingReplace !== null}
        onOpenChange={(open) => {
          if (!open) setPendingReplace(null);
        }}
      >
        <AlertDialogPopup>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {pendingReplace?.scope === "file"
                ? `Replace all matches in ${pendingReplace.paths[0]}?`
                : "Replace all matches?"}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {pendingReplace === null
                ? null
                : `This replaces ${pluralize(pendingReplace.matchCount, "match", "matches")} across ${pluralize(pendingReplace.paths.length, "file", "files")}${replaceText.length === 0 ? " with an empty string" : ` with "${replaceText}"`}. Files that change on disk during the replace are skipped.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogClose render={<Button variant="outline" />}>Cancel</AlertDialogClose>
            <Button onClick={confirmPendingReplace}>Replace</Button>
          </AlertDialogFooter>
        </AlertDialogPopup>
      </AlertDialog>
    </div>
  );
}
