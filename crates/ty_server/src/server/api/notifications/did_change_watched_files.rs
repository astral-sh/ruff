use crate::document::DocumentKey;
use crate::server::Result;
use crate::server::api::diagnostics::{
    publish_diagnostics_if_needed, publish_settings_diagnostics,
};
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use lsp_types as types;
use lsp_types::{FileChangeType, notification as notif};
use ty_project::Db as _;
use ty_project::watch::{ChangeEvent, ChangedKind, CreatedKind, DeletedKind, ExistingPathKind};

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
        let mut changes = Vec::new();
        let system = session.system();

        for change in params.changes {
            let path = DocumentKey::from_url(&change.uri).into_file_path();

            let system_path = match path {
                AnySystemPath::System(system) => system,
                AnySystemPath::SystemVirtual(path) => {
                    tracing::debug!("Ignoring virtual path from change event: `{path}`");
                    continue;
                }
            };

            let change_event = match change.typ {
                FileChangeType::CREATED => ChangeEvent::Created {
                    kind: CreatedKind::from(ExistingPathKind::from_system(system, &system_path)),
                    path: system_path,
                },
                FileChangeType::CHANGED => {
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

            changes.push(change_event);
        }

        if changes.is_empty() {
            return Ok(());
        }

        let roots: Vec<_> = session
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
            client.send_request::<types::request::WorkspaceDiagnosticRefresh>(
                session,
                (),
                |_, ()| {},
            );
        } else {
            for key in session.text_document_handles() {
                publish_diagnostics_if_needed(&key, session, client);
            }
        }

        if client_capabilities.supports_inlay_hint_refresh() {
            client.send_request::<types::request::InlayHintRefreshRequest>(session, (), |_, ()| {});
        }

        Ok(())
    }
}
