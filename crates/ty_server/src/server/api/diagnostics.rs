use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash as _, Hasher as _};

use lsp_types::notification::PublishDiagnostics;
use lsp_types::{
    CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag,
    NumberOrString, PublishDiagnosticsParams, Url,
};
use ruff_diagnostics::Applicability;
use ruff_text_size::Ranged;
use rustc_hash::FxHashMap;

use ruff_db::diagnostic::{Annotation, Severity, SubDiagnostic};
use ruff_db::files::{File, FileRange};
use ruff_db::system::SystemPathBuf;
use serde::{Deserialize, Serialize};
use ty_project::{Db as _, ProjectDatabase};

use crate::capabilities::ResolvedClientCapabilities;
use crate::document::{FileRangeExt, ToRangeExt};
use crate::session::client::Client;
use crate::session::{DocumentHandle, GlobalSettings};
use crate::system::{AnySystemPath, file_to_url};
use crate::{DIAGNOSTIC_NAME, Db, DiagnosticMode};
use crate::{PositionEncoding, Session};

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

    pub(super) fn to_lsp_diagnostics(
        &self,
        db: &ProjectDatabase,
        client_capabilities: ResolvedClientCapabilities,
        global_settings: &GlobalSettings,
    ) -> LspDiagnostics {
        if let Some(notebook_document) = db.notebook_document(self.file_or_notebook) {
            let mut cell_diagnostics: FxHashMap<Url, Vec<Diagnostic>> = FxHashMap::default();

            // Populates all relevant URLs with an empty diagnostic list. This ensures that documents
            // without diagnostics still get updated.
            for cell_url in notebook_document.cell_urls() {
                cell_diagnostics.entry(cell_url.clone()).or_default();
            }

            for diagnostic in &self.items {
                let Some((url, lsp_diagnostic)) = to_lsp_diagnostic(
                    db,
                    diagnostic,
                    self.encoding,
                    client_capabilities,
                    global_settings,
                ) else {
                    continue;
                };

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
                    .filter_map(|diagnostic| {
                        Some(
                            to_lsp_diagnostic(
                                db,
                                diagnostic,
                                self.encoding,
                                client_capabilities,
                                global_settings,
                            )?
                            .1,
                        )
                    })
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
    document: &DocumentHandle,
    session: &Session,
    client: &Client,
) {
    if session.client_capabilities().supports_pull_diagnostics() && !document.is_cell_or_notebook()
    {
        return;
    }

    clear_diagnostics(document.url(), session, client);
}

/// Clears the diagnostics for the document identified by `uri`.
///
/// This is done by notifying the client with an empty list of diagnostics for the document.
/// For notebook cells, this clears diagnostics for the specific cell.
/// For other document types, this clears diagnostics for the main document.
pub(super) fn clear_diagnostics(uri: &lsp_types::Url, session: &Session, client: &Client) {
    if session.global_settings().diagnostic_mode().is_off() {
        return;
    }

    client.send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics: vec![],
        version: None,
    });
}

/// Publishes the diagnostics for the given document snapshot using the [publish diagnostics
/// notification] .
///
/// Unlike [`publish_diagnostics`], this function only publishes diagnostics if a client doesn't support
/// pull diagnostics and `document` is not a notebook or cell (VS Code
/// does not support pull diagnostics for notebooks or cells (as of 2025-11-12).
///
/// [publish diagnostics notification]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_publishDiagnostics
pub(super) fn publish_diagnostics_if_needed(
    document: &DocumentHandle,
    session: &Session,
    client: &Client,
) {
    if !document.is_cell_or_notebook() && session.client_capabilities().supports_pull_diagnostics()
    {
        return;
    }

    publish_diagnostics(document, session, client);
}

