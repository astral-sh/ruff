use std::path::Path;

use anyhow::Context;
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{recommended_watcher, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use ruff_db::system::{SystemPath, SystemPathBuf};

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
        let watcher = recommended_watcher(move |event: notify::Result<Event>| {
            match event {
                Ok(event) => {
                    // TODO verify that this handles all events correctly
                    let change_kind = match event.kind {
                        EventKind::Create(CreateKind::File) => FileChangeKind::Created,
                        EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::From)) => {
                            FileChangeKind::Deleted
                        }
                        EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::To)) => {
                            FileChangeKind::Created
                        }
                        EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Any)) => {
                            // TODO Introduce a better catch all event for cases that we don't understand.
                            FileChangeKind::Created
                        }
                        EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)) => {
                            todo!("Handle both create and delete event.");
                        }
                        EventKind::Modify(_) => FileChangeKind::Modified,
                        EventKind::Remove(RemoveKind::File) => FileChangeKind::Deleted,
                        _ => {
                            return;
                        }
                    };

                    let mut changes = Vec::new();

                    for path in event.paths {
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

#[derive(Clone, Debug)]
pub struct FileWatcherChange {
    pub path: SystemPathBuf,
    #[allow(unused)]
    pub kind: FileChangeKind,
}

impl FileWatcherChange {
    pub fn new(path: SystemPathBuf, kind: FileChangeKind) -> Self {
        Self { path, kind }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
}
