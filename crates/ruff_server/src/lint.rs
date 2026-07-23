//! Access to the Ruff linting API for the LSP

use std::fmt::Write;
use std::path::Path;

use ruff_python_ast::SourceType;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{
    DIAGNOSTIC_NAME, PositionEncoding,
    edit::{NotebookDocument, NotebookRange, ToRangeExt},
    resolve::is_document_excluded_for_linting,
    session::DocumentQuery,
};
use ruff_db::diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic};
use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_linter::{
    Locator, SuppressionKind,
    directives::{Flags, extract_directives},
    generate_suppression_edits,
    linter::{check_path, parse_unchecked_source},
    package::PackageRoot,
    packaging::detect_package_root,
    preview::is_human_readable_names_enabled,
    settings::{LinterSettings, flags},
    source_kind::SourceKind,
    suppression::Suppressions,
};
use ruff_notebook::Notebook;
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

/// This is serialized on the diagnostic `data` field.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct AssociatedDiagnosticData {
    /// The message describing what the fix does, if it exists, or the diagnostic name otherwise.
    pub(crate) title: String,
    /// Edits to fix the diagnostic. If this is empty, a fix
    /// does not exist.
    pub(crate) edits: Vec<lsp_types::TextEdit>,
    /// The identifier displayed for the diagnostic.
    pub(crate) code: String,
    /// Possible edit to add a suppression comment which will disable this diagnostic.
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
    /// The identifier displayed for the diagnostic.
    pub(crate) code: String,
    /// Edits to fix the diagnostic. If this is empty, a fix
    /// does not exist.
    pub(crate) edits: Vec<lsp_types::TextEdit>,
    /// Possible edit to add a suppression comment which will disable this diagnostic.
    pub(crate) noqa_edit: Option<lsp_types::TextEdit>,
}

/// A series of diagnostics across a single text document or an arbitrary number of notebook cells.
pub(crate) type DiagnosticsMap = FxHashMap<lsp_types::Uri, Vec<lsp_types::Diagnostic>>;

pub(crate) fn check(
    query: &DocumentQuery,
    encoding: PositionEncoding,
    show_syntax_errors: bool,
    supports_related_information: bool,
) -> DiagnosticsMap {
    let settings = query.settings();
    let document_path = query.virtual_file_path();

    let SourceType::Python(source_type) = query.source_type_for_lint() else {
        return DiagnosticsMap::default();
    };
    let source_kind = query.make_python_source_kind(source_type);
    let document_uri = query.make_key().into_uri();
    let notebook = query.as_notebook();

    // If the document is excluded, return an empty list of diagnostics.
    if is_document_excluded_for_linting(
        &document_path,
        &settings.file_resolver,
        &settings.linter,
        query.text_document_language_id(),
    ) {
        return DiagnosticsMap::default();
    }

    let file_path = query.file_path();
    let package = if let Some(file_path) = &file_path {
        detect_package_root(
            file_path
                .parent()
                .expect("a path to a document should have a parent path"),
            &settings.linter.namespace_packages,
        )
        .map(PackageRoot::root)
    } else {
        None
    };

    let target_version = settings.linter.resolve_target_version(&document_path);

    // Parse once.
    let parsed = parse_unchecked_source(&source_kind, source_type, target_version.parser_version());

    // Map row and column locations to byte slices (lazily).
    let locator = Locator::new(source_kind.source_code());

    // Detect the current code style (lazily).
    let stylist = Stylist::from_tokens(parsed.tokens(), locator.contents());

    // Extra indices from the code.
    let indexer = Indexer::from_tokens(parsed.tokens(), locator.contents());

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = extract_directives(parsed.tokens(), Flags::all(), &locator, &indexer);

    // Parse range suppression comments
    let suppressions = Suppressions::from_tokens(
        locator.contents(),
        parsed.tokens(),
        &indexer,
        &settings.linter,
    );

    // Generate checks.
    let diagnostics = check_path(
        &document_path,
        package,
        &locator,
        &stylist,
        &indexer,
        &directives,
        &settings.linter,
        flags::Noqa::Enabled,
        &source_kind,
        source_type,
        &parsed,
        target_version,
        &suppressions,
    );

    let suppression_edits = generate_suppression_edits(
        &document_path,
        &diagnostics,
        &locator,
        indexer.comment_ranges(),
        &settings.linter.external,
        &directives.noqa_line_for,
        stylist.line_ending(),
        &suppressions,
        if is_human_readable_names_enabled(settings.linter.preview)
            && !settings.linter.prefer_rule_codes_in_output
        {
            SuppressionKind::Ignore
        } else {
            SuppressionKind::Noqa
        },
        settings.linter.preview,
    );
    let context = LspDiagnosticContext {
        source_kind: &source_kind,
        index: locator.to_index(),
        encoding,
        document_path: document_path.as_ref(),
        document_uri: &document_uri,
        notebook,
        supports_related_information,
        settings: &settings.linter,
    };

    let mut diagnostics_map = DiagnosticsMap::default();

    // Populates all relevant URLs with an empty diagnostic list.
    // This ensures that documents without diagnostics still get updated.
    if let Some(notebook) = query.as_notebook() {
        for uri in notebook.uris() {
            diagnostics_map.entry(uri.clone()).or_default();
        }
    } else {
        diagnostics_map
            .entry(query.make_key().into_uri())
            .or_default();
    }

    let lsp_diagnostics =
        diagnostics
            .into_iter()
            .zip(suppression_edits)
            .filter_map(|(message, noqa_edit)| {
                if message.is_invalid_syntax() && !show_syntax_errors {
                    None
                } else {
                    Some(to_lsp_diagnostic(&message, noqa_edit, &context))
                }
            });

    if let Some(notebook) = query.as_notebook() {
        for (index, diagnostic) in lsp_diagnostics {
            let Some(uri) = notebook.cell_uri_by_index(index) else {
                tracing::warn!("Unable to find notebook cell at index {index}.");
                continue;
            };
            diagnostics_map
                .entry(uri.clone())
                .or_default()
                .push(diagnostic);
        }
    } else {
        diagnostics_map
            .entry(query.make_key().into_uri())
            .or_default()
            .extend(lsp_diagnostics.map(|(_, diagnostic)| diagnostic));
    }

    diagnostics_map
}

