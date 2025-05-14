use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use anyhow::Context;
use lsp_types::{self as types, request as req};
use regex::Regex;
use ruff_diagnostics::FixAvailability;
use ruff_linter::registry::{Linter, Rule, RuleNamespace};
use ruff_source_file::OneIndexed;
use std::fmt::Write;

pub(crate) struct Hover;

impl super::RequestHandler for Hover {
    type RequestType = req::HoverRequest;
}

impl super::BackgroundDocumentRequestHandler for Hover {
    fn document_url(params: &types::HoverParams) -> std::borrow::Cow<lsp_types::Url> {
        std::borrow::Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        params: types::HoverParams,
    ) -> Result<Option<types::Hover>> {
        Ok(hover(&snapshot, &params.text_document_position_params))
    }
}

pub(crate) fn hover(
    snapshot: &DocumentSnapshot,
    position: &types::TextDocumentPositionParams,
) -> Option<types::Hover> {
    // Hover only operates on text documents or notebook cells
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

    // Get the list of codes.
    let noqa_regex = Regex::new(r"(?i:# (?:(?:ruff|flake8): )?(?P<noqa>noqa))(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?").unwrap();
    let noqa_captures = noqa_regex.captures(line)?;
    let codes_match = noqa_captures.name("codes")?;
    let codes_start = codes_match.start();
    let code_regex = Regex::new(r"[A-Z]+[0-9]+").unwrap();
    let cursor: usize = position
        .position
        .character
        .try_into()
        .expect("column number should fit within a usize");
    let word = code_regex.find_iter(codes_match.as_str()).find(|code| {
        cursor >= (code.start() + codes_start) && cursor < (code.end() + codes_start)
    })?;

    // Get rule for the code under the cursor.
    let rule = Rule::from_code(word.as_str());
    let output = if let Ok(rule) = rule {
        format_rule_text(rule)
    } else {
        format!("{}: Rule not found", word.as_str())
    };

    let hover = types::Hover {
        contents: types::HoverContents::Markup(types::MarkupContent {
            kind: types::MarkupKind::Markdown,
            value: output,
        }),
        range: None,
    };

    Some(hover)
}

fn format_rule_text(rule: Rule) -> String {
    let mut output = String::new();
    let _ = write!(&mut output, "# {} ({})", rule.as_ref(), rule.noqa_code());
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
