//! Search/replace semantics shared by the native search panel and its preview.
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// The search protocol reports JavaScript UTF-16 offsets; GPUI expects UTF-8.
pub fn match_range(text: &str, start: usize, end: usize) -> std::ops::Range<usize> {
    fn byte_at(text: &str, target: usize) -> usize {
        let mut units = 0;
        for (byte, ch) in text.char_indices() {
            if units >= target {
                return byte;
            }
            units += ch.len_utf16();
        }
        text.len()
    }
    let start = byte_at(text, start);
    let end = byte_at(text, end).max(start);
    start..end
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchPattern {
    pub query: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
}

impl SearchPattern {
    pub fn compile(&self) -> Result<Regex, String> {
        if self.query.is_empty() {
            return Err("Enter text to search for.".into());
        }
        let source = if self.regex {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        };
        let source = if self.whole_word {
            format!(r"\b(?:{source})\b")
        } else {
            source
        };
        RegexBuilder::new(&source)
            .case_insensitive(
                !self.case_sensitive && !self.query.chars().any(|c| c.is_ascii_uppercase()),
            )
            .multi_line(true)
            .build()
            .map_err(|e| e.to_string())
    }

    /// Literal replacement never expands dollars. Regex replacement supports
    /// capture groups ($1 / ${name}) and the JS whole-match alias $&.
    pub fn replace(&self, contents: &str, replacement: &str) -> Result<(String, usize), String> {
        let regex = self.compile()?;
        let count = regex.find_iter(contents).count();
        let next = if self.regex {
            // Leave escaped dollar tokens intact: `$$&` means a literal `$&`.
            let mut converted = String::with_capacity(replacement.len());
            let mut chars = replacement.chars().peekable();
            while let Some(ch) = chars.next() {
                converted.push(ch);
                if ch == '$' {
                    match chars.peek() {
                        Some('$') => {
                            converted.push(chars.next().unwrap());
                        }
                        Some('&') => {
                            chars.next();
                            converted.push('0');
                        }
                        _ => {}
                    }
                }
            }
            regex.replace_all(contents, converted.as_str())
        } else {
            regex.replace_all(contents, regex::NoExpand(replacement))
        };
        Ok((next.into_owned(), count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_match_offsets_are_utf16_not_bytes() {
        assert_eq!(&"🦀 café code"[match_range("🦀 café code", 3, 7)], "café");
        assert_eq!(match_range("café", 200, 300), 5..5);
    }
    #[test]
    fn replacement_preserves_unicode_crlf_and_literal_dollars() {
        let p = SearchPattern {
            query: "café".into(),
            ..Default::default()
        };
        assert_eq!(
            p.replace("Café\r\ncafé", "$1").unwrap(),
            ("$1\r\n$1".into(), 2)
        );
    }
    #[test]
    fn smart_case_whole_words_and_groups() {
        let p = SearchPattern {
            query: "Item".into(),
            whole_word: true,
            ..Default::default()
        };
        assert_eq!(
            p.replace("Item item Items", "X").unwrap(),
            ("X item Items".into(), 1)
        );
        let p = SearchPattern {
            query: "(foo)(bar)".into(),
            regex: true,
            ..Default::default()
        };
        assert_eq!(p.replace("foobar", "$2/$1/$&").unwrap().0, "bar/foo/foobar");
        assert_eq!(p.replace("foobar", "$$&/$$1/$&").unwrap().0, "$&/$1/foobar");
        assert!(
            SearchPattern {
                query: "[".into(),
                regex: true,
                ..Default::default()
            }
            .compile()
            .is_err()
        );
    }
    #[test]
    fn line_anchors_and_named_captures_are_preserved() {
        let p = SearchPattern {
            query: "^(?<name>foo)$".into(),
            regex: true,
            ..Default::default()
        };
        assert_eq!(
            p.replace("foo\nfoo\nfood", "${name}!").unwrap(),
            ("foo!\nfoo!\nfood".into(), 2)
        );
        assert_eq!(p.replace("bar", "").unwrap(), ("bar".into(), 0));
    }
}