/// Converts LSP diagnostics to a list of `DiagnosticFix`es by deserializing associated data on each diagnostic.
pub(crate) fn fixes_for_diagnostics(
    diagnostics: Vec<lsp_types::Diagnostic>,
) -> crate::Result<Vec<DiagnosticFix>> {
    diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.source.as_deref() == Some(DIAGNOSTIC_NAME))
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
                title: associated_data.title,
                noqa_edit: associated_data.noqa_edit,
                edits: associated_data.edits,
            }))
        })
        .filter_map(crate::Result::transpose)
        .collect()
}

struct LspDiagnosticContext<'a> {
    source_kind: &'a SourceKind,
    index: &'a LineIndex,
    encoding: PositionEncoding,
    document_path: &'a Path,
    document_uri: &'a lsp_types::Uri,
    notebook: Option<&'a NotebookDocument>,
    supports_related_information: bool,
    settings: &'a LinterSettings,
}

/// Generates an LSP diagnostic with an associated cell index for the diagnostic to go in.
/// If the source kind is a text document, the cell index will always be `0`.
fn to_lsp_diagnostic(
    diagnostic: &Diagnostic,
    noqa_edit: Option<Edit>,
    context: &LspDiagnosticContext,
) -> (usize, lsp_types::Diagnostic) {
    let diagnostic_range = diagnostic.range().unwrap_or_default();
    let name = diagnostic.name();
    let fix = diagnostic.fix();
    let suggestion = diagnostic.first_help_text();
    let fix = fix.and_then(|fix| fix.applies(Applicability::Unsafe).then_some(fix));

    let (severity, code) = if let Some(code) = diagnostic.secondary_code() {
        let severity = severity(code);
        let code = if is_human_readable_names_enabled(context.settings.preview)
            && !context.settings.prefer_rule_codes_in_output
        {
            name.to_string()
        } else {
            code.to_string()
        };
        (severity, code)
    } else {
        (
            match diagnostic.severity() {
                ruff_db::diagnostic::Severity::Info => lsp_types::DiagnosticSeverity::Information,
                ruff_db::diagnostic::Severity::Warning => lsp_types::DiagnosticSeverity::Warning,
                ruff_db::diagnostic::Severity::Error => lsp_types::DiagnosticSeverity::Error,
                ruff_db::diagnostic::Severity::Fatal => lsp_types::DiagnosticSeverity::Error,
            },
            diagnostic.id().to_string(),
        )
    };

    let data = (fix.is_some() || noqa_edit.is_some())
        .then(|| {
            let edits = fix
                .into_iter()
                .flat_map(Fix::edits)
                .map(|edit| lsp_types::TextEdit {
                    range: diagnostic_edit_range(
                        edit.range(),
                        context.source_kind,
                        context.index,
                        context.encoding,
                    ),
                    new_text: edit.content().unwrap_or_default().to_string(),
                })
                .collect();
            let noqa_edit = noqa_edit.map(|noqa_edit| lsp_types::TextEdit {
                range: diagnostic_edit_range(
                    noqa_edit.range(),
                    context.source_kind,
                    context.index,
                    context.encoding,
                ),
                new_text: noqa_edit.into_content().unwrap_or_default().into_string(),
            });
            serde_json::to_value(AssociatedDiagnosticData {
                title: suggestion.unwrap_or(name).to_string(),
                noqa_edit,
                edits,
                code: code.clone(),
            })
            .ok()
        })
        .flatten();

    let range: lsp_types::Range;
    let cell: usize;

    if let Some(notebook_index) = context.source_kind.as_ipy_notebook().map(Notebook::index) {
        NotebookRange { cell, range } = diagnostic_range.to_notebook_range(
            context.source_kind.source_code(),
            context.index,
            notebook_index,
            context.encoding,
        );
    } else {
        cell = usize::default();
        range = diagnostic_range.to_range(
            context.source_kind.source_code(),
            context.index,
            context.encoding,
        );
    }

    let related_information =
        if context.supports_related_information {
            let mut related_information = Vec::new();
            related_information.extend(
                diagnostic.secondary_annotations().filter_map(|annotation| {
                    annotation_to_related_information(annotation, context)
                }),
            );

            for sub_diagnostic in diagnostic.sub_diagnostics() {
                related_information.extend(sub_diagnostic_to_related_information(
                    sub_diagnostic,
                    context,
                ));
                related_information.extend(sub_diagnostic.secondary_annotations().filter_map(
                    |annotation| annotation_to_related_information(annotation, context),
                ));
            }

            Some(related_information)
        } else {
            None
        };

    let mut body = if context.supports_related_information {
        if let Some(annotation_message) = diagnostic
            .primary_annotation()
            .and_then(Annotation::get_message)
        {
            format!("{}: {annotation_message}", diagnostic.primary_message())
        } else {
            diagnostic.primary_message().to_string()
        }
    } else {
        diagnostic.concise_message().to_string()
    };

    // Append sub-diagnostics that have no location (and thus can't be shown as related
    // information) to the message.
    let mut first = true;
    for sub_diagnostic in diagnostic.sub_diagnostics() {
        if sub_diagnostic.primary_annotation().is_none() {
            if first {
                body.push('\n');
                first = false;
            }
            write!(
                body,
                "\n{severity}: {message}",
                severity = sub_diagnostic.severity(),
                message = sub_diagnostic.concise_message(),
            )
            .ok();
        }
    }

    (
        cell,
        lsp_types::Diagnostic {
            range,
            severity: Some(severity),
            tags: tags(diagnostic),
            code: Some(lsp_types::Code::String(code)),
            code_description: diagnostic.documentation_url().and_then(|url| {
                Some(lsp_types::CodeDescription {
                    href: lsp_types::Uri::parse(url).ok()?,
                })
            }),
            source: Some(DIAGNOSTIC_NAME.into()),
            message: body.into(),
            related_information,
            data,
        },
    )
}

