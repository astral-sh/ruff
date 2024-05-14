//! Access to the Ruff linting API for the LSP

use ruff_diagnostics::{Applicability, Diagnostic, DiagnosticKind, Edit, Fix};
use ruff_linter::{
    directives::{extract_directives, Flags},
    generate_noqa_edits,
    linter::{check_path, LinterResult, TokenSource},
    packaging::detect_package_root,
    registry::AsRule,
    settings::{flags, LinterSettings},
    source_kind::SourceKind,
};
use ruff_python_ast::PySourceType;
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_parser::AsMode;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;
use serde::{Deserialize, Serialize};

use crate::{edit::ToRangeExt, PositionEncoding, DIAGNOSTIC_NAME};

/// This is serialized on the diagnostic `data` field.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct AssociatedDiagnosticData {
    pub(crate) kind: DiagnosticKind,
    /// A possible fix for the associated diagnostic.
    pub(crate) fix: Option<Fix>,
    /// The NOQA code for the diagnostic.
    pub(crate) code: String,
    /// Possible edit to add a `noqa` comment which will disable this diagnostic.
    pub(crate) noqa_edit: Option<ruff_diagnostics::Edit>,
}

/// Describes a fix for `fixed_diagnostic` that may have quick fix
/// edits available, `noqa` comment edits, or both.
#[derive(Clone, Debug)]
pub(crate) struct DiagnosticFix {
    /// The original diagnostic to be fixed
    pub(crate) fixed_diagnostic: lsp_types::Diagnostic,
    /// The message describing what the fix does.
    pub(crate) title: String,
    /// The NOQA code for the diagnostic.
    pub(crate) code: String,
    /// Edits to fix the diagnostic. If this is empty, a fix
    /// does not exist.
    pub(crate) edits: Vec<lsp_types::TextEdit>,
    /// Possible edit to add a `noqa` comment which will disable this diagnostic.
    pub(crate) noqa_edit: Option<lsp_types::TextEdit>,
}

pub(crate) fn check(
    document: &crate::edit::Document,
    document_url: &lsp_types::Url,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
) -> Vec<lsp_types::Diagnostic> {
    let contents = document.contents();
    let index = document.index().clone();

    let document_path = document_url
        .to_file_path()
        .expect("document URL should be a valid file path");

    let package = detect_package_root(
        document_path
            .parent()
            .expect("a path to a document should have a parent path"),
        &linter_settings.namespace_packages,
    );

    let source_type = PySourceType::default();

    // TODO(jane): Support Jupyter Notebooks
    let source_kind = SourceKind::Python(contents.to_string());

    // Tokenize once.
    let tokens = ruff_python_parser::tokenize(contents, source_type.as_mode());

    // Map row and column locations to byte slices (lazily).
    let locator = Locator::with_index(contents, index);

    // Detect the current code style (lazily).
    let stylist = Stylist::from_tokens(&tokens, &locator);

    // Extra indices from the code.
    let indexer = Indexer::from_tokens(&tokens, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = extract_directives(&tokens, Flags::all(), &locator, &indexer);

    // Generate checks.
    let LinterResult {
        data: diagnostics, ..
    } = check_path(
        &document_path,
        package,
        &locator,
        &stylist,
        &indexer,
        &directives,
        linter_settings,
        flags::Noqa::Enabled,
        &source_kind,
        source_type,
        TokenSource::Tokens(tokens),
    );

    let noqa_edits = generate_noqa_edits(
        &document_path,
        diagnostics.as_slice(),
        &locator,
        indexer.comment_ranges(),
        &linter_settings.external,
        &directives.noqa_line_for,
        stylist.line_ending(),
    );

    diagnostics
        .into_iter()
        .zip(noqa_edits)
        .map(|(diagnostic, noqa_edit)| to_lsp_diagnostic(diagnostic, noqa_edit, document, encoding))
        .collect()
}

pub(crate) fn fixes_for_diagnostics(
    document: &crate::edit::Document,
    encoding: PositionEncoding,
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> crate::Result<Vec<DiagnosticFix>> {
    diagnostics
        .into_iter()
        .map(move |mut diagnostic| {
            let Some(data) = diagnostic.data.take() else {
                return Ok(None);
            };
            let fixed_diagnostic = diagnostic;
            let associated_data: crate::lint::AssociatedDiagnosticData =
                serde_json::from_value(data).map_err(|err| {
                    anyhow::anyhow!("failed to deserialize diagnostic data: {err}")
                })?;
            let edits = associated_data
                .fix
                .map(|fix| {
                    fix.edits()
                        .iter()
                        .map(|edit| lsp_types::TextEdit {
                            range: edit.range().to_range(
                                document.contents(),
                                document.index(),
                                encoding,
                            ),
                            new_text: edit.content().unwrap_or_default().to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            let noqa_edit =
                associated_data
                    .noqa_edit
                    .as_ref()
                    .map(|noqa_edit| lsp_types::TextEdit {
                        range: noqa_edit.range().to_range(
                            document.contents(),
                            document.index(),
                            encoding,
                        ),
                        new_text: noqa_edit.content().unwrap_or_default().to_string(),
                    });

            Ok(Some(DiagnosticFix {
                fixed_diagnostic,
                code: associated_data.code,
                title: associated_data
                    .kind
                    .suggestion
                    .unwrap_or(associated_data.kind.name),
                edits,
                noqa_edit,
            }))
        })
        .filter_map(crate::Result::transpose)
        .collect()
}

fn to_lsp_diagnostic(
    diagnostic: Diagnostic,
    noqa_edit: Option<Edit>,
    document: &crate::edit::Document,
    encoding: PositionEncoding,
) -> lsp_types::Diagnostic {
    let Diagnostic {
        kind, range, fix, ..
    } = diagnostic;

    let rule = kind.rule();

    let fix = fix.and_then(|fix| fix.applies(Applicability::Unsafe).then_some(fix));

    let data = (fix.is_some() || noqa_edit.is_some())
        .then(|| {
            serde_json::to_value(&AssociatedDiagnosticData {
                kind: kind.clone(),
                fix,
                code: rule.noqa_code().to_string(),
                noqa_edit,
            })
            .ok()
        })
        .flatten();

    let code = rule.noqa_code().to_string();

    lsp_types::Diagnostic {
        range: range.to_range(document.contents(), document.index(), encoding),
        severity: Some(severity(&code)),
        tags: tags(&code),
        code: Some(lsp_types::NumberOrString::String(code)),
        code_description: rule.url().and_then(|url| {
            Some(lsp_types::CodeDescription {
                href: lsp_types::Url::parse(&url).ok()?,
            })
        }),
        source: Some(DIAGNOSTIC_NAME.into()),
        message: kind.body,
        related_information: None,
        data,
    }
}

fn severity(code: &str) -> lsp_types::DiagnosticSeverity {
    match code {
        // F821: undefined name <name>
        // E902: IOError
        // E999: SyntaxError
        "F821" | "E902" | "E999" => lsp_types::DiagnosticSeverity::ERROR,
        _ => lsp_types::DiagnosticSeverity::WARNING,
    }
}

fn tags(code: &str) -> Option<Vec<lsp_types::DiagnosticTag>> {
    match code {
        // F401: <module> imported but unused
        // F841: local variable <name> is assigned to but never used
        "F401" | "F841" => Some(vec![lsp_types::DiagnosticTag::UNNECESSARY]),
        _ => None,
    }
}
