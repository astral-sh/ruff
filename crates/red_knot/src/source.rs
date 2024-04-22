use std::hash::Hash;
use std::sync::Arc;

use crate::files::FileId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceData {
    text: Arc<str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Source {
    file: FileId,
    // TODO support Jupyter notebooks
    source: Arc<str>,
}

impl Source {
    pub fn new<T: Into<Arc<str>>>(file: FileId, source: T) -> Self {
        Self {
            file,
            source: source.into(),
        }
    }

    pub fn file(&self) -> FileId {
        self.file
    }

    pub fn text(&self) -> &str {
        &self.source
    }
}
