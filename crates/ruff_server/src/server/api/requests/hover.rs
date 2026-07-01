use crate::edit::{RangeExt, ToRangeExt};
use crate::server::Result;
use crate::session::{Client, DocumentSnapshot};
use anyhow::Context;
use lsp_types::{self as types, HoverRequest};
use ruff_linter::FixAvailability;
use ruff_linter::registry::{Linter, Rule, RuleNamespace};
use ruff_linter::suppression::rule_identifier_range_at_offset;
use ruff_python_ast::SourceType;
use ruff_python_ast::token::TokenKind;
use ruff_python_parser::parse_unchecked_source;
use ruff_source_file::OneIndexed;
use ruff_text_size::Ranged;
use std::fmt::Write;

pub(crate) struct Hover;

impl super::RequestHandler for Hover {
    type RequestType = HoverRequest;
}

impl super::BackgroundDocumentRequestHandler for Hover {
    fn document_uri(params: &types::HoverParams) -> std::borrow::Cow<'_, lsp_types::Uri> {
        std::borrow::Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: Self::Snapshot,
        _client: &Client,
        params: types::HoverParams,
    ) -> Result<Option<types::Hover>> {
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(uri) => {
                tracing::warn!(
                    "Returning no hover information because document `{uri}` isn't open."
                );
                return Ok(None);
            }
        };

        Ok(hover(&snapshot, &params.text_document_position_params))
    }
}

pub(crate) fn hover(
    snapshot: &DocumentSnapshot,
    position: &types::TextDocumentPositionParams,
) -> Option<types::Hover> {
    // Don't show noqa hover for non-Python documents (e.g., markdown files).
    let SourceType::Python(source_type) = snapshot.query().source_type_for_lint() else {
        return None;
    };

    let document = snapshot
        .query()
        .as_single_document()
        .context("Failed to get text document for the hover request")
        .unwrap();
    let line_number: usize = position
        .position
        .line
        .try_into()
        .expect("line number should fit within a usize");
    let line_range = document.index().line_range(
        OneIndexed::from_zero_indexed(line_number),
        document.contents(),
    );

    let line = &document.contents()[line_range];

    // Avoid parsing the document if the hovered line doesn't contain a comment.
    memchr::memchr(b'#', line.as_bytes())?;

    let cursor = types::Range::new(position.position, position.position)
        .to_text_range(document.contents(), document.index(), snapshot.encoding())
        .start();
    let parsed = parse_unchecked_source(document.contents(), source_type);
    let comment = parsed
        .tokens()
        .at_offset(cursor)
        .find(|token| token.kind() == TokenKind::Comment)?;
    let identifier_range =
        rule_identifier_range_at_offset(document.contents(), comment.range(), cursor)?;

    // Get the rule for the identifier under the cursor.
    let identifier = &document.contents()[identifier_range];
    let rule = Rule::from_code(identifier)
        .ok()
        .or_else(|| Rule::from_name(identifier).ok());
    let output = if let Some(rule) = rule {
        format_rule_text(rule)
    } else {
        format!("{identifier}: Rule not found")
    };

    let hover = types::Hover {
        contents: types::MarkupContent {
            kind: types::MarkupKind::Markdown,
            value: output,
        }
        .into(),
        range: Some(identifier_range.to_range(
            document.contents(),
            document.index(),
            snapshot.encoding(),
        )),
    };

    Some(hover)
}

fn format_rule_text(rule: Rule) -> String {
    let mut output = String::new();
    let _ = write!(&mut output, "# {} ({})", rule.name(), rule.noqa_code());
    output.push('\n');
    output.push('\n');

    let (linter, _) = Linter::parse_code(&rule.noqa_code().to_string()).unwrap();
    let _ = write!(
        &mut output,
        "Derived from the **{}** linter.",
        linter.name()
    );
    output.push('\n');
    output.push('\n');

    let fix_availability = rule.fixable();
    if matches!(
        fix_availability,
        FixAvailability::Always | FixAvailability::Sometimes
    ) {
        output.push_str(&fix_availability.to_string());
        output.push('\n');
        output.push('\n');
    }

    if rule.is_preview() {
        output.push_str(r"This rule is in preview and is not stable.");
        output.push('\n');
        output.push('\n');
    }

    if let Some(explanation) = rule.explanation() {
        output.push_str(explanation.trim());
    } else {
        tracing::warn!("Rule {} does not have an explanation", rule.noqa_code());
        output.push_str("An issue occurred: an explanation for this rule was not found.");
    }
    output
}
