use std::ops::Deref;

use lsp_types::{Url, WorkspaceFolder};
use thiserror::Error;

use crate::session::{ClientOptions, WorkspaceOptionsMap};

#[derive(Debug)]
pub struct Workspaces(Vec<Workspace>);

impl Workspaces {
    pub fn new(workspaces: Vec<Workspace>) -> Self {
        Self(workspaces)
    }

    /// Create the workspaces from the provided workspace folders as provided by the client during
    /// initialization.
    pub(crate) fn from_workspace_folders(
        workspace_folders: Option<Vec<WorkspaceFolder>>,
        mut workspace_options: WorkspaceOptionsMap,
    ) -> std::result::Result<Workspaces, WorkspacesError> {
        let mut client_options_for_url = |url: &Url| {
            workspace_options.remove(url).unwrap_or_else(|| {
                tracing::info!(
                    "No workspace options found for {}, using default options",
                    url
                );
                ClientOptions::default()
            })
        };

        let workspaces =
            if let Some(folders) = workspace_folders.filter(|folders| !folders.is_empty()) {
                folders
                    .into_iter()
                    .map(|folder| {
                        let options = client_options_for_url(&folder.uri);
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
                let uri = Url::from_file_path(current_dir)
                    .map_err(|()| WorkspacesError::InvalidCurrentDir)?;
                let options = client_options_for_url(&uri);
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
    #[error("Failed to create a URL from the current working directory")]
    InvalidCurrentDir,
}

#[derive(Debug)]
pub struct Workspace {
    /// The [`Url`] pointing to the root of the workspace.
    url: Url,
    /// The client options for this workspace.
    options: Option<ClientOptions>,
    /// Whether this is the default workspace as created by the server. This will be the case when
    /// no workspace folders were provided during initialization.
    is_default: bool,
}

impl Workspace {
    /// Create a new workspace with the given root URL.
    pub fn new(url: Url) -> Self {
        Self {
            url,
            options: None,
            is_default: false,
        }
    }

    /// Create a new default workspace with the given root URL.
    pub fn default(url: Url) -> Self {
        Self {
            url,
            options: None,
            is_default: true,
        }
    }

    /// Set the client options for this workspace.
    #[must_use]
    pub fn with_options(mut self, options: ClientOptions) -> Self {
        self.options = Some(options);
        self
    }

    /// Returns the root URL of the workspace.
    pub(crate) fn url(&self) -> &Url {
        &self.url
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
