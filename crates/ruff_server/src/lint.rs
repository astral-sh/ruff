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
use ruff_notebook::Notebook;
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_python_parser::AsMode;
use ruff_source_file::{LineIndex, Locator};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{
    edit::{NotebookRange, ToRangeExt},
    session::DocumentQuery,
    PositionEncoding, DIAGNOSTIC_NAME,
};

/// This is serialized on the diagnostic `data` field.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct AssociatedDiagnosticData {
    pub(crate) kind: DiagnosticKind,
    /// Edits to fix the diagnostic. If this is empty, a fix
    /// does not exist.
    pub(crate) edits: Vec<lsp_types::TextEdit>,
    /// The NOQA code for the diagnostic.
    pub(crate) code: String,
    /// Possible edit to add a `noqa` comment which will disable this diagnostic.
    pub(crate) noqa_edit: Option<lsp_types::TextEdit>,
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

/// A series of diagnostics across a single text document or an arbitrary number of notebook cells.
pub(crate) type Diagnostics = FxHashMap<lsp_types::Url, Vec<lsp_types::Diagnostic>>;

pub(crate) fn check(
    query: &DocumentQuery,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
) -> Diagnostics {
    let document_path = query.file_path();
    let source_kind = query.make_source_kind();

    let package = detect_package_root(
        document_path
            .parent()
            .expect("a path to a document should have a parent path"),
        &linter_settings.namespace_packages,
    );

    let source_type = query.source_type();

    // Tokenize once.
    let tokens = ruff_python_parser::tokenize(source_kind.source_code(), source_type.as_mode());

    let index = LineIndex::from_source_text(source_kind.source_code());

    // Map row and column locations to byte slices (lazily).
    let locator = Locator::with_index(source_kind.source_code(), index.clone());

    // Detect the current code style (lazily).
    let stylist = Stylist::from_tokens(&tokens, &locator);

    // Extra indices from the code.
    let indexer = Indexer::from_tokens(&tokens, &locator);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = extract_directives(&tokens, Flags::all(), &locator, &indexer);

    // Generate checks.
    let LinterResult { data, .. } = check_path(
        document_path,
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
        document_path,
        data.as_slice(),
        &locator,
        indexer.comment_ranges(),
        &linter_settings.external,
        &directives.noqa_line_for,
        stylist.line_ending(),
    );

    let mut diagnostics = Diagnostics::default();

    // Populate all cell URLs with an empty diagnostic list.
    // This ensures that cells without diagnostics still get updated.
    if let Some(notebook) = query.as_notebook() {
        for url in notebook.urls() {
            diagnostics.entry(url.clone()).or_default();
        }
    }

    let lsp_diagnostics = data
        .into_iter()
        .zip(noqa_edits)
        .map(|(diagnostic, noqa_edit)| {
            to_lsp_diagnostic(diagnostic, &noqa_edit, &source_kind, &index, encoding)
        });

    if let Some(notebook) = query.as_notebook() {
        for (index, diagnostic) in lsp_diagnostics {
            let Some(uri) = notebook.cell_uri_by_index(index) else {
                tracing::warn!("Unable to find notebook cell at index {index}.");
                continue;
            };
            diagnostics.entry(uri.clone()).or_default().push(diagnostic);
        }
    } else {
        for (_, diagnostic) in lsp_diagnostics {
            diagnostics
                .entry(query.make_key().into_url())
                .or_default()
                .push(diagnostic);
        }
    }

    diagnostics
}

/// Converts LSP diagnostics to a list of `DiagnosticFix`es by deserializing associated data on each diagnostic.
pub(crate) fn fixes_for_diagnostics(
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
            Ok(Some(DiagnosticFix {
                fixed_diagnostic,
                code: associated_data.code,
                title: associated_data
                    .kind
                    .suggestion
                    .unwrap_or(associated_data.kind.name),
                noqa_edit: associated_data.noqa_edit,
                edits: associated_data.edits,
            }))
        })
        .filter_map(crate::Result::transpose)
        .collect()
}

/// Generates an LSP diagnostic with an associated cell index for the diagnostic to go in.
/// If the source kind is a text document, the cell index will always be `0`.
fn to_lsp_diagnostic(
    diagnostic: Diagnostic,
    noqa_edit: &Option<Edit>,
    source_kind: &SourceKind,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> (usize, lsp_types::Diagnostic) {
    let Diagnostic {
        kind,
        range: diagnostic_range,
        fix,
        ..
    } = diagnostic;

    let rule = kind.rule();

    let fix = fix.and_then(|fix| fix.applies(Applicability::Unsafe).then_some(fix));

    let data = (fix.is_some() || noqa_edit.is_some())
        .then(|| {
            let edits = fix
                .as_ref()
                .into_iter()
                .flat_map(Fix::edits)
                .map(|edit| lsp_types::TextEdit {
                    range: diagnostic_edit_range(edit.range(), source_kind, index, encoding),
                    new_text: edit.content().unwrap_or_default().to_string(),
                })
                .collect();
            let noqa_edit = noqa_edit.as_ref().map(|noqa_edit| lsp_types::TextEdit {
                range: diagnostic_edit_range(noqa_edit.range(), source_kind, index, encoding),
                new_text: noqa_edit.content().unwrap_or_default().to_string(),
            });
            serde_json::to_value(AssociatedDiagnosticData {
                kind: kind.clone(),
                noqa_edit,
                edits,
                code: rule.noqa_code().to_string(),
            })
            .ok()
        })
        .flatten();

    let code = rule.noqa_code().to_string();

    let range: lsp_types::Range;
    let cell: usize;

    if let Some(notebook_index) = source_kind.as_ipy_notebook().map(Notebook::index) {
        NotebookRange { cell, range } = diagnostic_range.to_notebook_range(
            source_kind.source_code(),
            index,
            notebook_index,
            encoding,
        );
    } else {
        cell = usize::default();
        range = diagnostic_range.to_range(source_kind.source_code(), index, encoding);
    }

    (
        cell,
        lsp_types::Diagnostic {
            range,
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
        },
    )
}

fn diagnostic_edit_range(
    range: TextRange,
    source_kind: &SourceKind,
    index: &LineIndex,
    encoding: PositionEncoding,
) -> lsp_types::Range {
    if let Some(notebook_index) = source_kind.as_ipy_notebook().map(Notebook::index) {
        range
            .to_notebook_range(source_kind.source_code(), index, notebook_index, encoding)
            .range
    } else {
        range.to_range(source_kind.source_code(), index, encoding)
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