fn annotation_to_related_information(
    annotation: &Annotation,
    context: &LspDiagnosticContext,
) -> Option<lsp_types::DiagnosticRelatedInformation> {
    Some(lsp_types::DiagnosticRelatedInformation {
        location: span_to_location(annotation.get_span(), context)?,
        message: annotation.get_message()?.to_string(),
    })
}

fn sub_diagnostic_to_related_information(
    diagnostic: &SubDiagnostic,
    context: &LspDiagnosticContext,
) -> Option<lsp_types::DiagnosticRelatedInformation> {
    Some(lsp_types::DiagnosticRelatedInformation {
        location: span_to_location(diagnostic.primary_annotation()?.get_span(), context)?,
        message: diagnostic.concise_message().to_string(),
    })
}

fn span_to_location(span: &Span, context: &LspDiagnosticContext) -> Option<lsp_types::Location> {
    let source_file = span.as_ruff_file()?;
    if Path::new(source_file.name()) != context.document_path {
        return None;
    }
    let range = span.range()?;

    if let Some(notebook) = context.notebook {
        let notebook_index = context.source_kind.as_ipy_notebook().map(Notebook::index)?;
        let NotebookRange { cell, range } = range.to_notebook_range(
            source_file.source_text(),
            source_file.index(),
            notebook_index,
            context.encoding,
        );
        Some(lsp_types::Location {
            uri: notebook.cell_uri_by_index(cell)?.clone(),
            range,
        })
    } else {
        Some(lsp_types::Location {
            uri: context.document_uri.clone(),
            range: range.to_range(
                source_file.source_text(),
                source_file.index(),
                context.encoding,
            ),
        })
    }
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
        "F821" | "E902" => lsp_types::DiagnosticSeverity::Error,
        _ => lsp_types::DiagnosticSeverity::Warning,
    }
}

