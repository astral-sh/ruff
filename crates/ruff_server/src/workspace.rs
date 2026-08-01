use std::ops::Deref;

use lsp_types::{Uri, WorkspaceFolders};
use thiserror::Error;

use crate::session::{ClientOptions, WorkspaceOptionsMap};

#[derive(Debug)]
pub(crate) struct Workspaces(Vec<Workspace>);

impl Workspaces {
    /// Create the workspaces from the provided workspace folders as provided by the client during
    /// initialization.
    pub(crate) fn from_workspace_folders(
        workspace_folders: Option<WorkspaceFolders>,
        mut workspace_options: WorkspaceOptionsMap,
    ) -> std::result::Result<Workspaces, WorkspacesError> {
        let mut client_options_for_uri = |uri: &Uri| {
            workspace_options.remove(uri).unwrap_or_else(|| {
                tracing::info!(
                    "No workspace options found for {}, using default options",
                    uri
                );
                ClientOptions::default()
            })
        };

        let workspaces = if let Some(WorkspaceFolders::WorkspaceFolderList(folders)) =
            workspace_folders
            && !folders.is_empty()
        {
            folders
                .into_iter()
                .map(|folder| {
                    let options = client_options_for_uri(&folder.uri);
                    Workspace::new(folder.uri).with_options(options)
                })
                .collect()
        } else {
            let current_dir = std::env::current_dir().map_err(WorkspacesError::Io)?;
            tracing::info!(
                "No workspace(s) were provided during initialization. \
                Using the current working directory as a default workspace: {}",
                current_dir.display()
            );
            let uri = Uri::from_file_path(current_dir)
                .map_err(|()| WorkspacesError::InvalidCurrentDir)?;
            let options = client_options_for_uri(&uri);
            vec![Workspace::default(uri).with_options(options)]
        };

        Ok(Workspaces(workspaces))
    }
}

impl Deref for Workspaces {
    type Target = [Workspace];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Error, Debug)]
pub(crate) enum WorkspacesError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Failed to create a URI from the current working directory")]
    InvalidCurrentDir,
}

#[derive(Debug)]
pub(crate) struct Workspace {
    /// The [`Uri`] pointing to the root of the workspace.
    uri: Uri,
    /// The client options for this workspace.
    options: Option<ClientOptions>,
    /// Whether this is the default workspace as created by the server. This will be the case when
    /// no workspace folders were provided during initialization.
    is_default: bool,
}

impl Workspace {
    /// Create a new workspace with the given root URI.
    pub(crate) fn new(uri: Uri) -> Self {
        Self {
            uri,
            options: None,
            is_default: false,
        }
    }

    /// Create a new default workspace with the given root URI.
    pub(crate) fn default(uri: Uri) -> Self {
        Self {
            uri,
            options: None,
            is_default: true,
        }
    }

    /// Set the client options for this workspace.
    #[must_use]
    pub(crate) fn with_options(mut self, options: ClientOptions) -> Self {
        self.options = Some(options);
        self
    }

    /// Returns the root URI of the workspace.
    pub(crate) fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Returns the client options for this workspace.
    pub(crate) fn options(&self) -> Option<&ClientOptions> {
        self.options.as_ref()
    }

    /// Returns true if this is the default workspace.
    pub(crate) fn is_default(&self) -> bool {
        self.is_default
    }
}
