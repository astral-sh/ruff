use std::sync::Arc;

use ruff_source_file::LineIndex;

/// Cheap cloneable document
#[derive(Debug, Eq, PartialEq, Clone)]
pub(crate) struct Document {
    inner: Arc<DocumentInner>,
}

#[derive(Debug, Clone)]
struct DocumentInner {
    version: i32,
    text: String,
    line_index: LineIndex,
}

impl PartialEq for DocumentInner {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version && self.text == other.text
    }
}

impl Eq for DocumentInner {}

impl Document {
    #[inline]
    pub(crate) fn new<T>(text: T, version: i32) -> Self
    where
        T: Into<String>,
    {
        let text = text.into();
        let index = LineIndex::from_source_text(&text);
        Self {
            inner: Arc::new(DocumentInner {
                version,
                text,
                line_index: index,
            }),
        }
    }

    pub(crate) fn version(&self) -> i32 {
        self.inner.version
    }

    /// Updates the document in place, without allocating if there's no other reference to this document.
    pub(crate) fn update<T>(&mut self, text: T, version: i32)
    where
        T: Into<String>,
    {
        let updated = Arc::make_mut(&mut self.inner);

        let text = text.into();
        updated.line_index = LineIndex::from_source_text(&text);
        updated.version = version;
        updated.text = text;
    }

    pub(crate) fn line_index(&self) -> &LineIndex {
        &self.inner.line_index
    }

    pub(crate) fn text(&self) -> &str {
        &self.inner.text
    }
}
