use std::collections::HashMap;
use std::fmt::Write as _;
use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::panic::AssertUnwindSafe;

use lsp_types::{Code, PublishDiagnosticsNotification};
use lsp_types::{
    CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag,
    Message, PublishDiagnosticsParams, Uri,
};
use ruff_diagnostics::Applicability;
use ruff_text_size::Ranged;
use rustc_hash::{FxHashMap, FxHashSet};
use ty_ide::{Hint, hints};

use ruff_db::diagnostic::{
    Annotation, DisplayDiagnosticConfig, HyperlinkMode, Severity, SubDiagnostic,
};
use ruff_db::files::{File, FileRange};
use ruff_db::source::source_text;
use ruff_db::system::SystemPathBuf;
use serde::{Deserialize, Serialize};
use ty_project::{Db as _, ProjectDatabase};

use crate::capabilities::ResolvedClientCapabilities;
use crate::document::{FileRangeExt, ToRangeExt};
use crate::server::Action;
use crate::server::schedule::{BackgroundSchedule, Task};
use crate::session::client::Client;
use crate::session::{DocumentHandle, GlobalSettings};
use crate::system::{AnySystemPath, file_to_uri};
use crate::{DIAGNOSTIC_NAME, Db};
use crate::{PositionEncoding, Session};

#[derive(Debug)]
pub(super) struct Diagnostics {
    items: Vec<ruff_db::diagnostic::Diagnostic>,
    unnecessary_hints: Vec<Hint>,
    encoding: PositionEncoding,
    file_or_notebook: File,
}

impl Diagnostics {
    /// Computes the result ID for `diagnostics`.
    ///
    /// The result ID is `None` if there are no diagnostics or hints.
    pub(super) fn result_id_from_hash(
        db: &dyn Db,
        diagnostics: &[ruff_db::diagnostic::Diagnostic],
        unnecessary_hints: &[Hint],
        client_capabilities: ResolvedClientCapabilities,
    ) -> Option<String> {
        if diagnostics.is_empty() && unnecessary_hints.is_empty() {
            return None;
        }

        // Generate the base result ID from raw diagnostic content.
        let mut hasher = DefaultHasher::new();

        diagnostics.hash(&mut hasher);
        unnecessary_hints.hash(&mut hasher);

        if client_capabilities.supports_full_diagnostic_output() {
            // The rendered output includes source snippets that aren't part of the raw diagnostic.
            // Hash each referenced file's source once so that source-only changes invalidate the
            // result without eagerly rendering every diagnostic.
            // TODO: Hash only the source snippets used by the rendered output. Hashing the entire
            // file is deliberately conservative: an edit outside the rendered context can cause a
            // full report, but an edit inside it can never leave stale rendered output on the client.
            let mut hashed_files = FxHashSet::default();

            for diagnostic in diagnostics {
                let annotations = diagnostic
                    .sub_diagnostics()
                    .iter()
                    .flat_map(SubDiagnostic::annotations)
                    .chain(diagnostic.annotations());

                for annotation in annotations {
                    let file = annotation.get_span().expect_ty_file();
                    if hashed_files.insert(file) {
                        source_text(db, file).as_str().hash(&mut hasher);
                    }
                }
            }
        }

        Some(format!("{:016x}", hasher.finish()))
    }

    /// Computes the result ID for the diagnostics.
    ///
    /// The result ID is `None` if there are no diagnostics or hints.
    pub(super) fn result_id(
        &self,
        db: &dyn Db,
        client_capabilities: ResolvedClientCapabilities,
    ) -> Option<String> {
        Self::result_id_from_hash(
            db,
            &self.items,
            &self.unnecessary_hints,
            client_capabilities,
        )
    }

