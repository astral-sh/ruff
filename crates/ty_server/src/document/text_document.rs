use lsp_types::{
    LanguageKind, TextDocumentContentChangeEvent, TextDocumentContentChangePartial,
    TextDocumentContentChangeWholeDocument, Uri,
};
use ruff_source_file::LineIndex;

use crate::PositionEncoding;
use crate::document::range::{PositionError, lsp_range_to_text_range};
use crate::system::AnySystemPath;

pub(crate) type DocumentVersion = i32;

/// A regular text file or the content of a notebook cell.
///
/// The state of an individual document in the server. Stays up-to-date
/// with changes made by the user, including unsaved changes.
#[derive(Debug, Clone)]
pub struct TextDocument {
    /// The URI as sent by the client
    uri: Uri,

    /// The string contents of the document.
    contents: String,

    /// The latest version of the document, set by the LSP client. The server will panic in
    /// debug mode if we attempt to update the document with an 'older' version.
    version: DocumentVersion,

    /// The language ID of the document as provided by the client.
    language_id: LanguageId,

    /// For cells, the path to the notebook document.
    notebook: Option<AnySystemPath>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum LanguageId {
    Python,
    Other,
}

impl From<LanguageKind> for LanguageId {
    fn from(language_id: LanguageKind) -> Self {
        match language_id {
            LanguageKind::Python => Self::Python,
            _ => Self::Other,
        }
    }
}

impl TextDocument {
    pub(crate) fn new(
        uri: Uri,
        contents: String,
        version: DocumentVersion,
        language_id: LanguageKind,
    ) -> Self {
        Self {
            uri,
            contents,
            version,
            language_id: LanguageId::from(language_id),
            notebook: None,
        }
    }

    #[must_use]
    pub(crate) fn with_notebook(mut self, notebook: AnySystemPath) -> Self {
        self.notebook = Some(notebook);
        self
    }

    pub(crate) fn uri(&self) -> &Uri {
        &self.uri
    }

    pub(crate) fn contents(&self) -> &str {
        &self.contents
    }

    pub(crate) fn version(&self) -> DocumentVersion {
        self.version
    }

    pub(crate) fn language_id(&self) -> LanguageId {
        self.language_id
    }

    pub(crate) fn notebook(&self) -> Option<&AnySystemPath> {
        self.notebook.as_ref()
    }

    pub(crate) fn apply_changes(
        &mut self,
        changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
        encoding: PositionEncoding,
    ) -> Result<(), PositionError> {
        if let [
            lsp_types::TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                TextDocumentContentChangeWholeDocument { text },
            ),
        ] = changes.as_slice()
        {
            tracing::debug!("Fast path - replacing entire document");
            self.modify(|contents, version| {
                contents.clone_from(text);
                *version = new_version;
            });
            return Ok(());
        }

        let mut new_contents = self.contents().to_string();
        let mut active_index = LineIndex::from_source_text(&new_contents);

        for change in changes {
            match change {
                TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                    TextDocumentContentChangePartial { range, text, .. },
                ) => {
                    let range =
                        lsp_range_to_text_range(range, &new_contents, &active_index, encoding)?;

                    new_contents
                        .replace_range(usize::from(range.start())..usize::from(range.end()), &text);
                }
                TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                    TextDocumentContentChangeWholeDocument { text },
                ) => {
                    new_contents = text;
                }
            }

