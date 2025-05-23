use lsp_server::ErrorCode;
use lsp_types::notification::PublishDiagnostics;
use lsp_types::{
    CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag,
    NumberOrString, PublishDiagnosticsParams, Range, Url,
};
use rustc_hash::FxHashMap;

use ruff_db::diagnostic::{Annotation, Severity, SubDiagnostic};
use ruff_db::files::FileRange;
use ruff_db::source::{line_index, source_text};
use ty_project::{Db, ProjectDatabase};

use crate::document::{FileRangeExt, ToRangeExt};
use crate::server::Result;
use crate::server::client::Notifier;
use crate::{DocumentSnapshot, PositionEncoding};

use super::LSPResult;

/// A series of diagnostics across a single text document or an arbitrary number of notebook cells.
pub(super) type DiagnosticsMap = FxHashMap<Url, Vec<Diagnostic>>;

/// Clears the diagnostics for the document at `uri`.
///
/// This is done by notifying the client with an empty list of diagnostics for the document.
pub(super) fn clear_diagnostics(uri: &Url, notifier: &Notifier) -> Result<()> {
    notifier
        .notify::<PublishDiagnostics>(PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: vec![],
            version: None,
        })
        .with_failure_code(ErrorCode::InternalError)?;
    Ok(())
}

/// Publishes the diagnostics for the given document snapshot using the [publish diagnostics
/// notification].
///
/// [publish diagnostics notification]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_publishDiagnostics
pub(super) fn publish_diagnostics_for_document(
    db: &ProjectDatabase,
    snapshot: &DocumentSnapshot,
    notifier: &Notifier,
) -> Result<()> {
    for (uri, diagnostics) in compute_diagnostics(db, snapshot) {
        notifier
            .notify::<PublishDiagnostics>(PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: Some(snapshot.query().version()),
            })
            .with_failure_code(lsp_server::ErrorCode::InternalError)?;
    }

    Ok(())
}

pub(super) fn compute_diagnostics(
    db: &ProjectDatabase,
    snapshot: &DocumentSnapshot,
) -> DiagnosticsMap {
    let Some(file) = snapshot.file(db) else {
        tracing::info!(
            "No file found for snapshot for `{}`",
            snapshot.query().file_url()
        );
        return DiagnosticsMap::default();
    };

    let diagnostics = db.check_file(file);

    let mut diagnostics_map = DiagnosticsMap::default();
    let query = snapshot.query();
    // let source_kind = query.make_source_kind();

    // Populates all relevant URLs with an empty diagnostic list.
    // This ensures that documents without diagnostics still get updated.
    if let Some(notebook) = query.as_notebook() {
        for url in notebook.urls() {
            diagnostics_map.entry(url.clone()).or_default();
        }
    } else {
        diagnostics_map
            .entry(query.make_key().into_url())
            .or_default();
    }

    let lsp_diagnostics = diagnostics.as_slice().iter().map(|diagnostic| {
        (
            // TODO: Use the cell index instead using `source_kind`
            usize::default(),
            to_lsp_diagnostic(db, diagnostic, snapshot.encoding()),
        )
    });

    if let Some(notebook) = query.as_notebook() {
        for (index, diagnostic) in lsp_diagnostics {
            let Some(uri) = notebook.cell_uri_by_index(index) else {
                tracing::warn!("Unable to find notebook cell at index {index}");
                continue;
            };
            diagnostics_map
                .entry(uri.clone())
                .or_default()
                .push(diagnostic);
        }
    } else {
        diagnostics_map
            .entry(query.make_key().into_url())
            .or_default()
            .extend(lsp_diagnostics.map(|(_, diagnostic)| diagnostic));
    }

    diagnostics_map
}

/// Converts the tool specific [`Diagnostic`][ruff_db::diagnostic::Diagnostic] to an LSP
/// [`Diagnostic`].
fn to_lsp_diagnostic(
    db: &dyn Db,
    diagnostic: &ruff_db::diagnostic::Diagnostic,
    encoding: PositionEncoding,
) -> Diagnostic {
    let range = if let Some(span) = diagnostic.primary_span() {
        let file = span.expect_ty_file();
        let index = line_index(db.upcast(), file);
        let source = source_text(db.upcast(), file);

        span.range()
            .map(|range| range.to_lsp_range(&source, &index, encoding))
            .unwrap_or_default()
    } else {
        Range::default()
    };

    let severity = match diagnostic.severity() {
        Severity::Info => DiagnosticSeverity::INFORMATION,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Error | Severity::Fatal => DiagnosticSeverity::ERROR,
    };

    let tags = diagnostic
        .primary_tags()
        .map(|tags| {
            tags.iter()
                .map(|tag| match tag {
                    ruff_db::diagnostic::DiagnosticTag::Unnecessary => DiagnosticTag::UNNECESSARY,
                    ruff_db::diagnostic::DiagnosticTag::Deprecated => DiagnosticTag::DEPRECATED,
                })
                .collect::<Vec<DiagnosticTag>>()
        })
        .filter(|mapped_tags| !mapped_tags.is_empty());

    let code_description = diagnostic
        .id()
        .is_lint()
        .then(|| {
            Some(CodeDescription {
                href: Url::parse(&format!("https://ty.dev/rules#{}", diagnostic.id())).ok()?,
            })
        })
        .flatten();

    let mut related_information = Vec::new();

    related_information.extend(
        diagnostic
            .secondary_annotations()
            .filter_map(|annotation| annotation_to_related_information(db, annotation, encoding)),
    );

    for sub_diagnostic in diagnostic.sub_diagnostics() {
        related_information.extend(sub_diagnostic_to_related_information(
            db,
            sub_diagnostic,
            encoding,
        ));

        related_information.extend(
            sub_diagnostic
                .annotations()
                .iter()
                .filter_map(|annotation| {
                    annotation_to_related_information(db, annotation, encoding)
                }),
        );
    }

    Diagnostic {
        range,
        severity: Some(severity),
        tags,
        code: Some(NumberOrString::String(diagnostic.id().to_string())),
        code_description,
        source: Some("ty".into()),
        message: diagnostic.concise_message().to_string(),
        related_information: Some(related_information),
        data: None,
    }
}

/// Converts an [`Annotation`] to a [`DiagnosticRelatedInformation`].
fn annotation_to_related_information(
    db: &dyn Db,
    annotation: &Annotation,
    encoding: PositionEncoding,
) -> Option<DiagnosticRelatedInformation> {
    let span = annotation.get_span();

    let annotation_message = annotation.get_message()?;
    let range = FileRange::try_from(span).ok()?;
    let location = range.to_location(db.upcast(), encoding)?;

    Some(DiagnosticRelatedInformation {
        location,
        message: annotation_message.to_string(),
    })
}

/// Converts a [`SubDiagnostic`] to a [`DiagnosticRelatedInformation`].
fn sub_diagnostic_to_related_information(
    db: &dyn Db,
    diagnostic: &SubDiagnostic,
    encoding: PositionEncoding,
) -> Option<DiagnosticRelatedInformation> {
    let primary_annotation = diagnostic.primary_annotation()?;

    let span = primary_annotation.get_span();
    let range = FileRange::try_from(span).ok()?;
    let location = range.to_location(db.upcast(), encoding)?;

    Some(DiagnosticRelatedInformation {
        location,
        message: diagnostic.concise_message().to_string(),
    })
}
