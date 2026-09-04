//! Project scripts ("actions"): id allocation, primary selection, list edits,
//! t3.json import parsing.
//!
//! Ports `apps/web/src/projectScripts.ts`, the script-list edit rules from
//! `ChatView.tsx` (`saveProjectScript`/`updateProjectScript`), the importable
//! filter from `ProjectScriptsControl.tsx`, and the `T3ProjectFile` decode
//! from `packages/contracts/src/t3ProjectFile.ts` +
//! `hooks/useT3ProjectFileScripts.ts` (all-or-nothing: any invalid field
//! rejects the whole file, exactly like Electron's `Schema.decodeExit`).
//!
//! Not ported (app-layer or deferred): script keybindings
//! (`lib/projectScriptKeybindings.ts` — Vitre has no dynamic keymap yet) and
//! the preview-URL open-on-run behavior (preview subsystem is M4).

use std::collections::HashSet;

use serde_json::Value;

use vitre_contracts::{ProjectScript, ProjectScriptIcon, TrimmedNonEmptyString};

/// `MAX_SCRIPT_ID_LENGTH` (contracts/keybindings.ts).
pub const MAX_SCRIPT_ID_LENGTH: usize = 24;

/// `T3_PROJECT_FILE_NAME`.
pub const T3_PROJECT_FILE_NAME: &str = "t3.json";

const T3_PROJECT_FILE_PATH_MAX_LENGTH: usize = 512;
const T3_PROJECT_FILE_MAX_SCRIPTS: usize = 50;

fn tnes(value: &str) -> TrimmedNonEmptyString {
    TrimmedNonEmptyString(value.to_string())
}

/// `ProjectScriptInput` — the dialog's payload shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectScriptInput {
    pub name: String,
    pub command: String,
    pub icon: ProjectScriptIcon,
    pub run_on_worktree_create: bool,
    /// `None` mirrors Electron's `previewUrl: null` (drops the field and
    /// `autoOpenPreview` from the built script).
    pub preview_url: Option<String>,
    pub auto_open_preview: bool,
}

/// `buildProjectScript`: `previewUrl === null` omits both preview fields.
pub fn build_project_script(id: &str, input: &ProjectScriptInput) -> ProjectScript {
    let (preview_url, auto_open_preview) = match &input.preview_url {
        None => (None, None),
        Some(url) => (Some(Some(tnes(url))), Some(Some(input.auto_open_preview))),
    };
    ProjectScript {
        auto_open_preview,
        command: tnes(&input.command),
        icon: input.icon.clone(),
        id: tnes(id),
        name: tnes(&input.name),
        preview_url,
        run_on_worktree_create: input.run_on_worktree_create,
    }
}

/// `normalizeScriptId`: trim → lowercase → non-alphanumeric runs to `-` →
/// strip edge dashes → `"script"` fallback → length cap (re-stripping a
/// trailing dash the cut may expose).
fn normalize_script_id(value: &str) -> String {
    let lowered = value.trim().to_lowercase();
    let mut cleaned = String::new();
    let mut pending_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            if pending_dash && !cleaned.is_empty() {
                cleaned.push('-');
            }
            pending_dash = false;
            cleaned.push(ch);
        } else {
            pending_dash = true;
        }
    }
    if cleaned.is_empty() {
        return "script".to_string();
    }
    if cleaned.len() <= MAX_SCRIPT_ID_LENGTH {
        return cleaned;
    }
    let cut = cleaned[..MAX_SCRIPT_ID_LENGTH]
        .trim_end_matches('-')
        .to_string();
    if cut.is_empty() {
        "script".to_string()
    } else {
        cut
    }
}

/// `nextProjectScriptId`: the normalized name, then `-2`, `-3`, … suffixes
/// (length-capped by truncating the base). Electron's post-10,000 last resort
/// appends `Date.now()`; Vitre uses the same wall-clock stamp.
pub fn next_project_script_id<'a>(
    name: &str,
    existing_ids: impl IntoIterator<Item = &'a str>,
) -> String {
    let taken: HashSet<&str> = existing_ids.into_iter().collect();
    let base_id = normalize_script_id(name);
    if !taken.contains(base_id.as_str()) {
        return base_id;
    }
    let mut suffix: u32 = 2;
    while suffix < 10_000 {
        let candidate = format!("{base_id}-{suffix}");
        let safe_candidate = if candidate.len() <= MAX_SCRIPT_ID_LENGTH {
            candidate
        } else {
            let suffix_len = suffix.to_string().len();
            let keep = MAX_SCRIPT_ID_LENGTH.saturating_sub(suffix_len + 1).max(1);
            format!("{}-{suffix}", &base_id[..keep.min(base_id.len())])
        };
        if !taken.contains(safe_candidate.as_str()) {
            return safe_candidate;
        }
        suffix += 1;
    }
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    let mut fallback = format!("{base_id}-{millis}");
    fallback.truncate(MAX_SCRIPT_ID_LENGTH);
    fallback
}

