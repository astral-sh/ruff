use crate::cache::KeyValueCache;
use crate::db::{HasJar, SourceDb, SourceJar};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::files::FileId;

pub(crate) fn source_text<Db>(db: &Db, file_id: FileId) -> Source
where
    Db: SourceDb + HasJar<SourceJar>,
{
    let sources = &db.jar().sources;

    sources.get(&file_id, |file_id| {
        let path = db.file_path(*file_id);

        let source_text = std::fs::read_to_string(&path).unwrap_or_else(|err| {
            tracing::error!("Failed to read file '{path:?}: {err}'. Falling back to empty text");
            String::new()
        });

        Source::new(source_text)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    text: Arc<str>,
}

impl Source {
    pub fn new<T: Into<Arc<str>>>(source: T) -> Self {
        Self {
            text: source.into(),
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Default)]
pub struct SourceStorage(pub(crate) KeyValueCache<FileId, Source>);

impl Deref for SourceStorage {
    type Target = KeyValueCache<FileId, Source>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SourceStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
