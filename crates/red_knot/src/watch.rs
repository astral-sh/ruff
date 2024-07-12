use std::path::Path;

use anyhow::Context;
use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{recommended_watcher, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use ruff_db::system::SystemPath;

use crate::db::{FileChangeKind, FileWatcherChange};

pub struct FileWatcher {
    watcher: RecommendedWatcher,
}

pub trait EventHandler: Send + 'static {
    fn handle(&self, changes: Vec<FileWatcherChange>);
}

impl<F> EventHandler for F
where
    F: Fn(Vec<FileWatcherChange>) + Send + 'static,
{
    fn handle(&self, changes: Vec<FileWatcherChange>) {
        let f = self;
        f(changes);
    }
}

impl FileWatcher {
    pub fn new<E>(handler: E) -> anyhow::Result<Self>
    where
        E: EventHandler,
    {
        Self::from_handler(Box::new(handler))
    }

    fn from_handler(handler: Box<dyn EventHandler>) -> anyhow::Result<Self> {
        let watcher = recommended_watcher(move |changes: notify::Result<Event>| {
            dbg!(&changes);
            match changes {
                Ok(event) => {
                    // TODO verify that this handles all events correctly
                    let change_kind = match event.kind {
                        EventKind::Create(CreateKind::File) => FileChangeKind::Created,
                        EventKind::Modify(kind) => match kind {
                            ModifyKind::Name(RenameMode::To) => FileChangeKind::Created,
                            ModifyKind::Name(RenameMode::From) => FileChangeKind::Deleted,
                            _ => FileChangeKind::Modified,
                        },
                        EventKind::Remove(RemoveKind::File) => FileChangeKind::Deleted,
                        _ => {
                            return;
                        }
                    };

                    let mut changes = Vec::new();

                    for path in event.paths {
                        // TODO what about directory removals?
                        //   how do we track this, because we don't know anymore which files were deleted.
                        //   One option is that `Files` uses a `BTreeMap` so that we can search by the prefix
                        //   An other option is to use Rust-analyzers's `SourceRoot` approach where a source-root maps to a directory
                        //   and it stores all known files to that directory. Removing a directory would mean resolving the
                        //   relevant source root and then schedule a removal of all its files.
                        //   Having a `SourceRoot` concept might also be interesting for other use cases:
                        //   - WorkspaceMember::open_paths could store a `SourceRoot` instead of a `PathBuf`
                        //   - Queries retrieving the formatter/linter settings could accept a `SourceRoot` rather than a file, reducing the granularity.
                        if let Some(fs_path) = SystemPath::from_std_path(&path) {
                            changes
                                .push(FileWatcherChange::new(fs_path.to_path_buf(), change_kind));
                        }
                    }

                    if !changes.is_empty() {
                        handler.handle(changes);
                    }
                }
                // TODO proper error handling
                Err(err) => {
                    panic!("Error: {err}");
                }
            }
        })
        .context("Failed to create file watcher.")?;

        Ok(Self { watcher })
    }

    pub fn watch_folder(&mut self, path: &Path) -> anyhow::Result<()> {
        self.watcher.watch(path, RecursiveMode::Recursive)?;

        Ok(())
    }
}