/// `primaryProjectScript`: the first non-setup script, else the first script.
pub fn primary_project_script(scripts: &[ProjectScript]) -> Option<&ProjectScript> {
    scripts
        .iter()
        .find(|script| !script.run_on_worktree_create)
        .or_else(|| scripts.first())
}

/// The header button's script: the remembered last-invoked id when it still
/// exists, else [`primary_project_script`].
pub fn preferred_project_script<'a>(
    scripts: &'a [ProjectScript],
    preferred_script_id: Option<&str>,
) -> Option<&'a ProjectScript> {
    if let Some(id) = preferred_script_id
        && let Some(preferred) = scripts.iter().find(|script| script.id.0 == id)
    {
        return Some(preferred);
    }
    primary_project_script(scripts)
}

/// `saveProjectScript`'s list edit: append, clearing every other script's
/// setup flag when the new one claims it (one setup script at a time).
pub fn scripts_after_add(
    existing: &[ProjectScript],
    new_script: ProjectScript,
) -> Vec<ProjectScript> {
    let mut next: Vec<ProjectScript> = existing
        .iter()
        .map(|script| {
            if new_script.run_on_worktree_create && script.run_on_worktree_create {
                let mut cleared = script.clone();
                cleared.run_on_worktree_create = false;
                cleared
            } else {
                script.clone()
            }
        })
        .collect();
    next.push(new_script);
    next
}

/// `updateProjectScript`'s list edit: replace by id; when the update claims
/// the setup flag, clear it everywhere else.
pub fn scripts_after_update(
    existing: &[ProjectScript],
    script_id: &str,
    updated: ProjectScript,
) -> Vec<ProjectScript> {
    existing
        .iter()
        .map(|script| {
            if script.id.0 == script_id {
                updated.clone()
            } else if updated.run_on_worktree_create && script.run_on_worktree_create {
                let mut cleared = script.clone();
                cleared.run_on_worktree_create = false;
                cleared
            } else {
                script.clone()
            }
        })
        .collect()
}

/// `deleteProjectScript`'s list edit.
pub fn scripts_after_delete(existing: &[ProjectScript], script_id: &str) -> Vec<ProjectScript> {
    existing
        .iter()
        .filter(|script| script.id.0 != script_id)
        .cloned()
        .collect()
}

/// A script from the checked-in `t3.json` (`T3ProjectFileScript`), decoded
/// with the contract's optionality preserved — defaults (`icon` → play,
/// flags → false) apply at import time, like Electron's `importFileScript`.
#[derive(Debug, Clone, PartialEq)]
pub struct T3FileScript {
    pub name: String,
    pub command: String,
    pub icon: Option<ProjectScriptIcon>,
    pub run_on_worktree_create: Option<bool>,
    pub preview_url: Option<String>,
    pub auto_open_preview: Option<bool>,
}

