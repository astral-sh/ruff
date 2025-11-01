use std::hash::{DefaultHasher, Hash as _, Hasher as _};

use lsp_types::notification::PublishDiagnostics;
use lsp_types::{
    CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag,
    NumberOrString, PublishDiagnosticsParams, Url,
};
use rustc_hash::FxHashMap;

use ruff_db::diagnostic::{Annotation, Severity, SubDiagnostic};
use ruff_db::files::{File, FileRange};
use ruff_db::system::SystemPathBuf;
use ty_project::{Db as _, ProjectDatabase};

use crate::Db;
use crate::document::{FileRangeExt, ToRangeExt};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use crate::system::{AnySystemPath, file_to_url};
use crate::{PositionEncoding, Session};

// THe real challenge here is that `to_lsp_range` now needs to
// map to the cell document instead of the document. This is tricky.

pub(super) struct Diagnostics {
    items: Vec<ruff_db::diagnostic::Diagnostic>,
    encoding: PositionEncoding,
    file_or_notebook: File,
}

impl Diagnostics {
    /// Computes the result ID for `diagnostics`.
    ///
    /// Returns `None` if there are no diagnostics.
    pub(super) fn result_id_from_hash(
        diagnostics: &[ruff_db::diagnostic::Diagnostic],
    ) -> Option<String> {
        if diagnostics.is_empty() {
            return None;
        }

        // Generate result ID based on raw diagnostic content only
        let mut hasher = DefaultHasher::new();

        // Hash the length first to ensure different numbers of diagnostics produce different hashes
        diagnostics.hash(&mut hasher);

        Some(format!("{:016x}", hasher.finish()))
    }

    /// Computes the result ID for the diagnostics.
    ///
    /// Returns `None` if there are no diagnostics.
    pub(super) fn result_id(&self) -> Option<String> {
        Self::result_id_from_hash(&self.items)
    }

    pub(super) fn to_lsp_diagnostics(&self, db: &ProjectDatabase) -> LspDiagnostics {
        if let Some(notebook_document) = db.notebook_document(self.file_or_notebook) {
            let mut cell_diagnostics: FxHashMap<Url, Vec<Diagnostic>> = FxHashMap::default();

            // Populates all relevant URLs with an empty diagnostic list. This ensures that documents
            // without diagnostics still get updated.
            for cell_url in notebook_document.cell_urls() {
                cell_diagnostics.entry(cell_url.clone()).or_default();
            }

            for diagnostic in &self.items {
                let (url, lsp_diagnostic) = to_lsp_diagnostic(db, diagnostic, self.encoding);

                let Some(url) = url else {
                    tracing::warn!("Unable to find notebook cell");
                    continue;
                };

                cell_diagnostics
                    .entry(url)
                    .or_default()
                    .push(lsp_diagnostic);
            }

            LspDiagnostics::NotebookDocument(cell_diagnostics)
        } else {
            LspDiagnostics::TextDocument(
                self.items
                    .iter()
                    .map(|diagnostic| to_lsp_diagnostic(db, diagnostic, self.encoding).1)
                    .collect(),
            )
        }
    }
}

/// Represents the diagnostics for a text document or a notebook document.
pub(super) enum LspDiagnostics {
    TextDocument(Vec<Diagnostic>),

    /// A map of cell URLs to the diagnostics for that cell.
    NotebookDocument(FxHashMap<Url, Vec<Diagnostic>>),
}

impl LspDiagnostics {
    /// Returns the diagnostics for a text document.
    ///
    /// # Panics
    ///
    /// Panics if the diagnostics are for a notebook document.
    pub(super) fn expect_text_document(self) -> Vec<Diagnostic> {
        match self {
            LspDiagnostics::TextDocument(diagnostics) => diagnostics,
            LspDiagnostics::NotebookDocument(_) => {
                panic!("Expected a text document diagnostics, but got notebook diagnostics")
            }
        }
    }
}

pub(super) fn clear_diagnostics_if_needed(
    session: &Session,
    uri: &lsp_types::Url,
    client: &Client,
) {
    if session.client_capabilities().supports_pull_diagnostics() {
        return;
    }

    clear_diagnostics(uri, client);
}

/// Clears the diagnostics for the document identified by `uri`.
///
/// This is done by notifying the client with an empty list of diagnostics for the document.
/// For notebook cells, this clears diagnostics for the specific cell.
/// For other document types, this clears diagnostics for the main document.
pub(super) fn clear_diagnostics(uri: &lsp_types::Url, client: &Client) {
    client.send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: vec![],
        version: None,
    });
}

