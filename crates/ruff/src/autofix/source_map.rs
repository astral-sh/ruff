use ruff_text_size::TextSize;

use ruff_diagnostics::Edit;

/// Lightweight sourcemap marker representing the source and destination
/// position for an [`Edit`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SourceMarker {
    /// Position of the marker in the original source.
    pub(crate) source: TextSize,
    /// Position of the marker in the transformed code.
    pub(crate) dest: TextSize,
}

/// A collection of [`SourceMarker`].
///
/// Sourcemaps are used to map positions in the original source to positions in
/// the transformed code. Here, only the boundaries of edits are tracked instead
/// of every single character.
#[derive(Default, PartialEq, Eq)]
pub(crate) struct SourceMap(Vec<SourceMarker>);

impl SourceMap {
    /// Returns a slice of all the markers in the sourcemap in the order they
    /// were added.
    pub(crate) fn markers(&self) -> &[SourceMarker] {
        &self.0
    }

    /// Push the start marker for an [`Edit`].
    ///
    /// The `output_length` is the length of the transformed string before the
    /// edit is applied.
    pub(crate) fn push_start_marker(&mut self, edit: &Edit, output_length: TextSize) {
        self.0.push(SourceMarker {
            source: edit.start(),
            dest: output_length,
        });
    }

    /// Push the end marker for an [`Edit`].
    ///
    /// The `output_length` is the length of the transformed string after the
    /// edit has been applied.
    pub(crate) fn push_end_marker(&mut self, edit: &Edit, output_length: TextSize) {
        if edit.is_insertion() {
            self.0.push(SourceMarker {
                source: edit.start(),
                dest: output_length,
            });
        } else {
            // Deletion or replacement
            self.0.push(SourceMarker {
                source: edit.end(),
                dest: output_length,
            });
        }
    }
}
