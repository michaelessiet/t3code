//! LSP gating logic: when completion may fire and which files LSP attaches to.
//!
//! Ports of two pure predicates from the Electron app:
//! - `completionAnchor` / `COMPLETION_TRIGGER_CHARS` in
//!   `apps/web/src/components/files/codemirror/lspBridge.ts`
//! - `isLspSupportedPath` in `packages/shared/src/lspSupport.ts`, plus the
//!   server-reported-extensions override in
//!   `apps/web/src/components/files/codemirror/useLspBridge.ts`
//!
//! Both must keep the exact JS semantics (including the quirks documented on
//! each function) so the two apps gate identically against the same server.

/// Characters that should open (or re-query) completion even though no word
/// precedes the cursor: member access, import-path strings, decorators, JSX.
/// Mirrors `COMPLETION_TRIGGER_CHARS` (lspBridge.ts).
const COMPLETION_TRIGGER_CHARS: [char; 6] = ['.', '"', '\'', '/', '@', '<'];

/// The JS regex `/[\w$]+$/` character class: ASCII word chars plus `$`.
fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

/// Byte offset within `line_before_cursor` where the completion query starts,
/// or `None` when completion should not fire. Mirrors the Electron
/// `completionAnchor` (lspBridge.ts): a trailing `[A-Za-z0-9_$]+` word anchors
/// at the word start; otherwise fire only on explicit invoke or when the text
/// ends with a member/trigger character, anchored at the cursor (i.e. at
/// `line_before_cursor.len()`).
///
/// The trailing-word scan is byte-wise: the word class is pure ASCII, and
/// UTF-8 continuation bytes never fall in it, so the anchor always lands on a
/// char boundary. A multi-byte final char is not a trigger char (the JS
/// `.at(-1)` check likewise never matches one of the ASCII triggers there).
pub fn completion_anchor(line_before_cursor: &str, explicit: bool) -> Option<usize> {
    let bytes = line_before_cursor.as_bytes();
    let word_start = bytes
        .iter()
        .rposition(|&b| !is_word_byte(b))
        .map_or(0, |i| i + 1);
    if word_start < bytes.len() {
        return Some(word_start);
    }
    let last_char = line_before_cursor.chars().next_back();
    if explicit || last_char.is_some_and(|c| COMPLETION_TRIGGER_CHARS.contains(&c)) {
        return Some(line_before_cursor.len());
    }
    None
}

/// Whether completion should run at all for this context — the boolean view
/// of [`completion_anchor`].
pub fn completion_should_trigger(line_before_cursor: &str, explicit: bool) -> bool {
    completion_anchor(line_before_cursor, explicit).is_some()
}

/// Extensions the server's *built-in* language servers handle. Port of
/// `LSP_SUPPORTED_EXTENSIONS` (packages/shared/src/lspSupport.ts), which is
/// asserted against the server registry by a server-side test. Used only as
/// the pre-load fallback: once `lsp.serverStatus` reports
/// `supportedExtensions`, that list wins (user-defined servers add to it).
pub const LSP_SUPPORTED_EXTENSIONS: [&str; 12] = [
    ".ts", ".mts", ".cts", ".tsx", ".js", ".mjs", ".cjs", ".jsx", ".rs", ".py", ".pyi", ".go",
];

