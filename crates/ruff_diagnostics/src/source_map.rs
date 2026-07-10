use ruff_text_size::{Ranged, TextSize};

use crate::Edit;

/// Lightweight source map marker representing corresponding source and target positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMarker {
    /// Position of the marker in the source.
    source: TextSize,
    /// Corresponding position in the target.
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

/// A collection of [`SourceMarker`]s that maps offsets from one source to another.
///
/// Each marker establishes a source-to-target correspondence. Offsets between markers preserve
/// their displacement from the preceding marker.
///
/// This mapping maintains the invariant that markers are in source order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceMap(Vec<SourceMarker>);

impl SourceMap {
    /// Maps an offset in the source to the corresponding offset in the target.
    pub fn map_offset(&self, offset: TextSize) -> TextSize {
        let Some(index) = self
            .0
            .partition_point(|marker| marker.source <= offset)
            .checked_sub(1)
        else {
            return offset;
        };
        let marker = &self.0[index];

        marker.dest + (offset - marker.source)
    }

    /// Returns a slice of all the markers in the source map in source order.
    pub fn markers(&self) -> &[SourceMarker] {
        &self.0
    }

    /// Push the start marker for an [`Edit`].
    ///
    /// The `output_length` is the length of the transformed string before the
    /// edit is applied.
    ///
    /// ## Panics
    ///
    /// If the start of `edit` is less than previous markers.
    pub fn push_start_marker(&mut self, edit: &Edit, output_length: TextSize) {
        self.push_marker(edit.start(), output_length);
    }

    /// Push the end marker for an [`Edit`].
    ///
    /// The `output_length` is the length of the transformed string after the
    /// edit has been applied.
    ///
    /// ## Panics
    ///
    /// If `edit` falls before previous markers.
    pub fn push_end_marker(&mut self, edit: &Edit, output_length: TextSize) {
        if edit.is_insertion() {
            self.push_marker(edit.start(), output_length);
        } else {
            // Deletion or replacement
            self.push_marker(edit.end(), output_length);
        }
    }

    /// Push a new marker to the source map.
    ///
    /// ## Panics
    ///
    /// If `source` is less than previous markers.
    pub fn push_marker(&mut self, source: TextSize, target: TextSize) {
        assert!(
            self.0.last().is_none_or(|last| source >= last.source),
            "Markers must be pushed in source order",
        );

        self.0.push(SourceMarker {
            source,
            dest: target,
        });
    }
}
