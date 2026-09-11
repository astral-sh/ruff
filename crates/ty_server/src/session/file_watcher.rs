//! Dynamic LSP file-watcher registrations for projects and their scripts.

use std::hash::{DefaultHasher, Hash, Hasher};

use indexmap::IndexMap;
use lsp_types::{
    DidChangeWatchedFilesNotification, DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher,
    GlobPattern, Notification, Registration, RegistrationParams, RegistrationRequest,
    RelativePattern, Request, Unregistration, UnregistrationParams, UnregistrationRequest, Uri,
};
use ruff_db::Db as _;
use ruff_db::files::Files;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ty_project::Db as _;
use ty_project::ProjectDatabase;
use ty_project::watch::watch_paths;

use crate::capabilities::ResolvedClientCapabilities;
use crate::server::SendRequest;

use super::{
    Session,
    client::{Client, ClientResponseHandler},
};

const MAX_RECONCILIATION_PASSES: u8 = 10;

/// Manages the client's watched-file registration for all projects.
///
/// The watched paths are all registered with a single register request. A replacement is registered
/// before the previous registration is removed, so existing paths remain watched.
#[derive(Default)]
pub(super) struct LspFileWatcher {
    /// ID of the last registration accepted by the client, retained during replacement.
    registered_id: Option<String>,

    /// Absolute paths currently watched.
    /// Empty when the client doesn't support relative file watchers.
    registered_paths: Vec<SystemPathBuf>,

    /// Id of the next registration request.
    next_registration_id: u64,

    /// Registration ID and path state awaiting the client's response.
    pending_registration: Option<PendingRegistration>,

    /// Cache key to short-circuit `reconcile` if the watched paths are unchanged.
    cache_key: Option<u64>,

    /// Registrations in the current reconciliation cycle, capped if refreshes keep changing paths.
    reconciliation_passes: u8,

    /// Whether the client supports relative file watches. For the watcher,
    /// this mainly defines whether the client supports watching paths outside the project's root.
    supports_relative_file_watcher: bool,
}

impl LspFileWatcher {
    pub(super) fn new(capabilities: ResolvedClientCapabilities) -> Option<Self> {
        if !capabilities.supports_file_watcher() {
            tracing::warn!(
                "Your LSP client doesn't support file watching: \
                 You may see stale results when files change outside the editor"
            );
            return None;
        }

        Some(Self {
            supports_relative_file_watcher: capabilities.supports_relative_file_watcher(),
            ..Self::default()
        })
    }

    /// Updates project watch paths, starting a new reconciliation cycle when idle.
    pub(super) fn update<'db>(
        &mut self,
        projects: impl ExactSizeIterator<Item = &'db ProjectDatabase> + Clone,
    ) -> Option<FileWatcherUpdate> {
        // The pending response will reconcile current paths, so this update
        // stays within the same reconciliation limit.
        if self.pending_registration.is_none() {
            self.reconciliation_passes = 0;
        }

        self.reconcile(projects)
    }

    /// Computes which paths need to be watched and returns an updated registration
    /// if there are any changes to the watched paths.
    fn reconcile<'db>(
        &mut self,
        projects: impl ExactSizeIterator<Item = &'db ProjectDatabase> + Clone,
    ) -> Option<FileWatcherUpdate> {
        // Wait for the client's response before preparing another registration;
        // its handler will reconcile the latest project paths.
        if self.pending_registration.is_some() {
            return None;
        }

        // Compute a single cache key across all projects
        // We can remove this once ty supports multiple project and
        // workspace-folders is migrated to that feature.
        let mut hasher = DefaultHasher::new();
        projects.len().hash(&mut hasher);
        for db in projects.clone() {
            watch_paths(db, db.project()).cache_key().hash(&mut hasher);
        }
        let cache_key = hasher.finish();

        if self.cache_key == Some(cache_key) {
            return None;
        }

        if self.reconciliation_passes >= MAX_RECONCILIATION_PASSES {
            tracing::warn!(
                "LSP file watcher paths did not stabilize after {MAX_RECONCILIATION_PASSES} registrations; changes outside the registered paths may be missed until the next update"
            );
            return None;
        }

        let paths: Vec<_> = projects
            .map(|db| (db, db.project().root(db), watch_paths(db, db.project())))
            .collect();

        let (watchers, paths): (Vec<_>, Vec<_>) = if self.supports_relative_file_watcher {
            let watchers: IndexMap<_, _> = paths
                .iter()
                .flat_map(|(db, _, paths)| {
                    paths.paths().iter().map(move |path| {
                        // Watch a file by name from its parent so the glob matches the file itself.
                        let watcher = if db.system().is_file(path)
                            && let (Some(parent), Some(name)) = (path.parent(), path.file_name())
                        {
                            relative_watcher(parent, name)
                        } else {
                            relative_watcher(path, "**")
                        };
                        (path.to_path_buf(), watcher)
                    })
                })
                .collect();
            let (paths, watchers) = watchers.into_iter().unzip();
            (watchers, paths)
        } else {
            if let Some(unwatched_path) = paths.iter().find_map(|(_, root, paths)| {
                paths.paths().iter().find(|path| !path.starts_with(root))
            }) {
                tracing::warn!(
                    "Your LSP client doesn't support file watching outside the project: \
                     changes at or under `{unwatched_path}` may not be reported"
                );
            }
            if self.registered_id.is_some() {
                // Without relative patterns, changing an external search path cannot
                // change the workspace-wide watch we registered with the client.
                self.cache_key = Some(cache_key);
                return None;
            }
            (
                vec![FileSystemWatcher {
                    glob_pattern: GlobPattern::Pattern("**".into()),
                    kind: None,
                }],
                Vec::new(),
            )
        };

        // Refresh files in paths that weren't watched before. This is to handle
        // the case where a path was watched, then unwatched, and now gets watched again.
        // Files in that path might have changed while it was unwatched.
        // We don't need to do this if this is the initial registration.
        let newly_covered_paths = if self.registered_id.is_some() {
            paths
                .iter()
                .filter(|path| {
                    !self
                        .registered_paths
                        .iter()
                        .any(|registered| path.starts_with(registered))
                })
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let registration_id = format!(
            "ty/workspace/didChangeWatchedFiles/{}",
            self.next_registration_id
        );
        self.next_registration_id += 1;
        self.reconciliation_passes += 1;

        Some(FileWatcherUpdate {
            registration: Registration {
                id: registration_id,
                method: DidChangeWatchedFilesNotification::METHOD.into(),
                register_options: Some(
                    serde_json::to_value(DidChangeWatchedFilesRegistrationOptions { watchers })
                        .unwrap(),
                ),
            },
            cache_key,
            paths,
            newly_covered_paths,
        })
    }

    fn complete_registration(
        &mut self,
        response: &lsp_server::Response,
    ) -> Option<FileWatcherCompletion> {
        let update = self.pending_registration.take()?;
        match &response.response_result {
            Ok(value) if value.is_null() => {}
            Err(error) => {
                tracing::error!(
                    "File watcher registration failed (code {}): {}",
                    error.code,
                    error.message
                );
                // Leave the old registration active. A later update can retry.
                return None;
            }
            Ok(_) => {
                tracing::error!("File watcher registration returned an invalid response");
                return None;
            }
        }

        let old_id = self.registered_id.replace(update.registration_id);
        self.registered_paths = update.paths;
        self.cache_key = Some(update.cache_key);
        Some(FileWatcherCompletion {
            old_id,
            newly_covered_paths: update.newly_covered_paths,
        })
    }
}

