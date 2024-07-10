use std::path::Path;

use anyhow::Context;
use notify::event::{CreateKind, RemoveKind};
use notify::{recommended_watcher, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use ruff_db::system::SystemPath;

use crate::program::{FileChangeKind, FileWatcherChange};

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
            match changes {
                Ok(event) => {
                    // TODO verify that this handles all events correctly
                    let change_kind = match event.kind {
                        EventKind::Create(CreateKind::File) => FileChangeKind::Created,
                        EventKind::Modify(_) => FileChangeKind::Modified,
                        EventKind::Remove(RemoveKind::File) => FileChangeKind::Deleted,
                        _ => {
                            return;
                        }
                    };

                    let mut changes = Vec::new();

                    for path in event.paths {
                        if path.is_file() {
                            if let Some(fs_path) = SystemPath::from_std_path(&path) {
                                changes.push(FileWatcherChange::new(
                                    fs_path.to_path_buf(),
                                    change_kind,
                                ));
                            }
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
