use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{CodeDescription, DiagnosticSeverity, NumberOrString, Url};

use ruff_diagnostics::Fix;
use ruff_linter::message::Message;
use ruff_linter::registry::AsRule;
use ruff_text_size::TextSize;

use crate::document::Document;
use crate::encoding::{text_range_to_range, PositionEncoding};

pub(crate) fn to_lsp_diagnostic(
    message: Message,
    document: &Document,
    encoding: PositionEncoding,
) -> anyhow::Result<tower_lsp::lsp_types::Diagnostic> {
    let Message {
        kind,
        range,
        fix,
        file: _file,
        noqa_offset,
    } = message;

    let rule = kind.rule();

    let data = if let Some(fix) = fix {
        Some(serde_json::to_value(DiagnosticData {
            fix,
            noqa_offset,
            suggestion: kind.suggestion,
        })?)
    } else {
        None
    };

    Ok(tower_lsp::lsp_types::Diagnostic {
        range: text_range_to_range(range, document, encoding),
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(rule.noqa_code().to_string())),
        code_description: rule.url().and_then(|url| {
            Some(CodeDescription {
                href: Url::parse(&url).ok()?,
            })
        }),
        source: Some("ruff".to_string()),
        message: kind.body,
        related_information: None,
        tags: None,
        // TODO: Not all clients support the data property
        // TODO: Ideally we wouldn't compute the fixes already because that's a lot of unnecessary work.
        // It is further necessary to serialize the data between the client and server which is expensive too.
        // Instead, delay the fix and noqa offset computation until the editor requests the code actions.
        data,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DiagnosticData {
    pub(crate) fix: Fix,
    pub(crate) suggestion: Option<String>,
    pub(crate) noqa_offset: TextSize,
}
