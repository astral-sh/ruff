use lsp_types as types;
use lsp_types::notification as notif;

use crate::server::Result;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;

pub(crate) struct DidChangeWorkspaceFoldersHandler;

impl NotificationHandler for DidChangeWorkspaceFoldersHandler {
    type NotificationType = notif::DidChangeWorkspaceFolders;
}

impl SyncNotificationHandler for DidChangeWorkspaceFoldersHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: types::DidChangeWorkspaceFoldersParams,
    ) -> Result<()> {
        let format_workspace_folders = |folders: &[lsp_types::WorkspaceFolder]| -> String {
            if folders.is_empty() {
                "<empty>".to_string()
            } else {
                folders
                    .iter()
                    .map(|folder| format!("({}, {})", folder.name, folder.uri))
                    .collect::<Vec<String>>()
                    .join(", ")
            }
        };
        tracing_unlikely::debug!(
            "Workspace folder change notification, to add: {to_add}, to remove: {to_remove}",
            to_add = format_workspace_folders(&params.event.added),
            to_remove = format_workspace_folders(&params.event.removed),
        );

        let mut added_workspace_folder = false;
        for folder in params.event.added {
            match session.register_workspace_folder(folder.uri.clone()) {
                Ok(true) => {
                    added_workspace_folder = true;
                }
                Ok(false) => {
                    tracing_unlikely::debug!(
                        "Workspace folder `{uri}` has already been added",
                        uri = folder.uri,
                    );
                }
                Err(err) => {
                    tracing_unlikely::error!(
                        "Failed to add workspace folder `{uri}`: {err}",
                        uri = folder.uri,
                    );
                }
            }
        }
        for folder in params.event.removed {
            // It would perhaps be more efficient to do this "in bulk"
            // in one step (since at present, the session needs to
            // iterate over all documents in its index). So if multiple
            // workspace folders are removed at once, we could save
            // some work.
            //
            // In practice though, the number of open text documents
            // is likely to be "small," and the frequency of removing
            // multiple workspace folders at a time is also likely to
            // be small. So we prefer a simpler implementation for now.
            // ---AG
            if let Err(err) = session.remove_workspace_folder(client, &folder.uri) {
                tracing_unlikely::error!(
                    "Failed to remove workspace folder `{uri}`: {err}",
                    uri = folder.uri,
                );
            }
        }
        if added_workspace_folder {
            session.request_uninitialized_workspace_folder_configurations(client);
        }
        Ok(())
    }
}
