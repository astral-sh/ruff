use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::schedule::Task;
use crate::server::Result;
use crate::session::Session;
use crate::system::{url_to_any_system_path, AnySystemPath};
use lsp_types as types;
use lsp_types::{notification as notif, FileChangeType};
use rustc_hash::FxHashMap;
use ty_project::watch::{ChangeEvent, ChangedKind, CreatedKind, DeletedKind};
use ty_project::Db;

pub(crate) struct DidChangeWatchedFiles;

impl NotificationHandler for DidChangeWatchedFiles {
    type NotificationType = notif::DidChangeWatchedFiles;
}

impl SyncNotificationHandler for DidChangeWatchedFiles {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        requester: &mut Requester,
        params: types::DidChangeWatchedFilesParams,
    ) -> Result<()> {
        let mut events_by_db: FxHashMap<_, Vec<ChangeEvent>> = FxHashMap::default();

        for change in params.changes {
            let path = match url_to_any_system_path(&change.uri) {
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

        for (root, changes) in events_by_db {
            tracing::debug!("Applying changes to `{root}`");
            let db = session.project_db_for_path_mut(&*root).unwrap();

            db.apply_changes(changes, None);
        }

        let client_capabilities = session.client_capabilities();

        if client_capabilities.diagnostics_refresh {
            requester
                .request::<types::request::WorkspaceDiagnosticRefresh>((), |()| Task::nothing())
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        }

        if client_capabilities.inlay_refresh {
            requester
                .request::<types::request::InlayHintRefreshRequest>((), |()| Task::nothing())
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        }

        Ok(())
    }
}
