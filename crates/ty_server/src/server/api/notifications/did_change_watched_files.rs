use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::publish_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use lsp_types as types;
use lsp_types::{FileChangeType, notification as notif};
use rustc_hash::FxHashMap;
use ty_project::Db;
use ty_project::watch::{ChangeEvent, ChangedKind, CreatedKind, DeletedKind};

pub(crate) struct DidChangeWatchedFiles;

impl NotificationHandler for DidChangeWatchedFiles {
    type NotificationType = notif::DidChangeWatchedFiles;
}

impl SyncNotificationHandler for DidChangeWatchedFiles {
    fn run(
        session: &mut Session,
        client: &Client,
        params: types::DidChangeWatchedFilesParams,
    ) -> Result<()> {
        let mut events_by_db: FxHashMap<_, Vec<ChangeEvent>> = FxHashMap::default();

        for change in params.changes {
            let path = match AnySystemPath::try_from_url(&change.uri) {
                Ok(path) => path,
                Err(err) => {
                    tracing::warn!(
                        "Failed to convert URI '{}` to system path: {err:?}",
                        change.uri
                    );
                    continue;
                }
            };

            let system_path = match path {
                AnySystemPath::System(system) => system,
                AnySystemPath::SystemVirtual(path) => {
                    tracing::debug!("Ignoring virtual path from change event: `{path}`");
                    continue;
                }
            };

            let Some(db) = session.project_db_for_path(system_path.as_std_path()) else {
                tracing::trace!(
                    "Ignoring change event for `{system_path}` because it's not in any workspace"
                );
                continue;
            };

            let change_event = match change.typ {
                FileChangeType::CREATED => ChangeEvent::Created {
                    path: system_path,
                    kind: CreatedKind::Any,
                },
                FileChangeType::CHANGED => ChangeEvent::Changed {
                    path: system_path,
                    kind: ChangedKind::Any,
                },
                FileChangeType::DELETED => ChangeEvent::Deleted {
                    path: system_path,
                    kind: DeletedKind::Any,
                },
                _ => {
                    tracing::debug!(
                        "Ignoring unsupported change event type: `{:?}` for {system_path}",
                        change.typ
                    );
                    continue;
                }
            };

            events_by_db
                .entry(db.project().root(db).to_path_buf())
                .or_default()
                .push(change_event);
        }

        if events_by_db.is_empty() {
            return Ok(());
        }

        let mut project_changed = false;

        for (root, changes) in events_by_db {
            tracing::debug!("Applying changes to `{root}`");

            // SAFETY: Only paths that are part of the workspace are registered for file watching.
            // So, virtual paths and paths that are outside of a workspace does not trigger this
            // notification.
            let db = session.project_db_for_path_mut(&*root).unwrap();

            let result = db.apply_changes(changes, None);

            project_changed |= result.project_changed();
        }

        let client_capabilities = session.client_capabilities();

        if project_changed {
            if client_capabilities.diagnostics_refresh {
                client
                    .send_request::<types::request::WorkspaceDiagnosticRefresh>(
                        session,
                        (),
                        |_, ()| {},
                    )
                    .with_failure_code(lsp_server::ErrorCode::InternalError)?;
            } else {
                for key in session.text_document_keys() {
                    publish_diagnostics(session, &key, client)?;
                }
            }

            // TODO: always publish diagnostics for notebook files (since they don't use pull diagnostics)
        }

        if client_capabilities.inlay_refresh {
            client
                .send_request::<types::request::InlayHintRefreshRequest>(session, (), |_, ()| {})
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        }

        Ok(())
    }
}