fn tags(diagnostic: &Diagnostic) -> Option<Vec<lsp_types::DiagnosticTag>> {
    diagnostic.primary_tags().map(|tags| {
        tags.iter()
            .map(|tag| match tag {
                ruff_db::diagnostic::DiagnosticTag::Unnecessary => {
                    lsp_types::DiagnosticTag::Unnecessary
                }
                ruff_db::diagnostic::DiagnosticTag::Deprecated => {
                    lsp_types::DiagnosticTag::Deprecated
                }
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use ruff_db::diagnostic::{DiagnosticId, Severity, SubDiagnosticSeverity};
    use ruff_source_file::SourceFileBuilder;
    use ruff_text_size::{TextRange, TextSize};

    use super::*;

    #[test]
    fn all_annotations_are_related_information() {
        let source = "abcdef";
        let source_file = SourceFileBuilder::new("test.py", source).finish();
        let span = |offset| {
            Span::from(source_file.clone())
                .with_range(TextRange::at(TextSize::new(offset), TextSize::new(1)))
        };

        let mut diagnostic = Diagnostic::new(
            DiagnosticId::lint("synthetic"),
            Severity::Error,
            "Synthetic diagnostic",
        );
        diagnostic.annotate(Annotation::primary(span(0)).message("Primary annotation"));
        diagnostic.annotate(Annotation::secondary(span(1)).message("Secondary annotation"));
        diagnostic.annotate(Annotation::primary(span(2)).message("Additional primary annotation"));
        diagnostic.annotate(Annotation::secondary(span(3)));
        diagnostic.annotate(
            Annotation::secondary(
                Span::from(SourceFileBuilder::new("other.py", "x").finish())
                    .with_range(TextRange::new(TextSize::new(0), TextSize::new(1))),
            )
            .message("Foreign annotation"),
        );

        let mut sub_diagnostic =
            SubDiagnostic::new(SubDiagnosticSeverity::Info, "Synthetic subdiagnostic");
        sub_diagnostic.annotate(Annotation::secondary(span(3)).message("Secondary subannotation"));
        sub_diagnostic.annotate(Annotation::primary(span(4)).message("Primary subannotation"));
        sub_diagnostic
            .annotate(Annotation::primary(span(5)).message("Additional primary subannotation"));
        diagnostic.sub(sub_diagnostic);
        diagnostic.sub(SubDiagnostic::new(
            SubDiagnosticSeverity::Help,
            "Unlocated subdiagnostic",
        ));

        let source_kind = SourceKind::Python {
            code: source.to_string(),
            is_stub: false,
        };
        let index = LineIndex::from_source_text(source);
        let uri = lsp_types::Uri::parse("file:///test.py").expect("URI to be valid");
        let settings = LinterSettings::default();
        let context = LspDiagnosticContext {
            source_kind: &source_kind,
            index: &index,
            encoding: PositionEncoding::UTF8,
            document_path: Path::new("test.py"),
            document_uri: &uri,
            notebook: None,
            supports_related_information: true,
            settings: &settings,
        };
        let (_, lsp_diagnostic) = to_lsp_diagnostic(&diagnostic, None, &context);

        assert_eq!(
            lsp_diagnostic.message,
            lsp_types::Message::String(
                "Synthetic diagnostic: Primary annotation\n\nhelp: Unlocated subdiagnostic"
                    .to_string()
            )
        );
        let related_information = lsp_diagnostic
            .related_information
            .expect("client supports diagnostic related information");
        assert!(
            related_information
                .iter()
                .all(|information| information.location.uri == uri)
        );
        assert_eq!(
            related_information
                .iter()
                .map(|information| information.location.range.start.character)
                .collect::<Vec<_>>(),
            [1, 2, 4, 3, 5]
        );
        assert_eq!(
            related_information
                .iter()
                .map(|information| information.message.as_str())
                .collect::<Vec<_>>(),
            [
                "Secondary annotation",
                "Additional primary annotation",
                "Synthetic subdiagnostic: Primary subannotation",
                "Secondary subannotation",
                "Additional primary subannotation",
            ]
        );
    }
}
