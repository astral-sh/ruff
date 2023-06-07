use ruff_diagnostics::Edit;
use ruff_text_size::TextSize;

#[derive(Debug)]
pub struct SourceMarker {
    /// Position of the marker in the original source
    pub source: TextSize,
    /// Position of the marker in the output code
    pub dest: TextSize,
}

/// A collection of [`SourceMarker`].
#[derive(Default)]
pub struct SourceMap(Vec<SourceMarker>);

impl SourceMap {
    pub fn markers(&self) -> &[SourceMarker] {
        &self.0
    }

    pub fn push_start_marker(&mut self, edit: &Edit, output_length: TextSize) {
        self.0.push(SourceMarker {
            source: edit.start(),
            dest: output_length,
        });
    }

    pub fn push_end_marker(&mut self, edit: &Edit, output_length: TextSize) {
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

#[cfg(test)]
mod tests {
    use super::{SourceMap, SourceMarker};

    // TODO(dhruvmanila): Write tests
}
