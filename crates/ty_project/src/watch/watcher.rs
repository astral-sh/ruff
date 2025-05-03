use notify::event::{CreateKind, MetadataKind, ModifyKind, RemoveKind, RenameMode};
use notify::{recommended_watcher, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

use ruff_db::system::{SystemPath, SystemPathBuf};

use crate::watch::{ChangeEvent, ChangedKind, CreatedKind, DeletedKind};

/// Creates a new watcher observing file system changes.
///
/// The watcher debounces events, but guarantees to send all changes eventually (even if the file system keeps changing).
pub fn directory_watcher<H>(handler: H) -> notify::Result<Watcher>
where
    H: EventHandler,
{
    let (sender, receiver) = crossbeam::channel::bounded(20);

    let debouncer = std::thread::Builder::new()
        .name("watcher::debouncer".to_string())
        .spawn(move || {
            // Wait for the next set of changes
            for message in &receiver {
                let event = match message {
                    DebouncerMessage::Event(event) => event,
                    DebouncerMessage::Flush => {
                        continue;
                    }
                };

                let mut debouncer = Debouncer::default();

                debouncer.add_result(event);

                // Debounce any new incoming changes:
                // * Take any new incoming change events and merge them with the previous change events
                // * If there are no new incoming change events after 10 ms, flush the changes and wait for the next notify event.
                // * Flush no later than after 3s.
                loop {
                    let start = std::time::Instant::now();

                    crossbeam::select! {
                        recv(receiver) -> message => {
                            match message {
                                Ok(DebouncerMessage::Event(event)) => {
                                    debouncer.add_result(event);

                                    // Ensure that we flush the changes eventually.
                                    if start.elapsed() > std::time::Duration::from_secs(3) {
                                        break;
                                    }
                                }
                                Ok(DebouncerMessage::Flush) => {
                                    break;
                                }

                                Err(_) => {
                                    // There are no more senders. That means `stop` was called.
                                    // Drop all events and exit immediately.
                                    return;
                                }
                            }
                        },
                        default(std::time::Duration::from_millis(10)) => {
                            break;
                        }
                    }
                }

                // No more file changes after 10 ms, send the changes and schedule a new analysis
                let events = debouncer.into_events();

                if !events.is_empty() {
                    handler.handle(events);
                }
            }
        })
        .unwrap();

    let debouncer_sender = sender.clone();
    let watcher =
        recommended_watcher(move |event| sender.send(DebouncerMessage::Event(event)).unwrap())?;

    Ok(Watcher {
        inner: Some(WatcherInner {
            watcher,
            debouncer_sender,
            debouncer_thread: debouncer,
        }),
    })
}

#[derive(Debug)]
enum DebouncerMessage {
    /// A new file system event.
    Event(notify::Result<notify::Event>),

    Flush,
}

pub struct Watcher {
    inner: Option<WatcherInner>,
}

struct WatcherInner {
    watcher: RecommendedWatcher,
    debouncer_sender: crossbeam::channel::Sender<DebouncerMessage>,
    debouncer_thread: std::thread::JoinHandle<()>,
}

impl Watcher {
    /// Sets up file watching for `path`.
    pub fn watch(&mut self, path: &SystemPath) -> notify::Result<()> {
        tracing::debug!("Watching path: `{path}`");

        self.inner_mut()
            .watcher
            .watch(path.as_std_path(), RecursiveMode::Recursive)
    }

    /// Stops file watching for `path`.
    pub fn unwatch(&mut self, path: &SystemPath) -> notify::Result<()> {
        tracing::debug!("Unwatching path: `{path}`");

        self.inner_mut().watcher.unwatch(path.as_std_path())
    }

    /// Stops the file watcher.
    ///
    /// Pending events will be discarded.
    ///
    /// The call blocks until the watcher has stopped.
    pub fn stop(mut self) {
        tracing::debug!("Stop file watcher");
        self.set_stop();
    }

    /// Flushes any pending events.
    pub fn flush(&self) {
        self.inner()
            .debouncer_sender
            .send(DebouncerMessage::Flush)
            .unwrap();
    }

    fn set_stop(&mut self) {
        if let Some(inner) = self.inner.take() {
            // drop the watcher to ensure there will be no more events.
            // and to drop the sender used by the notify callback.
            drop(inner.watcher);

            // Drop "our" sender to ensure the sender count goes down to 0.
            // The debouncer thread will end as soon as the sender count is 0.
            drop(inner.debouncer_sender);

            // Wait for the debouncer to finish, propagate any panics
            inner.debouncer_thread.join().unwrap();
        }
    }

    fn inner(&self) -> &WatcherInner {
        self.inner.as_ref().expect("Watcher to be running")
    }

    fn inner_mut(&mut self) -> &mut WatcherInner {
        self.inner.as_mut().expect("Watcher to be running")
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.set_stop();
    }
}

#[derive(Default)]
struct Debouncer {
    events: Vec<ChangeEvent>,
    rescan_event: Option<ChangeEvent>,
}

impl Debouncer {
    fn add_result(&mut self, result: notify::Result<notify::Event>) {
        tracing::trace!("Handling file watcher event: {result:?}");
        match result {
            Ok(event) => self.add_event(event),
            Err(error) => self.add_error(error),
        }
    }

    #[expect(clippy::unused_self, clippy::needless_pass_by_value)]
    fn add_error(&mut self, error: notify::Error) {
        // Micha: I skimmed through some of notify's source code and it seems the most common errors
        // are IO errors. All other errors should really only happen when adding or removing a watched folders.
        // It's not clear what an upstream handler should do in the case of an IOError (other than logging it).
        // That's what we do for now as well.
        tracing::warn!("File watcher error: {error:?}");
    }

    fn add_event(&mut self, event: notify::Event) {
        if self.rescan_event.is_some() {
            // We're already in a rescan state, ignore all other events
            return;
        }

        // If the file watcher is out of sync or we observed too many changes, trigger a full rescan
        if event.need_rescan() || self.events.len() > 10000 {
            self.events = Vec::new();
            self.rescan_event = Some(ChangeEvent::Rescan);

            return;
        }

        let kind = event.kind;

        // There are cases where paths can be empty.
        // https://github.com/astral-sh/ruff/issues/14222
        let Some(path) = event.paths.into_iter().next() else {
            tracing::debug!("Ignoring change event with kind '{kind:?}' without a path",);
            return;
        };

        let path = match SystemPathBuf::from_path_buf(path) {
            Ok(path) => path,
            Err(path) => {
                tracing::debug!(
                    "Ignore change to non-UTF8 path `{path}`: {kind:?}",
                    path = path.display()
                );

                // Ignore non-UTF8 paths because they aren't handled by the rest of the system.
                return;
            }
        };

        let event = match kind {
            EventKind::Create(create) => {
                let kind = match create {
                    CreateKind::File => CreatedKind::File,
                    CreateKind::Folder => CreatedKind::Directory,
                    CreateKind::Any | CreateKind::Other => {
                        CreatedKind::from(FileType::from_path(&path))
                    }
                };

                ChangeEvent::Created { path, kind }
            }

            EventKind::Modify(modify) => match modify {
                ModifyKind::Metadata(metadata) => {
                    if FileType::from_path(&path) != FileType::File {
                        // Only interested in file metadata events.
                        return;
                    }

                    match metadata {
                        MetadataKind::Any | MetadataKind::Permissions | MetadataKind::Other => {
                            ChangeEvent::Changed {
                                path,
                                kind: ChangedKind::FileMetadata,
                            }
                        }

                        MetadataKind::AccessTime
                        | MetadataKind::WriteTime
                        | MetadataKind::Ownership
                        | MetadataKind::Extended => {
                            // We're not interested in these metadata changes
                            return;
                        }
                    }
                }

                ModifyKind::Data(_) => ChangeEvent::Changed {
                    kind: ChangedKind::FileContent,
                    path,
                },

                ModifyKind::Name(rename) => match rename {
                    RenameMode::From => {
                        // TODO: notify_debouncer_full matches the `RenameMode::From` and `RenameMode::To` events.
                        //  Matching the from and to event would have the added advantage that we know the
                        //  type of the path that was renamed, allowing `apply_changes` to avoid traversing the
                        //  entire package.
                        //   https://github.com/notify-rs/notify/blob/128bf6230c03d39dbb7f301ff7b20e594e34c3a2/notify-debouncer-full/src/lib.rs#L293-L297
                        ChangeEvent::Deleted {
                            kind: DeletedKind::Any,
                            path,
                        }
                    }

                    RenameMode::To => ChangeEvent::Created {
                        kind: CreatedKind::from(FileType::from_path(&path)),
                        path,
                    },

                    RenameMode::Both => {
                        // Both is only emitted when moving a path from within a watched directory
                        // to another watched directory. The event is not emitted if the `to` or `from` path
                        // lay outside the watched directory. However, the `To` and `From` events are always emitted.
                        // That's why we ignore `Both` and instead rely on `To` and `From`.
                        return;
                    }

                    RenameMode::Other => {
                        // Skip over any other rename events
                        return;
                    }

                    RenameMode::Any => {
                        // Guess the action based on the current file system state
                        if path.as_std_path().exists() {
                            let file_type = FileType::from_path(&path);

                            ChangeEvent::Created {
                                kind: file_type.into(),
                                path,
                            }
                        } else {
                            ChangeEvent::Deleted {
                                kind: DeletedKind::Any,
                                path,
                            }
                        }
                    }
                },
                ModifyKind::Other => {
                    // Skip other modification events that are not content or metadata related
                    return;
                }
                ModifyKind::Any => {
                    if !path.as_std_path().is_file() {
                        return;
                    }

                    ChangeEvent::Changed {
                        path,
                        kind: ChangedKind::Any,
                    }
                }
            },

            EventKind::Access(_) => {
                // We're not interested in any access events
                return;
            }

            EventKind::Remove(kind) => {
                let kind = match kind {
                    RemoveKind::File => DeletedKind::File,
                    RemoveKind::Folder => DeletedKind::Directory,
                    RemoveKind::Any | RemoveKind::Other => DeletedKind::Any,
                };

                ChangeEvent::Deleted { path, kind }
            }

            EventKind::Other => {
                // Skip over meta events
                return;
            }

            EventKind::Any => {
                tracing::debug!("Skipping any FS event for `{path}`");
                return;
            }
        };

        self.events.push(event);
    }

    fn into_events(self) -> Vec<ChangeEvent> {
        if let Some(rescan_event) = self.rescan_event {
            vec![rescan_event]
        } else {
            self.events
        }
    }
}

pub trait EventHandler: Send + 'static {
    fn handle(&self, changes: Vec<ChangeEvent>);
}

impl<F> EventHandler for F
where
    F: Fn(Vec<ChangeEvent>) + Send + 'static,
{
    fn handle(&self, changes: Vec<ChangeEvent>) {
        let f = self;
        f(changes);
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum FileType {
    /// The event is related to a directory.
    File,

    /// The event is related to a directory.
    Directory,

    /// It's unknown whether the event is related to a file or a directory or if it is any other file type.
    Any,
}

impl FileType {
    fn from_path(path: &SystemPath) -> FileType {
        match path.as_std_path().metadata() {
            Ok(metadata) if metadata.is_file() => FileType::File,
            Ok(metadata) if metadata.is_dir() => FileType::Directory,
            Ok(_) | Err(_) => FileType::Any,
        }
    }
}

impl From<FileType> for CreatedKind {
    fn from(value: FileType) -> Self {
        match value {
            FileType::File => Self::File,
            FileType::Directory => Self::Directory,
            FileType::Any => Self::Any,
        }
    }
}
