//! Link recognition is independent of terminal selection and VT wrapping.
use std::{
    path::{Component, Path, PathBuf},
    sync::LazyLock,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TerminalLink {
    Url(String),
    File {
        path: PathBuf,
        line: Option<u32>,
        column: Option<u32>,
    },
}

static LINKS: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r#"https?://[^\s<>\"']+|(?:\.?\.?/|/)?[^\s<>\"'`|()]+\.[a-zA-Z0-9_]+(?::\d+(?::\d+)?)?"#,
    )
    .expect("terminal link pattern")
});

pub(super) fn link_at(text: &str, byte: usize, cwd: &str) -> Option<TerminalLink> {
    LINKS.find_iter(text).find_map(|m| {
        let value = m.as_str().trim_end_matches([',', ';', '.', ')', ']']);
        (byte >= m.start() && byte < m.start() + value.len())
            .then(|| parse_link(value, cwd))
            .flatten()
    })
}

pub(super) fn parse_link(value: &str, cwd: &str) -> Option<TerminalLink> {
    if value.starts_with("http://") || value.starts_with("https://") {
        return Some(TerminalLink::Url(value.into()));
    }
    let value = if let Some(path) = value.strip_prefix("file://") {
        // Do not reinterpret a network file URL as a local path.
        let path = path.strip_prefix("localhost").unwrap_or(path);
        if !path.starts_with('/') {
            return None;
        }
        percent_encoding::percent_decode_str(path)
            .decode_utf8()
            .ok()?
            .into_owned()
    } else {
        if value.contains("://") {
            return None;
        }
        value.to_string()
    };
    let mut path = value.as_str();
    let mut positions = Vec::new();
    while let Some((head, tail)) = path.rsplit_once(':') {
        let Ok(n) = tail.parse::<u32>() else {
            break;
        };
        if n == 0 || positions.len() == 2 {
            return None;
        }
        positions.push(n);
        path = head;
    }
    if path.is_empty() || path.contains(':') {
        return None;
    }
    let (line, column) = match positions.as_slice() {
        [column, line] => (Some(*line), Some(*column)),
        [line] => (Some(*line), None),
        _ => (None, None),
    };
    let absolute = Path::new(cwd).join(path);
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            _ => normalized.push(component),
        }
    }
    Some(TerminalLink::File {
        path: normalized,
        line,
        column,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_keep_ports_queries_and_fragments_without_selection() {
        assert_eq!(
            link_at("→ http://localhost:5173/a?q=x#part.", 15, "/repo"),
            Some(TerminalLink::Url("http://localhost:5173/a?q=x#part".into()))
        );
        assert_eq!(link_at("x http://host/a", 0, "/repo"), None);
    }
    #[test]
    fn compiler_locations_and_parent_paths() {
        assert_eq!(
            link_at("error at ../src/main.rs:12:4", 19, "/repo/crate"),
            Some(TerminalLink::File {
                path: "/repo/src/main.rs".into(),
                line: Some(12),
                column: Some(4)
            })
        );
    }
    #[test]
    fn osc8_only_allows_safe_schemes_and_local_files() {
        assert_eq!(parse_link("javascript:alert(1)", "/repo"), None);
        assert_eq!(parse_link("ssh://host/a", "/repo"), None);
        assert_eq!(parse_link("file://remote/a.rs", "/repo"), None);
        assert_eq!(
            parse_link("file:///repo/my%20file.rs", "/repo"),
            Some(TerminalLink::File {
                path: "/repo/my file.rs".into(),
                line: None,
                column: None
            })
        );
    }
}
