use std::ops::Deref;

use lsp_types::{Url, WorkspaceFolder};
use thiserror::Error;

use crate::session::WorkspaceSettingsMap;
use crate::ClientSettings;

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
        mut workspace_settings: WorkspaceSettingsMap,
    ) -> std::result::Result<Workspaces, WorkspacesError> {
        let mut client_settings_for_url = |url: &Url| {
            workspace_settings.remove(url).unwrap_or_else(|| {
                tracing::info!(
                    "No workspace settings found for {}, using default settings",
                    url
                );
                ClientSettings::default()
            })
        };

        let workspaces =
            if let Some(folders) = workspace_folders.filter(|folders| !folders.is_empty()) {
                folders
                    .into_iter()
                    .map(|folder| {
                        let settings = client_settings_for_url(&folder.uri);
                        Workspace::new(folder.uri).with_settings(settings)
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
                let settings = client_settings_for_url(&uri);
                vec![Workspace::default(uri).with_settings(settings)]
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
    /// The client settings for this workspace.
    settings: Option<ClientSettings>,
    /// Whether this is the default workspace as created by the server. This will be the case when
    /// no workspace folders were provided during initialization.
    is_default: bool,
}

impl Workspace {
    /// Create a new workspace with the given root URL.
    pub fn new(url: Url) -> Self {
        Self {
            url,
            settings: None,
            is_default: false,
        }
    }

    /// Create a new default workspace with the given root URL.
    pub fn default(url: Url) -> Self {
        Self {
            url,
            settings: None,
            is_default: true,
        }
    }

    /// Set the client settings for this workspace.
    #[must_use]
    pub fn with_settings(mut self, settings: ClientSettings) -> Self {
        self.settings = Some(settings);
        self
    }

    /// Returns the root URL of the workspace.
    pub(crate) fn url(&self) -> &Url {
        &self.url
    }

    /// Returns the client settings for this workspace.
    pub(crate) fn settings(&self) -> Option<&ClientSettings> {
        self.settings.as_ref()
    }

    /// Returns true if this is the default workspace.
    pub(crate) fn is_default(&self) -> bool {
        self.is_default
    }
}