    pub(super) fn to_lsp_diagnostics(
        &self,
        db: &ProjectDatabase,
        client_capabilities: ResolvedClientCapabilities,
        global_settings: &GlobalSettings,
    ) -> LspDiagnostics {
        if let Some(notebook_document) = db.notebook_document(self.file_or_notebook) {
            let mut cell_diagnostics: FxHashMap<Uri, Vec<Diagnostic>> = FxHashMap::default();

            // Populates all relevant URIs with an empty diagnostic list. This ensures that documents
            // without diagnostics still get updated.
            for cell_uri in notebook_document.cell_uris() {
                cell_diagnostics.entry(cell_uri.clone()).or_default();
            }

            for diagnostic in &self.items {
                let Some((uri, lsp_diagnostic)) = to_lsp_diagnostic(
                    db,
                    diagnostic,
                    self.encoding,
                    client_capabilities,
                    global_settings,
                ) else {
                    continue;
                };

                let Some(uri) = uri else {
                    tracing::warn!("Unable to find notebook cell");
                    continue;
                };

                cell_diagnostics
                    .entry(uri)
                    .or_default()
                    .push(lsp_diagnostic);
            }

            for hint in &self.unnecessary_hints {
                let Some((uri, lsp_diagnostic)) = unnecessary_hint_to_lsp_diagnostic(
                    db,
                    self.file_or_notebook,
                    self.encoding,
                    hint,
                ) else {
                    continue;
                };

                let Some(uri) = uri else {
                    tracing::warn!("Unable to find notebook cell");
                    continue;
                };

                cell_diagnostics
                    .entry(uri)
                    .or_default()
                    .push(lsp_diagnostic);
            }

            LspDiagnostics::NotebookDocument(cell_diagnostics)
        } else {
            let mut diagnostics = self
                .items
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
                .collect::<Vec<_>>();
            diagnostics.extend(unnecessary_hints_to_lsp_diagnostics(
                db,
                self.file_or_notebook,
                self.encoding,
                &self.unnecessary_hints,
            ));
            LspDiagnostics::TextDocument(diagnostics)
        }
    }
}

/// Represents the diagnostics for a text document or a notebook document.
pub(super) enum LspDiagnostics {
    TextDocument(Vec<Diagnostic>),

    /// A map of cell URIs to the diagnostics for that cell.
    NotebookDocument(FxHashMap<Uri, Vec<Diagnostic>>),
}

impl LspDiagnostics {
    /// Returns the diagnostics for the text document or notebook cell at `uri`.
    pub(super) fn into_document_diagnostics(self, uri: &Uri) -> Vec<Diagnostic> {
        match self {
            LspDiagnostics::TextDocument(diagnostics) => diagnostics,
            LspDiagnostics::NotebookDocument(mut diagnostics) => {
                diagnostics.remove(uri).unwrap_or_default()
            }
        }
    }
}

/// Publishes diagnostics for all open files that need push diagnostics.
pub(crate) fn publish_all_document_diagnostics(session: &Session, client: &Client) {
    for document in session.file_document_handles() {
        publish_diagnostics_if_needed(&document, session, client);
    }
}

/// Publishes the diagnostics for the given document snapshot using the [publish diagnostics
/// notification] .
///
/// Unlike [`publish_diagnostics`], this function only publishes diagnostics if a client doesn't support
/// pull diagnostics and `document` is not a notebook or cell (VS Code
/// does not support pull diagnostics for notebooks or cells (as of 2025-11-12).
///
/// [publish diagnostics notification]: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_publishDiagnostics
pub(crate) fn publish_diagnostics_if_needed(
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
    if session.global_settings().diagnostic_mode().is_off()
        || session.has_project_diagnostics(document.uri())
    {
        return;
    }

    let db = session.project_db(document.notebook_or_file_path());

    let Some(diagnostics) = compute_diagnostics(db, document, session.position_encoding()) else {
        return;
    };

    // Sends a notification to the client with the diagnostics for the document.
    let publish_diagnostics_notification =
        |uri: Uri, version: Option<i32>, diagnostics: Vec<Diagnostic>| {
            client.send_notification::<PublishDiagnosticsNotification>(PublishDiagnosticsParams {
                uri,
                diagnostics,
                version,
            });
        };

    match diagnostics.to_lsp_diagnostics(
        db,
        session.client_capabilities(),
        session.global_settings(),
    ) {
        LspDiagnostics::TextDocument(diagnostics) => {
            publish_diagnostics_notification(
                document.uri().clone(),
                Some(document.version()),
                diagnostics,
            );
        }
        LspDiagnostics::NotebookDocument(cell_diagnostics) => {
            #[expect(
                clippy::iter_over_hash_type,
                reason = "diagnostic notifications for distinct cell URIs are independent"
            )]
            for (cell_uri, diagnostics) in cell_diagnostics {
                let version = session
                    .document_handle(&cell_uri)
                    .map(|document| document.version())
                    .ok();
                publish_diagnostics_notification(cell_uri, version, diagnostics);
            }
        }
    }
}

