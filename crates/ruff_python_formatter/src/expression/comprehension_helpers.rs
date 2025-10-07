use ruff_text_size::Ranged;

use crate::prelude::*;

/// Determines whether a comprehension expression spans multiple lines in the source code.
pub(crate) fn is_comprehension_multiline(range: &impl Ranged, context: &PyFormatContext) -> bool {
    let source = context.source();
    let start = range.start();
    let end = range.end();

    // Get the source text for this comprehension
    let text = &source[start.to_usize()..end.to_usize()];

    // Check if it contains any newline characters
    text.contains('\n')
}