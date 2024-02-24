use lsp_types::{Position, TextDocumentContentChangeEvent};
use ruff_source_file::LineIndex;

use crate::PositionEncoding;

use super::range::text_range;

#[derive(Debug, Clone)]
pub struct Document {
    contents: String,
    index: LineIndex,
    version: i32,
}

impl Document {
    pub fn new(contents: String, version: i32) -> Self {
        let index = LineIndex::from_source_text(&contents);
        Self {
            contents,
            index,
            version,
        }
    }
    // TODO(jane): I would personally be in favor of removing access to this method and only
    // allowing document mutation via specialized methods.
    pub(crate) fn modify(&mut self, func: impl FnOnce(&mut String, &mut i32)) {
        self.modify_with_manual_index(|c, v, i| {
            func(c, v);
            *i = LineIndex::from_source_text(c);
        });
    }

    // A private function for overriding how we update the line index by default.
    fn modify_with_manual_index(
        &mut self,
        func: impl FnOnce(&mut String, &mut i32, &mut LineIndex),
    ) {
        let old_version = self.version;
        func(&mut self.contents, &mut self.version, &mut self.index);
        debug_assert!(self.version >= old_version);
    }
}

/* Mutable API */
impl Document {
    pub fn apply_changes(
        &mut self,
        changes: Vec<lsp_types::TextDocumentContentChangeEvent>,
        new_version: i32,
        encoding: PositionEncoding,
    ) {
        if let [lsp_types::TextDocumentContentChangeEvent {
            range: None, text, ..
        }] = changes.as_slice()
        {
            tracing::debug!("Fast path - replacing entire document");
            self.modify(|contents, version| {
                *contents = text.clone();
                *version = new_version;
            });
            return;
        }

        let mut new_contents = self.contents().to_string();
        let mut active_index = None;

        let mut last_position = Position {
            line: u32::MAX,
            character: u32::MAX,
        };

        for TextDocumentContentChangeEvent {
            range,
            text: change,
            ..
        } in changes
        {
            if let Some(range) = range {
                if last_position <= range.end {
                    active_index.replace(LineIndex::from_source_text(&new_contents));
                }

                last_position = range.start;
                let range = text_range(
                    range,
                    &new_contents,
                    active_index.as_ref().unwrap_or(self.index()),
                    encoding,
                );

                new_contents.replace_range(
                    usize::from(range.start())..usize::from(range.end()),
                    &change,
                );
            } else {
                new_contents = change;
                last_position = Position::default();
            }
        }

        self.modify_with_manual_index(|contents, version, index| {
            *index = LineIndex::from_source_text(&new_contents);
            *contents = new_contents;
            *version = new_version;
        });
    }
}

/* Immutable API */
impl Document {
    pub fn contents(&self) -> &str {
        &self.contents
    }
    pub fn index(&self) -> &LineIndex {
        &self.index
    }
    pub fn version(&self) -> i32 {
        self.version
    }
}
