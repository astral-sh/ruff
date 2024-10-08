use lsp_types::TextDocumentContentChangeEvent;
use ruff_source_file::LineIndex;

use crate::PositionEncoding;

use super::RangeExt;

pub(crate) type DocumentVersion = i32;

/// The state of an individual document in the server. Stays up-to-date
/// with changes made by the user, including unsaved changes.
#[derive(Debug, Clone)]
pub struct TextDocument {
    /// The string contents of the document.
    contents: String,
    /// A computed line index for the document. This should always reflect
    /// the current version of `contents`. Using a function like [`Self::modify`]
    /// will re-calculate the line index automatically when the `contents` value is updated.
    index: LineIndex,
    /// The latest version of the document, set by the LSP client. The server will panic in
    /// debug mode if we attempt to update the document with an 'older' version.
    version: DocumentVersion,
    /// The language ID of the document as provided by the client.
    language_id: Option<LanguageId>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LanguageId {
    Python,
    Other,
}

impl From<&str> for LanguageId {
    fn from(language_id: &str) -> Self {
        match language_id {
            "python" => Self::Python,
            _ => Self::Other,
        }
    }
}

impl TextDocument {
    pub fn new(contents: String, version: DocumentVersion) -> Self {
        let index = LineIndex::from_source_text(&contents);
        Self {
            contents,
            index,
            version,
            language_id: None,
        }
    }

    #[must_use]
    pub fn with_language_id(mut self, language_id: &str) -> Self {
        self.language_id = Some(LanguageId::from(language_id));
        self
    }

    pub fn into_contents(self) -> String {
        self.contents
    }

    pub fn contents(&self) -> &str {
        &self.contents
    }

    pub fn index(&self) -> &LineIndex {
        &self.index
    }

    pub fn version(&self) -> DocumentVersion {
        self.version
    }

    pub fn language_id(&self) -> Option<LanguageId> {
        self.language_id
    }

    pub fn apply_changes(
        &mut self,
        changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
        encoding: PositionEncoding,
    ) {
        if let [lsp_types::TextDocumentContentChangeEvent {
            range: None, text, ..
        }] = changes.as_slice()
        {
            tracing::debug!("Fast path - replacing entire document");
            self.modify(|contents, version| {
                contents.clone_from(text);
                *version = new_version;
            });
            return;
        }

        let old_contents = self.contents().to_string();
        let mut new_contents = self.contents().to_string();
        let mut active_index = self.index().clone();

        for TextDocumentContentChangeEvent {
            range,
            text: change,
            ..
        } in changes
        {
            if let Some(range) = range {
                let range = range.to_text_range(&new_contents, &active_index, encoding);

                new_contents.replace_range(
                    usize::from(range.start())..usize::from(range.end()),
                    &change,
                );
            } else {
                new_contents = change;
            }

            if new_contents != old_contents {
                active_index = LineIndex::from_source_text(&new_contents);
            }
        }

        self.modify_with_manual_index(|contents, version, index| {
            if contents != &new_contents {
                *index = active_index;
            }
            *contents = new_contents;
            *version = new_version;
        });
    }

    pub fn update_version(&mut self, new_version: DocumentVersion) {
        self.modify_with_manual_index(|_, version, _| {
            *version = new_version;
        });
    }

    // A private function for modifying the document's internal state
    fn modify(&mut self, func: impl FnOnce(&mut String, &mut DocumentVersion)) {
        self.modify_with_manual_index(|c, v, i| {
            func(c, v);
            *i = LineIndex::from_source_text(c);
        });
    }

    // A private function for overriding how we update the line index by default.
    fn modify_with_manual_index(
        &mut self,
        func: impl FnOnce(&mut String, &mut DocumentVersion, &mut LineIndex),
    ) {
        let old_version = self.version;
        func(&mut self.contents, &mut self.version, &mut self.index);
        debug_assert!(self.version >= old_version);
    }
}