/// Whether LSP should attach to this file. `supported_extensions` is the
/// server-reported list (leading-dot, lowercase) once loaded; `None` falls
/// back to the static built-in list (the Electron cold-load behavior in
/// useLspBridge.ts / lspSupport.ts).
///
/// Extension extraction mirrors the JS exactly: `lastIndexOf('.')` over the
/// WHOLE relative path (not just the file name), then lowercase. So a dotfile
/// yields itself (".gitignore" → ".gitignore"), a path with no '.' anywhere is
/// unsupported, and a dot in a directory name leaks into the "extension"
/// ("dir.v2/file" → ".v2/file") — a faithful quirk, not a feature.
pub fn is_lsp_supported_path(relative_path: &str, supported_extensions: Option<&[String]>) -> bool {
    let Some(dot_index) = relative_path.rfind('.') else {
        return false;
    };
    let ext = relative_path[dot_index..].to_lowercase();
    match supported_extensions {
        Some(list) => list.iter().any(|entry| entry == &ext),
        None => LSP_SUPPORTED_EXTENSIONS.contains(&ext.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(extensions: &[&str]) -> Vec<String> {
        extensions.iter().map(|e| (*e).to_string()).collect()
    }

    // --- completion_anchor: trailing word -----------------------------------

    #[test]
    fn anchor_trailing_word_starts_at_word() {
        // "foo.ba" → word "ba" → anchor two bytes before the end.
        assert_eq!(completion_anchor("foo.ba", false), Some(4));
        assert_eq!(completion_anchor("foo.ba", true), Some(4));
    }

    #[test]
    fn anchor_whole_line_word() {
        assert_eq!(completion_anchor("foobar", false), Some(0));
    }

    #[test]
    fn anchor_word_after_space() {
        assert_eq!(completion_anchor("let x", false), Some(4));
    }

    #[test]
    fn anchor_dollar_and_underscore_are_word_chars() {
        // JS class is [\w$]: "$var" and "_x1" are single words.
        assert_eq!(completion_anchor("$var", false), Some(0));
        assert_eq!(completion_anchor("_x1", false), Some(0));
        assert_eq!(completion_anchor("a.$f", false), Some(2));
    }

    #[test]
    fn anchor_word_scan_is_ascii_only() {
        // 'é' is not in [\w$]; only the trailing ASCII run anchors.
        // "héllo" = h(1 byte) é(2 bytes) llo → trailing word "llo" at byte 3.
        assert_eq!(completion_anchor("héllo", false), Some(3));
    }

    // --- completion_anchor: trigger chars ------------------------------------

    #[test]
    fn anchor_member_dot_fires_at_cursor() {
        assert_eq!(completion_anchor("foo.", false), Some(4));
    }

    #[test]
    fn anchor_all_trigger_chars_fire_at_cursor() {
        for trigger in ['.', '"', '\'', '/', '@', '<'] {
            let line = format!("x {trigger}");
            assert_eq!(
                completion_anchor(&line, false),
                Some(line.len()),
                "trigger {trigger:?}"
            );
        }
    }

    #[test]
    fn anchor_trigger_as_only_char() {
        assert_eq!(completion_anchor("@", false), Some(1));
        assert_eq!(completion_anchor("<", false), Some(1));
    }

    #[test]
    fn anchor_import_string_quote_and_slash() {
        assert_eq!(completion_anchor("import '", false), Some(8));
        assert_eq!(completion_anchor("from \"./a/", false), Some(10));
    }

    // --- completion_anchor: no word, no trigger ------------------------------

    #[test]
    fn anchor_plain_space_needs_explicit() {
        assert_eq!(completion_anchor("foo ", false), None);
        assert_eq!(completion_anchor("foo ", true), Some(4));
    }

    #[test]
    fn anchor_empty_string() {
        assert_eq!(completion_anchor("", false), None);
        assert_eq!(completion_anchor("", true), Some(0));
    }

    #[test]
    fn anchor_multibyte_last_char_is_not_a_trigger() {
        // 'é' (2 bytes) ends the line: not a word char, not a trigger.
        assert_eq!(completion_anchor("é", false), None);
        // Explicit still fires, anchored past the multi-byte char.
        assert_eq!(completion_anchor("é", true), Some(2));
        // Astral char (4 bytes): same, and the anchor stays a char boundary.
        assert_eq!(completion_anchor("foo.😀", false), None);
        assert_eq!(completion_anchor("foo.😀", true), Some(8));
    }

    #[test]
    fn anchor_non_trigger_punctuation() {
        assert_eq!(completion_anchor("a +", false), None);
        assert_eq!(completion_anchor("f(", false), None);
    }

    #[test]
    fn should_trigger_matches_anchor() {
        assert!(completion_should_trigger("foo.", false));
        assert!(completion_should_trigger("foo ", true));
        assert!(!completion_should_trigger("foo ", false));
        assert!(!completion_should_trigger("", false));
    }

    // --- is_lsp_supported_path: server list ----------------------------------

    #[test]
    fn support_server_list_hit_and_miss() {
        let list = owned(&[".rs", ".toml"]);
        assert!(is_lsp_supported_path("src/main.rs", Some(&list)));
        assert!(is_lsp_supported_path("Cargo.toml", Some(&list)));
        // The server list REPLACES the static set; built-ins absent from it
        // are unsupported.
        assert!(!is_lsp_supported_path("src/app.ts", Some(&list)));
    }

    #[test]
    fn support_empty_server_list_rejects_everything() {
        let list: Vec<String> = Vec::new();
        assert!(!is_lsp_supported_path("src/main.rs", Some(&list)));
    }

    #[test]
    fn support_extension_is_lowercased() {
        let list = owned(&[".tsx"]);
        assert!(is_lsp_supported_path("Component.TSX", Some(&list)));
        assert!(is_lsp_supported_path("Component.TSX", None));
    }

    #[test]
    fn support_no_dot_path_is_unsupported() {
        assert!(!is_lsp_supported_path("Makefile", None));
        let list = owned(&[".rs"]);
        assert!(!is_lsp_supported_path("Makefile", Some(&list)));
    }

    #[test]
    fn support_dotfile_extension_is_the_whole_name() {
        // lastIndexOf('.') = 0 → ".gitignore" is its own "extension".
        assert!(!is_lsp_supported_path(".gitignore", None));
        let list = owned(&[".gitignore"]);
        assert!(is_lsp_supported_path(".gitignore", Some(&list)));
    }

    #[test]
    fn support_none_falls_back_to_static_list() {
        for ext in LSP_SUPPORTED_EXTENSIONS {
            let path = format!("dir/file{ext}");
            assert!(is_lsp_supported_path(&path, None), "static ext {ext}");
        }
        assert!(!is_lsp_supported_path("notes.md", None));
        assert!(!is_lsp_supported_path("style.css", None));
    }

    #[test]
    fn support_last_dot_wins_for_compound_extensions() {
        // "types.d.ts" → ".ts", not ".d.ts".
        assert!(is_lsp_supported_path("types.d.ts", None));
    }

    #[test]
    fn support_whole_path_last_index_of_quirk() {
        // JS takes lastIndexOf('.') over the WHOLE path, so a dot in a
        // directory name leaks into the extension: "dir.v2/file" → ".v2/file".
        assert!(!is_lsp_supported_path("dir.v2/file", None));
        let quirk = owned(&[".v2/file"]);
        assert!(is_lsp_supported_path("dir.v2/file", Some(&quirk)));
        // Even a real built-in extension elsewhere in the list doesn't rescue
        // an extensionless file inside a dotted directory.
        let list = owned(&[".ts"]);
        assert!(!is_lsp_supported_path("dir.v2/file", Some(&list)));
    }
}