/// Publishes the diagnostics for the given document snapshot using the [publish diagnostics
/// notification].
pub(super) fn publish_diagnostics(document: &DocumentHandle, session: &Session, client: &Client) {
    if session.global_settings().diagnostic_mode().is_off() {
        return;
    }

    let db = session.project_db(document.notebook_or_file_path());

    let Some(diagnostics) = compute_diagnostics(db, document, session.position_encoding()) else {
        return;
    };

    // Sends a notification to the client with the diagnostics for the document.
    let publish_diagnostics_notification = |uri: Url, diagnostics: Vec<Diagnostic>| {
        client.send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: Some(document.version()),
        });
    };

    match diagnostics.to_lsp_diagnostics(
        db,
        session.client_capabilities(),
        session.global_settings(),
    ) {
        LspDiagnostics::TextDocument(diagnostics) => {
            publish_diagnostics_notification(document.url().clone(), diagnostics);
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
    match session.global_settings().diagnostic_mode() {
        DiagnosticMode::Workspace | DiagnosticMode::Off => {
            return;
        }
        DiagnosticMode::OpenFilesOnly => {}
    }

    let session_encoding = session.position_encoding();
    let client_capabilities = session.client_capabilities();

    let project_path = AnySystemPath::System(path);

    let (mut diagnostics_by_url, old_untracked) = {
        let state = session.project_state_mut(&project_path);
        let db = &state.db;
        let project = db.project();
        let settings_diagnostics = project.check_settings(db);

        // We need to send diagnostics if we have non-empty ones, or we have ones to clear.
        // These will both almost always be empty so this function will almost always be a no-op.
        if settings_diagnostics.is_empty()
            && state.untracked_files_with_pushed_diagnostics.is_empty()
        {
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

        (diagnostics_by_url, old_untracked)
    };

    // Add empty diagnostics for any files that had diagnostics before but don't now.
    // This will clear them (either the file is no longer relevant to us or fixed!)
    for url in old_untracked {
        diagnostics_by_url.entry(url).or_default();
    }

    let db = session.project_db(&project_path);
    let global_settings = session.global_settings();

    // Send the settings diagnostics!
    for (url, file_diagnostics) in diagnostics_by_url {
        // Convert diagnostics to LSP format
        let lsp_diagnostics = file_diagnostics
            .into_iter()
            .filter_map(|diagnostic| {
                Some(
                    to_lsp_diagnostic(
                        db,
                        &diagnostic,
                        session_encoding,
                        client_capabilities,
                        global_settings,
                    )?
                    .1,
                )
            })
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
    document: &DocumentHandle,
    encoding: PositionEncoding,
) -> Option<Diagnostics> {
    let Some(file) = document.notebook_or_file(db) else {
        tracing::info!(
            "No file found for snapshot for `{}`",
            document.notebook_or_file_path()
        );
        return None;
    };

    let diagnostics = db.check_file(file);

    Some(Diagnostics {
        items: diagnostics,
        encoding,
        file_or_notebook: file,
    })
}

/// Converts the tool specific [`Diagnostic`][ruff_db::diagnostic::Diagnostic] to an LSP
/// [`Diagnostic`].
pub(super) fn to_lsp_diagnostic(
    db: &dyn Db,
    diagnostic: &ruff_db::diagnostic::Diagnostic,
    encoding: PositionEncoding,
    client_capabilities: ResolvedClientCapabilities,
    global_settings: &GlobalSettings,
) -> Option<(Option<lsp_types::Url>, Diagnostic)> {
    if diagnostic.is_invalid_syntax() && !global_settings.show_syntax_errors() {
        return None;
    }

    let supports_related_information =
        client_capabilities.supports_diagnostic_related_information();

    let location = diagnostic.primary_span().and_then(|span| {
        let file = span.expect_ty_file();
        span.range()?
            .to_lsp_range(db, file, encoding)
            .unwrap_or_default()
            .to_location()
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

    let code_description = diagnostic.documentation_url().and_then(|url| {
        let href = Url::parse(url).ok()?;

        Some(CodeDescription { href })
    });

    let related_information =
        if supports_related_information {
            let mut related_information = Vec::new();
            related_information.extend(diagnostic.secondary_annotations().filter_map(
                |annotation| annotation_to_related_information(db, annotation, encoding),
            ));

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
                        .filter(|annotation| !annotation.is_primary())
                        .filter_map(|annotation| {
                            annotation_to_related_information(db, annotation, encoding)
                        }),
                );
            }

            Some(related_information)
        } else {
            None
        };

    let data = DiagnosticData::try_from_diagnostic(db, diagnostic, encoding);

    Some((
        url,
        Diagnostic {
            range,
            severity: Some(severity),
            tags,
            code: Some(NumberOrString::String(diagnostic.id().to_string())),
            code_description,
            source: Some(DIAGNOSTIC_NAME.into()),
            message: if supports_related_information {
                // Show both the primary and annotation messages if available,
                // because we don't create a related information for the primary message.
                if let Some(annotation_message) = diagnostic
                    .primary_annotation()
                    .and_then(|annotation| annotation.get_message())
                {
                    format!("{}: {annotation_message}", diagnostic.primary_message())
                } else {
                    diagnostic.primary_message().to_string()
                }
            } else {
                diagnostic.concise_message().to_string()
            },
            related_information,
            data: serde_json::to_value(data).ok(),
        },
    ))
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
    let location = range.to_lsp_range(db, encoding)?.into_location()?;

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
    let location = range.to_lsp_range(db, encoding)?.into_location()?;

    Some(DiagnosticRelatedInformation {
        location,
        message: diagnostic.concise_message().to_string(),
    })
}

#[derive(Serialize, Deserialize)]
pub(crate) struct DiagnosticData {
    pub(crate) fix_title: String,
    pub(crate) edits: HashMap<Url, Vec<lsp_types::TextEdit>>,
}

impl DiagnosticData {
    fn try_from_diagnostic(
        db: &dyn Db,
        diagnostic: &ruff_db::diagnostic::Diagnostic,
        encoding: PositionEncoding,
    ) -> Option<Self> {
        let fix = diagnostic
            .fix()
            .filter(|fix| fix.applies(Applicability::Unsafe))?;

        let primary_span = diagnostic.primary_span()?;
        let file = primary_span.expect_ty_file();

        let mut lsp_edits: HashMap<Url, Vec<lsp_types::TextEdit>> = HashMap::new();

        for edit in fix.edits() {
            let location = edit
                .range()
                .to_lsp_range(db, file, encoding)?
                .to_location()?;

            lsp_edits
                .entry(location.uri)
                .or_default()
                .push(lsp_types::TextEdit {
                    range: location.range,
                    new_text: edit.content().unwrap_or_default().to_string(),
                });
        }

        Some(Self {
            fix_title: diagnostic
                .first_help_text()
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("Fix {}", diagnostic.id())),
            edits: lsp_edits,
        })
    }
}