/// Computes diagnostics for files that clients do not request document diagnostics for.
///
/// Dependency diagnostics can change when any Python file changes, even when the manifest
/// is closed. Their project-wide import inventory must be computed on a background worker.
pub(in crate::server) fn project_diagnostics_task() -> Task {
    Task::background(BackgroundSchedule::Worker, |session| {
        let revision = session.revision();
        let encoding = session.position_encoding();
        let capabilities = session.client_capabilities();
        let settings = session.global_settings().clone();
        // The snapshots are discarded on unwinding and never reused afterward.
        let projects = AssertUnwindSafe(session.project_snapshots());

        Box::new(move |client| {
            let result = ruff_db::panic::catch_unwind(|| {
                let projects = projects;
                let mut project_diagnostics = Vec::with_capacity(projects.len());
                // Drop each completed snapshot before checking the next project. An edit cancels
                // projects in this order and would otherwise wait for an earlier snapshot.
                for (root, db) in projects.0 {
                    let mut diagnostics_by_uri: FxHashMap<Uri, Vec<Diagnostic>> =
                        FxHashMap::default();

                    // Workspace diagnostics already include these diagnostics. Empty results also
                    // clear previously pushed diagnostics when the diagnostic mode changes.
                    if settings.diagnostic_mode().is_open_files_only() {
                        let diagnostics = db.project().check_settings(&db);
                        for diagnostic in diagnostics
                            .iter()
                            .chain(ty_project::dependency::project_dependency_diagnostics(&db))
                        {
                            let Some(span) = diagnostic.primary_span() else {
                                continue;
                            };
                            let Some(uri) = file_to_uri(&db, span.expect_ty_file()) else {
                                continue;
                            };
                            if let Some((_, diagnostic)) = to_lsp_diagnostic(
                                &db,
                                diagnostic,
                                encoding,
                                capabilities,
                                &settings,
                            ) {
                                diagnostics_by_uri.entry(uri).or_default().push(diagnostic);
                            }
                        }
                    }

                    project_diagnostics.push((root, diagnostics_by_uri));
                }
                ProjectDiagnostics {
                    revision,
                    projects: project_diagnostics,
                }
            });
            // All snapshots are now released, including on cancellation. Sending an action can
            // block on the bounded main-loop queue, so it must not retain a database snapshot.

            let result = match result {
                Ok(diagnostics) => ProjectDiagnosticsResult::Completed(diagnostics),
                Err(error) if error.payload.downcast_ref::<salsa::Cancelled>().is_some() => {
                    ProjectDiagnosticsResult::Cancelled
                }
                Err(error) => {
                    tracing::error!("Failed to compute project diagnostics: {error}");
                    ProjectDiagnosticsResult::Failed
                }
            };
            client.queue_action(Action::ProjectDiagnosticsFinished(result));
        })
    })
}

#[derive(Debug)]
pub(crate) enum ProjectDiagnosticsResult {
    Completed(ProjectDiagnostics),
    Cancelled,
    Failed,
}

#[derive(Debug)]
pub(crate) struct ProjectDiagnostics {
    revision: u64,
    projects: Vec<(SystemPathBuf, FxHashMap<Uri, Vec<Diagnostic>>)>,
}

impl ProjectDiagnostics {
    pub(crate) fn publish(self, session: &mut Session, client: &Client) {
        // Conversion runs in the background, so source or settings may have changed since then.
        if self.revision != session.revision() {
            return;
        }

        for (root, diagnostics_by_uri) in self.projects {
            let state = session.project_state_mut(&AnySystemPath::System(root));
            let mut previous =
                std::mem::replace(&mut state.pushed_project_diagnostics, diagnostics_by_uri);

            #[expect(
                clippy::iter_over_hash_type,
                reason = "diagnostic notifications for distinct document URIs are independent"
            )]
            for (uri, diagnostics) in &state.pushed_project_diagnostics {
                if previous.remove(uri).as_ref() == Some(diagnostics) {
                    continue;
                }
                client.send_notification::<PublishDiagnosticsNotification>(
                    PublishDiagnosticsParams {
                        uri: uri.clone(),
                        diagnostics: diagnostics.clone(),
                        version: None,
                    },
                );
            }

