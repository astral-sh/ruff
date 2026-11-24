use crate::document::DocumentKey;
use crate::server::Result;
use crate::server::api::diagnostics::{
    publish_all_document_diagnostics, publish_settings_diagnostics,
};
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use lsp_types::FileChangeType;
use lsp_types::{self as types, DidChangeWatchedFilesNotification};
use ty_project::watch::{ChangeEvent, ChangedKind, CreatedKind, DeletedKind, ExistingPathKind};

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
        let system = session.system();

        for change in params.changes {
            let path = DocumentKey::from_uri(&change.uri).into_file_path();

            let system_path = match path {
                AnySystemPath::System(system) => system,
                AnySystemPath::SystemVirtual(path) => {
                    tracing::debug!("Ignoring virtual path from change event: `{path}`");
                    continue;
                }
            };

            let change_event = match change.kind {
                FileChangeType::Created => ChangeEvent::Created {
                    kind: CreatedKind::from(ExistingPathKind::from_system(system, &system_path)),
                    path: system_path,
                },
                FileChangeType::Changed => {
                    // We're only interested in file content or metadata changes.
                    // Renames are modelled as create/delete events.
                    if ExistingPathKind::from_system(system, &system_path).is_file() {
                        ChangeEvent::Changed {
                            path: system_path,
                            kind: ChangedKind::Any,
                        }
                    } else {
                        continue;
                    }
                }
                FileChangeType::Deleted => ChangeEvent::Deleted {
                    path: system_path,
                    kind: DeletedKind::Any,
                },
                // Custom file change types are not supported and should be ignored.
                FileChangeType::Custom(_) => continue,
            };

            changes.push(change_event);
        }

        if changes.is_empty() {
            return Ok(());
        }

        let client_capabilities = session.client_capabilities();
        let roots: Vec<_> = session
            .workspaces()
            .into_iter()
            .map(|(root, _)| root.clone())
            .collect();
        for root in roots {
            tracing::debug!("Applying changes to `{root}`");

            session.apply_changes(client, &AnySystemPath::System(root.clone()), &changes);
            publish_settings_diagnostics(session, client, root);
        }

        if client_capabilities.supports_workspace_diagnostic_refresh() {
            client.send_request::<types::DiagnosticRefreshRequest>(session, (), |_, ()| {});
        } else {
            publish_all_document_diagnostics(session, client);
        }

        if client_capabilities.supports_inlay_hint_refresh() {
            client.send_request::<types::InlayHintRefreshRequest>(session, (), |_, ()| {});
        }

        Ok(())
    }
}