            active_index = LineIndex::from_source_text(&new_contents);
        }

        self.modify(|contents, version| {
            *contents = new_contents;
            *version = new_version;
        });

        Ok(())
    }

    pub(crate) fn update_version(&mut self, new_version: DocumentVersion) {
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
    use lsp_types::{
        LanguageKind, Position, TextDocumentContentChangeEvent, TextDocumentContentChangePartial,
        Uri,
    };

    #[test]
    fn redo_edit() {
        let mut document = TextDocument::new(
            Uri::parse("file:///test").unwrap(),
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
            LanguageKind::Python,
        );

        // Add an `s`, remove it again (back to the original code), and then re-add the `s`
        document
            .apply_changes(
                vec![
                    TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                        TextDocumentContentChangePartial {
                            range: lsp_types::Range::new(Position::new(9, 7), Position::new(9, 7)),
                            text: "s".to_string(),
                            ..Default::default()
                        },
                    ),
                    TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                        TextDocumentContentChangePartial {
                            range: lsp_types::Range::new(Position::new(9, 7), Position::new(9, 8)),
                            text: String::new(),
                            ..Default::default()
                        },
                    ),
                    TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                        TextDocumentContentChangePartial {
                            range: lsp_types::Range::new(Position::new(9, 7), Position::new(9, 7)),
                            text: "s".to_string(),
                            ..Default::default()
                        },
                    ),
                ],
                1,
                PositionEncoding::UTF16,
            )
            .unwrap();

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

    #[test]
    fn rejects_reversed_changes() {
        let mut document = TextDocument::new(
            Uri::parse("file:///test").unwrap(),
            "abc".to_string(),
            1,
            LanguageKind::Python,
        );

        let result = document.apply_changes(
            vec![
                TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                    TextDocumentContentChangePartial {
                        range: lsp_types::Range::new(Position::new(0, 2), Position::new(0, 1)),
                        text: String::new(),
                        ..Default::default()
                    },
                ),
            ],
            2,
            PositionEncoding::UTF16,
        );

        assert!(result.is_err());
        assert_eq!(document.contents(), "abc");
        assert_eq!(document.version(), 1);
    }

    #[test]
    fn rejects_utf8_changes_inside_characters() {
        let mut document = TextDocument::new(
            Uri::parse("file:///test").unwrap(),
            "é".to_string(),
            1,
            LanguageKind::Python,
        );

        let result = document.apply_changes(
            vec![
                TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                    TextDocumentContentChangePartial {
                        range: lsp_types::Range::new(Position::new(0, 1), Position::new(0, 1)),
                        text: "x".to_string(),
                        ..Default::default()
                    },
                ),
            ],
            2,
            PositionEncoding::UTF8,
        );

        assert!(result.is_err());
        assert_eq!(document.contents(), "é");
        assert_eq!(document.version(), 1);
    }

    #[test]
    fn rejects_invalid_changes_without_committing_preceding_changes() {
        let mut document = TextDocument::new(
            Uri::parse("file:///test").unwrap(),
            "é".to_string(),
            1,
            LanguageKind::Python,
        );

        let result = document.apply_changes(
            vec![
                TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                    TextDocumentContentChangePartial {
                        range: lsp_types::Range::new(Position::new(0, 0), Position::new(0, 0)),
                        text: "x".to_string(),
                        ..Default::default()
                    },
                ),
                TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                    TextDocumentContentChangePartial {
                        range: lsp_types::Range::new(Position::new(0, 2), Position::new(0, 2)),
                        text: "y".to_string(),
                        ..Default::default()
                    },
                ),
            ],
            2,
            PositionEncoding::UTF8,
        );

        assert!(result.is_err());
        assert_eq!(document.contents(), "é");
        assert_eq!(document.version(), 1);
    }

    #[test]
    fn rejects_out_of_bounds_offset_on_bare_cr_line() {
        let mut document = TextDocument::new(
            Uri::parse("file:///test").unwrap(),
            "a\rb".to_string(),
            1,
            LanguageKind::Python,
        );

        let result = document.apply_changes(
            vec![
                TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                    TextDocumentContentChangePartial {
                        range: lsp_types::Range::new(Position::new(0, 1), Position::new(0, 1)),
                        text: "X".to_string(),
                        ..Default::default()
                    },
                ),
            ],
            2,
            PositionEncoding::UTF8,
        );

        assert!(result.is_ok());
        assert_eq!(document.contents(), "aX\rb");
    }
}