            for uri in previous.into_keys() {
                client.send_notification::<PublishDiagnosticsNotification>(
                    PublishDiagnosticsParams {
                        uri,
                        diagnostics: Vec::new(),
                        version: None,
                    },
                );
            }
        }
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

    // The first uv result supplies the module paths needed for correct diagnostics. Do not analyze
    // the script until that result is available. Waiting would not help: publishing the environment
    // advances the database revision and cancels this snapshot, so the request must retry anyway.
    if db.uv_environments().is_initialization_pending(db, file) {
        return None;
    }

    let diagnostics = db.check_file(file);
    let unnecessary_hints = hints(db, file);

    Some(Diagnostics {
        items: diagnostics,
        unnecessary_hints,
        encoding,
        file_or_notebook: file,
    })
}

pub(super) fn unnecessary_hints_to_lsp_diagnostics(
    db: &ProjectDatabase,
    file: File,
    encoding: PositionEncoding,
    hints: &[Hint],
) -> Vec<Diagnostic> {
    hints
        .iter()
        .filter_map(|hint| unnecessary_hint_to_lsp_diagnostic(db, file, encoding, hint))
        .map(|(_, diagnostic)| diagnostic)
        .collect()
}

fn unnecessary_hint_to_lsp_diagnostic(
    db: &ProjectDatabase,
    file: File,
    encoding: PositionEncoding,
    hint: &Hint,
) -> Option<(Option<Uri>, Diagnostic)> {
    let range = hint.range.to_lsp_range(db, file, encoding)?;
    let uri = range.to_location().map(|location| location.uri);

    Some((
        uri,
        Diagnostic {
            range: range.local_range(),
            severity: Some(DiagnosticSeverity::Hint),
            code: None,
            code_description: None,
            source: Some(DIAGNOSTIC_NAME.into()),
            message: Message::String(hint.message()),
            related_information: None,
            tags: Some(vec![DiagnosticTag::Unnecessary]),
            data: None,
        },
    ))
}

