use std::num::NonZeroUsize;

use ruff_source_file::OneIndexed;

/// Calculate the length of the string representation of `value`
pub(super) fn calculate_print_width(mut value: OneIndexed) -> NonZeroUsize {
    const TEN: OneIndexed = OneIndexed::from_zero_indexed(9);

    let mut width = OneIndexed::ONE;

    while value >= TEN {
        value = OneIndexed::new(value.get() / 10).unwrap_or(OneIndexed::MIN);
        width = width.checked_add(1).unwrap();
    }

    width
}
