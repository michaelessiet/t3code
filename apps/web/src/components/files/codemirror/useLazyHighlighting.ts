import type { Compartment } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { useEffect, useMemo } from "react";

import { stackedThreadToast, toastManager } from "~/components/ui/toast";

import { useSyntaxHighlightingSetup } from "./highlightingSetup";
import { lazyLanguageDescriptionForPath } from "./languages";

/** Languages already prompted this session, so reopening files doesn't re-toast. */
const promptedLanguages = new Set<string>();

/**
 * Reactive syntax highlighting for files whose language isn't bundled: when
 * the open file matches a lazily-loadable grammar from
 * `@codemirror/language-data`, either loads it into the editor's language
 * compartment (if the user enabled that language before) or prompts once per
 * session asking whether to set it up. Enabling from the prompt applies to
 * the already-open editor immediately and persists for future files.
 */
export function useLazyHighlighting(
  view: EditorView | null,
  languageCompartment: Compartment | null,
  relativePath: string,
): void {
  const { enabledLanguages, dismissedLanguages, enableLanguage, dismissLanguage } =
    useSyntaxHighlightingSetup();

  const description = useMemo(() => lazyLanguageDescriptionForPath(relativePath), [relativePath]);
  const languageName = description?.name ?? null;
  const enabled = languageName !== null && enabledLanguages.has(languageName);
  const dismissed = languageName !== null && dismissedLanguages.has(languageName);

  useEffect(() => {
    if (view === null || languageCompartment === null || description === null || !enabled) return;
    let cancelled = false;
    description
      .load()
      .then((support) => {
        if (cancelled) return;
        view.dispatch({ effects: languageCompartment.reconfigure(support) });
      })
      .catch((error: unknown) => {
        console.error(`Could not load ${description.name} syntax highlighting.`, error);
      });
    return () => {
      cancelled = true;
    };
  }, [view, languageCompartment, description, enabled]);

  useEffect(() => {
    if (languageName === null || enabled || dismissed) return;
    if (promptedLanguages.has(languageName)) return;
    promptedLanguages.add(languageName);
    let toastId: ReturnType<typeof toastManager.add> | undefined;
    toastId = toastManager.add(
      stackedThreadToast({
        type: "info",
        title: `Set up ${languageName} syntax highlighting?`,
        description: `Highlighting for ${languageName} files isn't set up yet.`,
        timeout: 15000,
        actionProps: {
          children: "Set up",
          onClick: () => {
            enableLanguage(languageName);
            if (toastId !== undefined) toastManager.close(toastId);
          },
        },
        data: {
          secondaryActionProps: {
            children: "Don't ask again",
            onClick: () => {
              dismissLanguage(languageName);
              if (toastId !== undefined) toastManager.close(toastId);
            },
          },
        },
      }),
    );
  }, [languageName, enabled, dismissed, enableLanguage, dismissLanguage]);
}