/// Publishes the diagnostics for the given document snapshot using the [publish diagnostics
/// notification].
///
/// This function is a no-op if the client supports pull diagnostics.
///
/// [publish diagnostics notification]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_publishDiagnostics
pub(super) fn publish_diagnostics(session: &Session, url: &lsp_types::Url, client: &Client) {
    let snapshot = match session.snapshot_document(url) {
        Ok(document) => document,
        Err(err) => {
            tracing::debug!("Failed to resolve document for URL `{}`: {}", url, err);
            return;
        }
    };

    if !snapshot.document().is_cell_or_notebook()
        && session.client_capabilities().supports_pull_diagnostics()
    {
        return;
    }

    let db = session.project_db(snapshot.notebook_or_file_path());

    let Some(diagnostics) = compute_diagnostics(db, &snapshot) else {
        return;
    };

    // Sends a notification to the client with the diagnostics for the document.
    let publish_diagnostics_notification = |uri: Url, diagnostics: Vec<Diagnostic>| {
        client.send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: Some(snapshot.document().version()),
        });
    };

    match diagnostics.to_lsp_diagnostics(db) {
        LspDiagnostics::TextDocument(diagnostics) => {
            publish_diagnostics_notification(url.clone(), diagnostics);
        }
        LspDiagnostics::NotebookDocument(cell_diagnostics) => {
            for (cell_url, diagnostics) in cell_diagnostics {
                publish_diagnostics_notification(cell_url, diagnostics);
            }
        }
    }
}

/// Publishes settings diagnostics for all the project at the given path
/// using the [publish diagnostics notification].
///
/// [publish diagnostics notification]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_publishDiagnostics
pub(crate) fn publish_settings_diagnostics(
    session: &mut Session,
    client: &Client,
    path: SystemPathBuf,
) {
    // Don't publish settings diagnostics for workspace that are already doing full diagnostics.
    //
    // Note we DO NOT respect the fact that clients support pulls because these are
    // files they *specifically* won't pull diagnostics from us for, because we don't
    // claim to be an LSP for them.
    if session.global_settings().diagnostic_mode().is_workspace() {
        return;
    }

    let session_encoding = session.position_encoding();
    let state = session.project_state_mut(&AnySystemPath::System(path));
    let db = &state.db;
    let project = db.project();
    let settings_diagnostics = project.check_settings(db);

    // We need to send diagnostics if we have non-empty ones, or we have ones to clear.
    // These will both almost always be empty so this function will almost always be a no-op.
    if settings_diagnostics.is_empty() && state.untracked_files_with_pushed_diagnostics.is_empty() {
        return;
    }

    // Group diagnostics by URL
    let mut diagnostics_by_url: FxHashMap<Url, Vec<_>> = FxHashMap::default();
    for diagnostic in settings_diagnostics {
        if let Some(span) = diagnostic.primary_span() {
            let file = span.expect_ty_file();
            let Some(url) = file_to_url(db, file) else {
                tracing::debug!("Failed to convert file to URL at {}", file.path(db));
                continue;
            };
            diagnostics_by_url.entry(url).or_default().push(diagnostic);
        }
    }

    // Record the URLs we're sending non-empty diagnostics for, so we know to clear them
    // the next time we publish settings diagnostics!
    let old_untracked = std::mem::replace(
        &mut state.untracked_files_with_pushed_diagnostics,
        diagnostics_by_url.keys().cloned().collect(),
    );

    // Add empty diagnostics for any files that had diagnostics before but don't now.
    // This will clear them (either the file is no longer relevant to us or fixed!)
    for url in old_untracked {
        diagnostics_by_url.entry(url).or_default();
    }
    // Send the settings diagnostics!
    for (url, file_diagnostics) in diagnostics_by_url {
        // Convert diagnostics to LSP format
        let lsp_diagnostics = file_diagnostics
            .into_iter()
            .map(|diagnostic| to_lsp_diagnostic(db, &diagnostic, session_encoding).1)
            .collect::<Vec<_>>();

        client.send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
            uri: url,
            diagnostics: lsp_diagnostics,
            version: None,
        });
    }
}

pub(super) fn compute_diagnostics(
    db: &ProjectDatabase,
    snapshot: &DocumentSnapshot,
) -> Option<Diagnostics> {
    let Some(file) = snapshot.to_notebook_or_file(db) else {
        tracing::info!(
            "No file found for snapshot for `{}`",
            snapshot.notebook_or_file_path()
        );
        return None;
    };

    let diagnostics = db.check_file(file);

    Some(Diagnostics {
        items: diagnostics,
        encoding: snapshot.encoding(),
        file_or_notebook: file,
    })
}

/// Converts the tool specific [`Diagnostic`][ruff_db::diagnostic::Diagnostic] to an LSP
/// [`Diagnostic`].
pub(super) fn to_lsp_diagnostic(
    db: &dyn Db,
    diagnostic: &ruff_db::diagnostic::Diagnostic,
    encoding: PositionEncoding,
) -> (Option<lsp_types::Url>, Diagnostic) {
    let location = diagnostic.primary_span().and_then(|span| {
        let file = span.expect_ty_file();
        span.range()?.as_lsp_range(db, file, encoding).to_location()
    });

    let (range, url) = match location {
        Some(location) => (location.range, Some(location.uri)),
        None => (lsp_types::Range::default(), None),
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

    (
        url,
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
        },
    )
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
    let location = range.as_lsp_range(db, encoding).to_location()?;

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
    let location = range.as_lsp_range(db, encoding).to_location()?;

    Some(DiagnosticRelatedInformation {
        location,
        message: diagnostic.concise_message().to_string(),
    })
}
