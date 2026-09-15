//! Normalize server-specific legends and UTF-16 columns into the editor's
//! stable theme vocabulary and Unicode scalar columns.
use super::*;
use gpui_component::input::DocumentRangeSemanticTokensProvider;
use lsp_types::{SemanticToken, SemanticTokenType, SemanticTokens, SemanticTokensLegend};
use std::ops::Range;
use vitre_contracts::methods::LspSemanticTokens;
use vitre_contracts::{LspCodeActionsInput, LspSemanticToken};

const TYPES: &[&str] = &[
    "type",
    "function",
    "variable",
    "property",
    "constant",
    "keyword",
    "comment",
    "string",
    "number",
    "operator",
    "attribute",
];

fn category(token: &LspSemanticToken) -> Option<u32> {
    let name = match token.kind.0.as_str() {
        "namespace" | "type" | "class" | "enum" | "interface" | "struct" | "typeParameter" => {
            "type"
        }
        "function" | "method" | "macro" => "function",
        "enumMember" => "constant",
        "variable" | "parameter" if token.modifiers.iter().any(|m| m.0 == "readonly") => "constant",
        "variable" | "parameter" => "variable",
        "property" | "event" => "property",
        "modifier" | "keyword" => "keyword",
        "regexp" | "string" => "string",
        "decorator" => "attribute",
        name => name,
    };
    TYPES.iter().position(|v| *v == name).map(|i| i as u32)
}

fn normalize(text: &Rope, tokens: Vec<LspSemanticToken>) -> SemanticTokens {
    let mut absolute = tokens
        .into_iter()
        .filter_map(|token| {
            let kind = category(&token)?;
            let range = wire_range_to_offsets(
                text,
                wire_position(&token.range.start),
                wire_position(&token.range.end),
            );
            let start = editor_position(text, range.start);
            let end = editor_position(text, range.end);
            (start.line == end.line && start.character < end.character)
                .then_some((start, end, kind))
        })
        .collect::<Vec<_>>();
    absolute.sort_by_key(|(start, _, _)| *start);
    let mut previous = lsp_types::Position::default();
    let data = absolute
        .into_iter()
        .map(|(start, end, kind)| {
            let token = SemanticToken {
                delta_line: start.line - previous.line,
                delta_start: if start.line == previous.line {
                    start.character - previous.character
                } else {
                    start.character
                },
                length: end.character - start.character,
                token_type: kind,
                token_modifiers_bitset: 0,
            };
            previous = start;
            token
        })
        .collect();
    SemanticTokens {
        result_id: None,
        data,
    }
}

impl DocumentRangeSemanticTokensProvider for LspBridge {
    fn legend(&self) -> SemanticTokensLegend {
        SemanticTokensLegend {
            token_types: TYPES
                .iter()
                .map(|name| SemanticTokenType::new(name))
                .collect(),
            token_modifiers: vec![],
        }
    }

    fn semantic_tokens(
        &self,
        text: &Rope,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut App,
    ) -> Task<Result<SemanticTokens>> {
        let Some(path) = self.current_document() else {
            return Task::ready(Ok(SemanticTokens::default()));
        };
        let start = self
            .position_payload(&path, offset_to_wire(text, range.start))
            .position;
        let end = self
            .position_payload(&path, offset_to_wire(text, range.end))
            .position;
        let payload = LspCodeActionsInput {
            cwd: tnes(&self.cwd),
            relative_path: tnes(path),
            range: WireRange { start, end },
        };
        let text = text.clone();
        let flush = self.flush_document(&text, cx);
        let client = self.client.clone();
        let epoch = self.epoch.clone();
        let generation = epoch.get();
        cx.spawn(async move |_| {
            flush.await;
            let result = client.call::<LspSemanticTokens>(&payload).await?;
            anyhow::ensure!(
                epoch.get() == generation,
                "Document changed during semantic highlighting"
            );
            Ok(normalize(&text, result.tokens))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantic_unicode_columns_and_readonly_styles() {
        let text = Rope::from("\"😀\"; value\ncall()");
        let tokens = serde_json::from_value(serde_json::json!([
            {"range":{"start":{"line":0,"character":6},"end":{"line":0,"character":11}},"kind":"variable","modifiers":["readonly"]},
            {"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":4}},"kind":"function","modifiers":[]}
        ])).unwrap();
        let result = normalize(&text, tokens);
        assert_eq!(result.data[0].delta_start, 5);
        assert_eq!(result.data[0].length, 5);
        assert_eq!(TYPES[result.data[0].token_type as usize], "constant");
        assert_eq!(result.data[1].delta_line, 1);
        assert_eq!(result.data[1].delta_start, 0);
    }
}
