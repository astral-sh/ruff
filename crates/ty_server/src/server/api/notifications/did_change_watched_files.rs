use crate::document::DocumentKey;
use crate::server::Result;
use crate::server::api::diagnostics::{
    publish_diagnostics_if_needed, publish_settings_diagnostics,
};
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use lsp_types::FileChangeType;
use lsp_types::{self as types, DidChangeWatchedFilesNotification};
use ruff_db::system::SystemPathBuf;
use ty_project::Db as _;
use ty_project::watch::{ChangeEvent, ChangedKind, CreatedKind, DeletedKind};

pub(crate) struct DidChangeWatchedFiles;

impl NotificationHandler for DidChangeWatchedFiles {
    type NotificationType = DidChangeWatchedFilesNotification;
}

impl SyncNotificationHandler for DidChangeWatchedFiles {
    fn run(
        session: &mut Session,
        client: &Client,
        params: types::DidChangeWatchedFilesParams,
    ) -> Result<()> {
        let mut changes = Vec::new();

        for change in params.changes {
            let path = DocumentKey::from_url(&change.uri).into_file_path();

            let system_path = match path {
                AnySystemPath::System(system) => system,
                AnySystemPath::SystemVirtual(path) => {
                    tracing::debug!("Ignoring virtual path from change event: `{path}`");
                    continue;
                }
            };

            let change_event = match change.kind {
                FileChangeType::Created => ChangeEvent::Created {
                    path: system_path,
                    kind: CreatedKind::Any,
                },
                FileChangeType::Changed => ChangeEvent::Changed {
                    path: system_path,
                    kind: ChangedKind::Any,
                },
                FileChangeType::Deleted => ChangeEvent::Deleted {
                    path: system_path,
                    kind: DeletedKind::Any,
                },
            };

            changes.push(change_event);
        }

        if changes.is_empty() {
            return Ok(());
        }

        let roots: Vec<SystemPathBuf> = session
            .project_dbs()
            .map(|db| db.project().root(db).to_owned())
            .collect();

        for root in roots {
            tracing::debug!("Applying changes to `{root}`");

            session.apply_changes(&AnySystemPath::System(root.clone()), &changes);
            publish_settings_diagnostics(session, client, root);
        }

        let client_capabilities = session.client_capabilities();

        if client_capabilities.supports_workspace_diagnostic_refresh() {
            client.send_request::<types::DiagnosticRefreshRequest>(session, (), |_, ()| {});
        } else {
            for key in session.text_document_handles() {
                publish_diagnostics_if_needed(&key, session, client);
            }
        }

        if client_capabilities.supports_inlay_hint_refresh() {
            client.send_request::<types::InlayHintRefreshRequest>(session, (), |_, ()| {});
        }

        Ok(())
    }
}
