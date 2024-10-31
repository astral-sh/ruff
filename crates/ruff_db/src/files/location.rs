use crate::files::File;
use ruff_text_size::TextRange;

/// A location inside a file within a workspace
/// (This can cover a text range)
pub struct Location {
    pub file: File,
    pub range: TextRange,
}
