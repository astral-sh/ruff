use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use lsp_types::{self as types, request as req};
use regex::Regex;
use ruff_diagnostics::FixAvailability;
use ruff_linter::registry::{Linter, Rule, RuleNamespace};

pub(crate) struct Hover;

impl super::RequestHandler for Hover {
    type RequestType = req::HoverRequest;
}

impl super::BackgroundDocumentRequestHandler for Hover {
    fn document_url(params: &types::HoverParams) -> std::borrow::Cow<lsp_types::Url> {
        let url = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        std::borrow::Cow::Owned(url)
    }
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        params: types::HoverParams,
    ) -> Result<Option<types::Hover>> {
        hover(&snapshot, &params.text_document_position_params)
    }
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn hover(
    snapshot: &DocumentSnapshot,
    position: &types::TextDocumentPositionParams,
) -> Result<Option<types::Hover>> {
    let doc: &str = snapshot.document().contents();
    let binding = String::from(doc);
    let line = binding.lines().nth(position.position.line as usize);
    let line = line.unwrap();

    if !line.contains("noqa") {
        return Ok(None); // No noqa in line
    }

    // Get the list of codes.
    let re = Regex::new(r"(?i:# (?:(?:ruff|flake8): )?(?P<noqa>noqa))(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?").unwrap();
    let caps = re.captures(line).unwrap();
    let codes = caps.name("codes").unwrap().as_str();

    // Get the word under the cursor.
    let pos = position.position.character as usize;
    let words: Vec<&str> = line.split(' ').collect();
    let mut start = 0;
    let mut word = "";
    for &w in &words {
        let end = start + w.len();
        if pos >= start && pos < end {
            let w = w.trim_end_matches(',');
            word = w;
            break;
        }
        start = end + 1;
    }

    if !codes.contains(word) || word.is_empty() {
        return Ok(None); // Cursor was not over a code.
    }

    // Get rule for the code under the cursor.
    let rule = Rule::from_code(word);
    let output = if let Ok(rule) = rule {
        format_rule_text(rule)
    } else {
        format!("{word}: Rule not found")
    };

    let hover = types::Hover {
        contents: types::HoverContents::Markup(types::MarkupContent {
            kind: types::MarkupKind::Markdown,
            value: output,
        }),
        range: None,
    };

    Ok(Some(hover))
}

fn format_rule_text(rule: Rule) -> String {
    let mut output = String::new();
    output.push_str(&format!("# {} ({})", rule.as_ref(), rule.noqa_code()));
    output.push('\n');
    output.push('\n');

    let (linter, _) = Linter::parse_code(&rule.noqa_code().to_string()).unwrap();
    output.push_str(&format!("Derived from the **{}** linter.", linter.name()));
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

    //if rule.is_preview() || rule.is_nursery() {
    //output.push_str(
    //r"This rule is in preview and is not stable. The `--preview` flag is required for use.",
    //);
    //output.push('\n');
    //output.push('\n');
    //}

    if let Some(explanation) = rule.explanation() {
        output.push_str(explanation.trim());
    } else {
        output.push_str("Something went wrong.");
        //output.push_str("Message formats:");
        //for format in rule.message_formats() {
        //output.push('\n');
        //output.push_str(&format!("* {format}"));
        //}
    }
    output
}
