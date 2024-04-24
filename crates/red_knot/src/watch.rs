use std::path::Path;

use crate::files::Files;
use crate::program::{FileChange, FileChangeKind};
use notify::event::{CreateKind, RemoveKind};
use notify::{recommended_watcher, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub struct FileWatcher {
    watcher: RecommendedWatcher,
}

pub trait EventHandler: Send + 'static {
    fn handle(&self, changes: Vec<FileChange>);
}

impl<F> EventHandler for F
where
    F: Fn(Vec<FileChange>) + Send + 'static,
{
    fn handle(&self, changes: Vec<FileChange>) {
        let f = self;
        f(changes);
    }
}

impl FileWatcher {
    pub fn new<E>(handler: E, files: Files) -> Self
    where
        E: EventHandler,
    {
        Self::from_handler(Box::new(handler), files)
    }

    fn from_handler(handler: Box<dyn EventHandler>, files: Files) -> Self {
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
                            let id = files.intern(&path);
                            changes.push(FileChange::new(id, change_kind));
                        }
                    }

                    if !changes.is_empty() {
                        handler.handle(changes);
                    }
                }
                // TODO proper error handling
                Err(_err) => {
                    panic!("Error");
                }
            }
        })
        .unwrap();

        Self { watcher }
    }

    pub fn watch_folder(&mut self, path: &Path) -> anyhow::Result<()> {
        self.watcher.watch(path, RecursiveMode::Recursive)?;

        Ok(())
    }
}