/// Converts the tool specific [`Diagnostic`][ruff_db::diagnostic::Diagnostic] to an LSP
/// [`Diagnostic`].
pub(super) fn to_lsp_diagnostic(
    db: &dyn Db,
    diagnostic: &ruff_db::diagnostic::Diagnostic,
    encoding: PositionEncoding,
    client_capabilities: ResolvedClientCapabilities,
    global_settings: &GlobalSettings,
) -> Option<(Option<lsp_types::Uri>, Diagnostic)> {
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

    let (range, uri) = match location {
        Some(location) => (location.range, Some(location.uri)),
        None => (lsp_types::Range::default(), None),
    };

    let severity = match diagnostic.severity() {
        Severity::Info => DiagnosticSeverity::Information,
        Severity::Warning => DiagnosticSeverity::Warning,
        Severity::Error | Severity::Fatal => DiagnosticSeverity::Error,
    };

    let tags = diagnostic
        .primary_tags()
        .map(|tags| {
            tags.iter()
                .map(|tag| match tag {
                    ruff_db::diagnostic::DiagnosticTag::Unnecessary => DiagnosticTag::Unnecessary,
                    ruff_db::diagnostic::DiagnosticTag::Deprecated => DiagnosticTag::Deprecated,
                })
                .collect::<Vec<DiagnosticTag>>()
        })
        .filter(|mapped_tags| !mapped_tags.is_empty());

    let code_description = diagnostic.documentation_url().and_then(|url| {
        let href = Uri::parse(url).ok()?;

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

                related_information.extend(sub_diagnostic.secondary_annotations().filter_map(
                    |annotation| annotation_to_related_information(db, annotation, encoding),
                ));
            }

            Some(related_information)
        } else {
            None
        };

    let data = DiagnosticData::try_from_diagnostic(
        db,
        diagnostic,
        encoding,
        FullDiagnosticOutput::from_client_capabilities(client_capabilities),
    );

    let mut message = if supports_related_information {
        // Show both the primary and annotation messages if available,
        // because we don't create a related information for the primary message.
        if let Some(annotation_message) = diagnostic
            .primary_annotation()
            .and_then(|annotation| annotation.get_message())
        {
            format!("{}: {annotation_message}", diagnostic.headline_message())
        } else {
            diagnostic.headline_message().to_string()
        }
    } else {
        diagnostic.concise_message().to_string()
    };

    // Append info sub-diagnostics that have no location (and thus
    // can't be shown as "related information") to the message.
    let mut first = true;
    for sub_diagnostic in diagnostic.sub_diagnostics() {
        if sub_diagnostic.primary_annotation().is_none() {
            if first {
                message.push('\n');
                first = false;
            }
            write!(
                message,
                "\n{severity}: {hint}",
                hint = sub_diagnostic.concise_message(),
                severity = sub_diagnostic.severity()
            )
            .ok();
        }
    }

    Some((
        uri,
        Diagnostic {
            range,
            severity: Some(severity),
            tags,
            code: Some(Code::String(diagnostic.id().to_string())),
            code_description,
            source: Some(DIAGNOSTIC_NAME.into()),
            message: Message::String(message),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FullDiagnosticOutput {
    Enabled,
    Disabled,
}

impl FullDiagnosticOutput {
    fn from_client_capabilities(client_capabilities: ResolvedClientCapabilities) -> Self {
        if client_capabilities.supports_full_diagnostic_output() {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct FullDiagnosticData {
    rendered: String,
    pub(crate) diagnostic_id: String,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub(crate) fix: Option<DiagnosticFixData>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct DiagnosticFixData {
    pub(crate) fix_title: String,
    pub(crate) edits: HashMap<Uri, Vec<lsp_types::TextEdit>>,
    pub(crate) preferred: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum DiagnosticData {
    Full(FullDiagnosticData),
    Fix(DiagnosticFixData),
}

impl DiagnosticData {
    fn try_from_diagnostic(
        db: &dyn Db,
        diagnostic: &ruff_db::diagnostic::Diagnostic,
        encoding: PositionEncoding,
        full_diagnostic_output: FullDiagnosticOutput,
    ) -> Option<Self> {
        let fix = Self::try_fix_from_diagnostic(db, diagnostic, encoding);

        match full_diagnostic_output {
            FullDiagnosticOutput::Enabled => Some(Self::Full(FullDiagnosticData {
                rendered: diagnostic
                    .display(
                        &(db as &dyn ruff_db::Db),
                        &DisplayDiagnosticConfig::new("ty")
                            .color(true)
                            // The styled renderer can enable OSC-8 hyperlinks based on the process
                            // environment, even though this output is sent over LSP rather than to a
                            // terminal. The ANSI parser used by ty-vscode does not strip OSC-8
                            // sequences, so their escape codes would appear in the virtual diagnostic
                            // document.
                            .hyperlinks(HyperlinkMode::Never),
                    )
                    .to_string(),
                diagnostic_id: diagnostic.id().to_string(),
                fix,
            })),
            FullDiagnosticOutput::Disabled => fix.map(Self::Fix),
        }
    }

    fn try_fix_from_diagnostic(
        db: &dyn Db,
        diagnostic: &ruff_db::diagnostic::Diagnostic,
        encoding: PositionEncoding,
    ) -> Option<DiagnosticFixData> {
        let fix = diagnostic
            .fix()
            .filter(|fix| fix.applies(Applicability::Unsafe))?;

        let primary_span = diagnostic.primary_span()?;
        let file = primary_span.expect_ty_file();

        let mut lsp_edits: HashMap<Uri, Vec<lsp_types::TextEdit>> = HashMap::new();

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

        Some(DiagnosticFixData {
            fix_title: diagnostic
                .first_help_text()
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("Fix {}", diagnostic.id())),
            edits: lsp_edits,
            preferred: fix.applies(Applicability::Safe),
        })
    }
}
