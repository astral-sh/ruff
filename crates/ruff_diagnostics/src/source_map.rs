use ruff_text_size::{Ranged, TextSize};

use crate::Edit;

/// Lightweight sourcemap marker representing the source and destination
/// position for an [`Edit`].
#[derive(Debug, PartialEq, Eq)]
pub struct SourceMarker {
    /// Position of the marker in the original source.
    source: TextSize,
    /// Position of the marker in the transformed code.
    dest: TextSize,
}

impl SourceMarker {
    pub fn new(source: TextSize, dest: TextSize) -> Self {
        Self { source, dest }
    }

    pub const fn source(&self) -> TextSize {
        self.source
    }

    pub const fn dest(&self) -> TextSize {
        self.dest
    }
}

/// A collection of [`SourceMarker`].
///
/// Sourcemaps are used to map positions in the original source to positions in
/// the transformed code. Here, only the boundaries of edits are tracked instead
/// of every single character.
#[derive(Default, PartialEq, Eq)]
pub struct SourceMap(Vec<SourceMarker>);

impl SourceMap {
    /// Returns a slice of all the markers in the sourcemap in the order they
    /// were added.
    pub fn markers(&self) -> &[SourceMarker] {
        &self.0
    }

    /// Push the start marker for an [`Edit`].
    ///
    /// The `output_length` is the length of the transformed string before the
    /// edit is applied.
    pub fn push_start_marker(&mut self, edit: &Edit, output_length: TextSize) {
        self.push_marker(edit.start(), output_length);
    }

    /// Push the end marker for an [`Edit`].
    ///
    /// The `output_length` is the length of the transformed string after the
    /// edit has been applied.
    pub fn push_end_marker(&mut self, edit: &Edit, output_length: TextSize) {
        if edit.is_insertion() {
            self.push_marker(edit.start(), output_length);
        } else {
            // Deletion or replacement
            self.push_marker(edit.end(), output_length);
        }
    }

    /// Push a new marker to the sourcemap.
    pub fn push_marker(&mut self, offset: TextSize, output_length: TextSize) {
        self.0.push(SourceMarker {
            source: offset,
            dest: output_length,
        });
    }
}