pub(super) struct FileWatcherUpdate {
    registration: Registration,
    cache_key: u64,
    paths: Vec<SystemPathBuf>,
    newly_covered_paths: Vec<SystemPathBuf>,
}

impl FileWatcherUpdate {
    pub(super) fn apply(self, session: &mut Session, client: &Client) {
        let FileWatcherUpdate {
            registration,
            cache_key,
            paths,
            newly_covered_paths,
        } = self;

        let pending_registration = PendingRegistration {
            registration_id: registration.id.clone(),
            cache_key,
            paths,
            newly_covered_paths,
        };

        client.send_request_raw(
            session,
            SendRequest {
                method: RegistrationRequest::METHOD.to_string(),
                params: serde_json::to_value(RegistrationParams {
                    registrations: vec![registration],
                })
                .expect("registration options are serializable"),
                response_handler: ClientResponseHandler::new(|client, session, response| {
                    let Some(completion) = session
                        .file_watcher
                        .as_mut()
                        .and_then(|watcher| watcher.complete_registration(&response))
                    else {
                        return;
                    };
                    completion.apply(session, client);
                    // Refreshing a newly covered `site-packages` can reveal an edited
                    // `.pth` file with another search path that needs a watch.
                    if let Some(update) = session.file_watcher.as_mut().and_then(|watcher| {
                        watcher.reconcile(session.projects.values().map(|state| &state.db))
                    }) {
                        update.apply(session, client);
                    }
                }),
            },
        );

        if let Some(watcher) = session.file_watcher.as_mut() {
            watcher.pending_registration = Some(pending_registration);
        }
    }
}

struct FileWatcherCompletion {
    old_id: Option<String>,
    newly_covered_paths: Vec<SystemPathBuf>,
}

impl FileWatcherCompletion {
    fn apply(self, session: &mut Session, client: &Client) {
        // Existing paths were watched by the old registration throughout this
        // request. Refresh newly covered paths after the client's response so
        // edits missed before registration invalidate cached files, including `.pth` files.
        // A client could respond before its watches are active, leaving a small
        // gap that the protocol does not let us close completely.
        if !self.newly_covered_paths.is_empty() {
            for state in session.projects.values_mut() {
                Files::sync_all_recursive(&mut state.db, &self.newly_covered_paths);
            }
        }

        if let Some(id) = self.old_id {
            client.send_request::<UnregistrationRequest>(
                session,
                UnregistrationParams {
                    unregisterations: vec![Unregistration {
                        id,
                        method: DidChangeWatchedFilesNotification::METHOD.into(),
                    }],
                },
                |_, ()| {},
            );
        }
    }
}

fn relative_watcher(path: &SystemPath, pattern: &str) -> FileSystemWatcher {
    let base_uri =
        Uri::from_file_path(path.as_std_path()).expect("system path must be a valid URI");
    FileSystemWatcher {
        glob_pattern: GlobPattern::RelativePattern(RelativePattern {
            base_uri: base_uri.into(),
            pattern: pattern.to_string(),
        }),
        kind: None,
    }
}

struct PendingRegistration {
    registration_id: String,
    cache_key: u64,
    paths: Vec<SystemPathBuf>,
    newly_covered_paths: Vec<SystemPathBuf>,
}
