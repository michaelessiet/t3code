import { useCallback, useMemo } from "react";
import * as Schema from "effect/Schema";

import { useLocalStorage } from "~/hooks/useLocalStorage";

export const SYNTAX_HIGHLIGHTING_SETUP_STORAGE_KEY = "t3code:syntax-highlighting-setup:v1";

/**
 * Per-language syntax-highlighting choices, keyed by the language name from
 * the `@codemirror/language-data` registry (e.g. `"Go"`, `"Dockerfile"`).
 * `enabled` languages load their lazy grammar on open; `dismissed` languages
 * never prompt again.
 */
const SyntaxHighlightingSetupSchema = Schema.Struct({
  enabled: Schema.Array(Schema.String),
  dismissed: Schema.Array(Schema.String),
});

type SyntaxHighlightingSetup = typeof SyntaxHighlightingSetupSchema.Type;

const EMPTY_SETUP: SyntaxHighlightingSetup = { enabled: [], dismissed: [] };

export function useSyntaxHighlightingSetup() {
  const [setup, setSetup] = useLocalStorage(
    SYNTAX_HIGHLIGHTING_SETUP_STORAGE_KEY,
    EMPTY_SETUP,
    SyntaxHighlightingSetupSchema,
  );

  const enabledLanguages = useMemo(() => new Set(setup.enabled), [setup.enabled]);
  const dismissedLanguages = useMemo(() => new Set(setup.dismissed), [setup.dismissed]);

  const enableLanguage = useCallback(
    (languageName: string) => {
      setSetup((current) =>
        current.enabled.includes(languageName)
          ? current
          : {
              enabled: [...current.enabled, languageName],
              dismissed: current.dismissed.filter((name) => name !== languageName),
            },
      );
    },
    [setSetup],
  );

  const dismissLanguage = useCallback(
    (languageName: string) => {
      setSetup((current) =>
        current.dismissed.includes(languageName)
          ? current
          : {
              enabled: current.enabled,
              dismissed: [...current.dismissed, languageName],
            },
      );
    },
    [setSetup],
  );

  return { enabledLanguages, dismissedLanguages, enableLanguage, dismissLanguage };
}