/// Required-string rule from the schema's `trimmedNonEmpty`: the raw value
/// must be a non-empty string AND stay non-empty after trimming (the decode
/// re-validates the trimmed value). Returns the trimmed string.
fn decode_trimmed_non_empty(value: &Value) -> Option<String> {
    let raw = value.as_str()?;
    if raw.is_empty() {
        return None;
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn decode_icon(value: &Value) -> Option<ProjectScriptIcon> {
    // Electron's literal union rejects unknown icons (failing the whole
    // file), so the forward-compatible `Unknown` variant is NOT accepted.
    match value.as_str()? {
        "play" => Some(ProjectScriptIcon::Play),
        "test" => Some(ProjectScriptIcon::Test),
        "lint" => Some(ProjectScriptIcon::Lint),
        "configure" => Some(ProjectScriptIcon::Configure),
        "build" => Some(ProjectScriptIcon::Build),
        "debug" => Some(ProjectScriptIcon::Debug),
        _ => None,
    }
}

fn decode_file_script(value: &Value) -> Option<T3FileScript> {
    let object = value.as_object()?;
    let name = decode_trimmed_non_empty(object.get("name")?)?;
    let command = decode_trimmed_non_empty(object.get("command")?)?;
    let icon = match object.get("icon") {
        None => None,
        Some(value) => Some(decode_icon(value)?),
    };
    let run_on_worktree_create = match object.get("runOnWorktreeCreate") {
        None => None,
        Some(value) => Some(value.as_bool()?),
    };
    let preview_url = match object.get("previewUrl") {
        None => None,
        Some(value) => Some(decode_trimmed_non_empty(value)?),
    };
    let auto_open_preview = match object.get("autoOpenPreview") {
        None => None,
        Some(value) => Some(value.as_bool()?),
    };
    Some(T3FileScript {
        name,
        command,
        icon,
        run_on_worktree_create,
        preview_url,
        auto_open_preview,
    })
}

/// `useT3ProjectFileScripts`: decode a `t3.json` body into its scripts.
/// Missing or invalid files (any schema violation, including an unknown icon
/// or an over-limit `iconPath`) resolve to the empty list — never a partial
/// one. Extra keys are ignored, matching Effect's default struct decode.
pub fn parse_t3_project_file_scripts(contents: &str) -> Vec<T3FileScript> {
    let Ok(value) = serde_json::from_str::<Value>(contents) else {
        return Vec::new();
    };
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    if let Some(schema) = object.get("$schema")
        && !schema.is_string()
    {
        return Vec::new();
    }
    if let Some(icon_path) = object.get("iconPath") {
        let Some(decoded) = decode_trimmed_non_empty(icon_path) else {
            return Vec::new();
        };
        // The max-length check runs on the raw (pre-trim) string.
        let raw_len = icon_path.as_str().map(str::len).unwrap_or(usize::MAX);
        if raw_len > T3_PROJECT_FILE_PATH_MAX_LENGTH {
            return Vec::new();
        }
        let _ = decoded;
    }
    let Some(scripts) = object.get("scripts") else {
        return Vec::new();
    };
    let Some(entries) = scripts.as_array() else {
        return Vec::new();
    };
    if entries.len() > T3_PROJECT_FILE_MAX_SCRIPTS {
        return Vec::new();
    }
    let mut decoded = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(script) = decode_file_script(entry) else {
            return Vec::new();
        };
        decoded.push(script);
    }
    decoded
}

/// `importableScripts`: file scripts not already present, matched by exact
/// command or case-insensitive name.
pub fn importable_file_scripts<'a>(
    file_scripts: &'a [T3FileScript],
    scripts: &[ProjectScript],
) -> Vec<&'a T3FileScript> {
    file_scripts
        .iter()
        .filter(|file_script| {
            !scripts.iter().any(|script| {
                script.command.0 == file_script.command
                    || script.name.0.to_lowercase() == file_script.name.to_lowercase()
            })
        })
        .collect()
}

