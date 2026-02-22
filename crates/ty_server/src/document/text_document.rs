use lsp_types::{TextDocumentContentChangeEvent, Url};
use ruff_source_file::LineIndex;

use crate::PositionEncoding;
use crate::document::range::lsp_range_to_text_range;
use crate::system::AnySystemPath;

pub(crate) type DocumentVersion = i32;

/// A regular text file or the content of a notebook cell.
///
/// The state of an individual document in the server. Stays up-to-date
/// with changes made by the user, including unsaved changes.
#[derive(Debug, Clone)]
pub struct TextDocument {
    /// The URL as sent by the client
    url: Url,

    /// The string contents of the document.
    contents: String,

    /// The latest version of the document, set by the LSP client. The server will panic in
    /// debug mode if we attempt to update the document with an 'older' version.
    version: DocumentVersion,

    /// The language ID of the document as provided by the client.
    language_id: Option<LanguageId>,

    /// For cells, the path to the notebook document.
    notebook: Option<AnySystemPath>,
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
    pub fn new(url: Url, contents: String, version: DocumentVersion) -> Self {
        Self {
            url,
            contents,
            version,
            language_id: None,
            notebook: None,
        }
    }

    #[must_use]
    pub fn with_language_id(mut self, language_id: &str) -> Self {
        self.language_id = Some(LanguageId::from(language_id));
        self
    }

    #[must_use]
    pub(crate) fn with_notebook(mut self, notebook: AnySystemPath) -> Self {
        self.notebook = Some(notebook);
        self
    }

    pub fn into_contents(self) -> String {
        self.contents
    }

    pub(crate) fn url(&self) -> &Url {
        &self.url
    }

    pub fn contents(&self) -> &str {
        &self.contents
    }

    pub fn version(&self) -> DocumentVersion {
        self.version
    }

    pub fn language_id(&self) -> Option<LanguageId> {
        self.language_id
    }

    pub(crate) fn notebook(&self) -> Option<&AnySystemPath> {
        self.notebook.as_ref()
    }

    pub fn apply_changes(
        &mut self,
        changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
        encoding: PositionEncoding,
    ) {
        if let [
            lsp_types::TextDocumentContentChangeEvent {
                range: None, text, ..
            },
        ] = changes.as_slice()
        {
            tracing::debug!("Fast path - replacing entire document");
            self.modify(|contents, version| {
                contents.clone_from(text);
                *version = new_version;
            });
            return;
        }

        let mut new_contents = self.contents().to_string();
        let mut active_index = LineIndex::from_source_text(&new_contents);

        for TextDocumentContentChangeEvent {
            range,
            text: change,
            ..
        } in changes
        {
            if let Some(range) = range {
                let range = lsp_range_to_text_range(range, &new_contents, &active_index, encoding);

                new_contents.replace_range(
                    usize::from(range.start())..usize::from(range.end()),
                    &change,
                );
            } else {
                new_contents = change;
            }

            active_index = LineIndex::from_source_text(&new_contents);
        }

        self.modify(|contents, version| {
            *contents = new_contents;
            *version = new_version;
        });
    }

    pub fn update_version(&mut self, new_version: DocumentVersion) {
        self.modify(|_, version| {
            *version = new_version;
        });
    }

    // A private function for overriding how we update the line index by default.
    fn modify(&mut self, func: impl FnOnce(&mut String, &mut DocumentVersion)) {
        let old_version = self.version;
        func(&mut self.contents, &mut self.version);
        debug_assert!(self.version >= old_version);
    }
}

#[cfg(test)]
mod tests {
    use crate::{PositionEncoding, TextDocument};
    use lsp_types::{Position, TextDocumentContentChangeEvent, Url};

    #[test]
    fn redo_edit() {
        let mut document = TextDocument::new(
            Url::parse("file:///test").unwrap(),
            r#""""
测试comment
一些测试内容
"""
import click


@click.group()
def interface():
    pas
"#
            .to_string(),
            0,
        );

        // Add an `s`, remove it again (back to the original code), and then re-add the `s`
        document.apply_changes(
            vec![
                TextDocumentContentChangeEvent {
                    range: Some(lsp_types::Range::new(
                        Position::new(9, 7),
                        Position::new(9, 7),
                    )),
                    range_length: Some(0),
                    text: "s".to_string(),
                },
                TextDocumentContentChangeEvent {
                    range: Some(lsp_types::Range::new(
                        Position::new(9, 7),
                        Position::new(9, 8),
                    )),
                    range_length: Some(1),
                    text: String::new(),
                },
                TextDocumentContentChangeEvent {
                    range: Some(lsp_types::Range::new(
                        Position::new(9, 7),
                        Position::new(9, 7),
                    )),
                    range_length: Some(0),
                    text: "s".to_string(),
                },
            ],
            1,
            PositionEncoding::UTF16,
        );

        assert_eq!(
            &document.contents,
            r#""""
测试comment
一些测试内容
"""
import click


@click.group()
def interface():
    pass
"#
        );
    }
}