/// `importFileScript`'s payload mapping: defaults applied, `autoOpenPreview`
/// only honored alongside a `previewUrl`.
pub fn import_input_for_file_script(file_script: &T3FileScript) -> ProjectScriptInput {
    ProjectScriptInput {
        name: file_script.name.clone(),
        command: file_script.command.clone(),
        icon: file_script.icon.clone().unwrap_or(ProjectScriptIcon::Play),
        run_on_worktree_create: file_script.run_on_worktree_create.unwrap_or(false),
        preview_url: file_script.preview_url.clone(),
        auto_open_preview: if file_script.preview_url.is_some() {
            file_script.auto_open_preview.unwrap_or(false)
        } else {
            false
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(id: &str, name: &str, command: &str, setup: bool) -> ProjectScript {
        ProjectScript {
            auto_open_preview: None,
            command: tnes(command),
            icon: ProjectScriptIcon::Play,
            id: tnes(id),
            name: tnes(name),
            preview_url: None,
            run_on_worktree_create: setup,
        }
    }

    #[test]
    fn normalizes_script_ids() {
        assert_eq!(normalize_script_id("  Run Tests!  "), "run-tests");
        assert_eq!(normalize_script_id("---"), "script");
        assert_eq!(normalize_script_id(""), "script");
        assert_eq!(
            normalize_script_id("a very long script name that overflows"),
            "a-very-long-script-name"
        );
    }

    #[test]
    fn allocates_suffixed_ids() {
        assert_eq!(next_project_script_id("Test", []), "test");
        assert_eq!(next_project_script_id("Test", ["test"]), "test-2");
        assert_eq!(next_project_script_id("Test", ["test", "test-2"]), "test-3");
        // Length-capped candidates truncate the base, not the suffix.
        let long = "a-very-long-script-name";
        let capped = next_project_script_id("a very long script name that overflows", [long]);
        assert_eq!(capped, "a-very-long-script-nam-2");
        assert!(capped.len() <= MAX_SCRIPT_ID_LENGTH);
    }

    #[test]
    fn primary_prefers_non_setup_scripts() {
        let scripts = vec![script("setup", "Setup", "make setup", true)];
        assert_eq!(primary_project_script(&scripts).unwrap().id.0, "setup");
        let scripts = vec![
            script("setup", "Setup", "make setup", true),
            script("dev", "Dev", "make dev", false),
        ];
        assert_eq!(primary_project_script(&scripts).unwrap().id.0, "dev");
        assert!(primary_project_script(&[]).is_none());
    }

    #[test]
    fn preferred_falls_back_when_the_id_is_gone() {
        let scripts = vec![
            script("dev", "Dev", "make dev", false),
            script("test", "Test", "make test", false),
        ];
        assert_eq!(
            preferred_project_script(&scripts, Some("test"))
                .unwrap()
                .id
                .0,
            "test"
        );
        assert_eq!(
            preferred_project_script(&scripts, Some("gone"))
                .unwrap()
                .id
                .0,
            "dev"
        );
    }

    #[test]
    fn add_and_update_keep_a_single_setup_script() {
        let existing = vec![
            script("old-setup", "Old", "make old", true),
            script("dev", "Dev", "make dev", false),
        ];
        let added = scripts_after_add(&existing, script("new-setup", "New", "make new", true));
        assert!(!added[0].run_on_worktree_create);
        assert!(added[2].run_on_worktree_create);

        let updated =
            scripts_after_update(&existing, "dev", script("dev", "Dev", "make dev", true));
        assert!(!updated[0].run_on_worktree_create);
        assert!(updated[1].run_on_worktree_create);
    }

    #[test]
    fn build_omits_preview_fields_without_a_url() {
        let input = ProjectScriptInput {
            name: "Dev".into(),
            command: "make dev".into(),
            icon: ProjectScriptIcon::Play,
            run_on_worktree_create: false,
            preview_url: None,
            auto_open_preview: true,
        };
        let built = build_project_script("dev", &input);
        assert_eq!(built.preview_url, None);
        assert_eq!(built.auto_open_preview, None);

        let with_url = ProjectScriptInput {
            preview_url: Some("http://localhost:5173".into()),
            ..input
        };
        let built = build_project_script("dev", &with_url);
        assert_eq!(built.preview_url, Some(Some(tnes("http://localhost:5173"))));
        assert_eq!(built.auto_open_preview, Some(Some(true)));
    }

    #[test]
    fn parses_a_valid_t3_project_file() {
        let scripts = parse_t3_project_file_scripts(
            r#"{
                "$schema": "https://t3.codes/schema/t3.json",
                "scripts": [
                    { "name": " Dev ", "command": "pnpm dev", "icon": "build" },
                    { "name": "Test", "command": "pnpm test", "runOnWorktreeCreate": true,
                      "previewUrl": "http://localhost:3000", "autoOpenPreview": true }
                ]
            }"#,
        );
        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts[0].name, "Dev");
        assert_eq!(scripts[0].icon, Some(ProjectScriptIcon::Build));
        assert_eq!(
            scripts[1].preview_url.as_deref(),
            Some("http://localhost:3000")
        );
    }

    #[test]
    fn any_invalid_script_rejects_the_whole_file() {
        // Whitespace-only command.
        assert!(parse_t3_project_file_scripts(
            r#"{ "scripts": [ { "name": "A", "command": "ok" }, { "name": "B", "command": "  " } ] }"#,
        )
        .is_empty());
        // Unknown icon literal.
        assert!(
            parse_t3_project_file_scripts(
                r#"{ "scripts": [ { "name": "A", "command": "ok", "icon": "rocket" } ] }"#,
            )
            .is_empty()
        );
        // Not JSON at all / no scripts key.
        assert!(parse_t3_project_file_scripts("not json").is_empty());
        assert!(parse_t3_project_file_scripts("{}").is_empty());
    }

    #[test]
    fn importable_filters_by_command_and_case_insensitive_name() {
        let file_scripts = vec![
            T3FileScript {
                name: "DEV".into(),
                command: "pnpm dev2".into(),
                icon: None,
                run_on_worktree_create: None,
                preview_url: None,
                auto_open_preview: None,
            },
            T3FileScript {
                name: "Lint".into(),
                command: "pnpm lint".into(),
                icon: None,
                run_on_worktree_create: None,
                preview_url: None,
                auto_open_preview: None,
            },
        ];
        let scripts = vec![script("dev", "Dev", "pnpm dev", false)];
        let importable = importable_file_scripts(&file_scripts, &scripts);
        // "DEV" collides with "Dev" by name despite the different command.
        assert_eq!(importable.len(), 1);
        assert_eq!(importable[0].name, "Lint");
    }

    #[test]
    fn import_input_applies_defaults() {
        let file_script = T3FileScript {
            name: "Dev".into(),
            command: "pnpm dev".into(),
            icon: None,
            run_on_worktree_create: None,
            preview_url: None,
            auto_open_preview: Some(true),
        };
        let input = import_input_for_file_script(&file_script);
        assert_eq!(input.icon, ProjectScriptIcon::Play);
        assert!(!input.run_on_worktree_create);
        // autoOpenPreview is dropped without a previewUrl.
        assert!(!input.auto_open_preview);
    }
}
