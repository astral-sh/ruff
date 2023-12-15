//! Helpers to test if a specific preview style is enabled or not.
//!
//! The motivation for these functions isn't to avoid code duplication but to ease promoting preview styles
//! to stable. The challenge with directly using [`is_preview`](PyFormatContext::is_preview) is that it is unclear
//! for which specific feature this preview check is for. Having named functions simplifies the promotion:
//! Simply delete the function and let Rust tell you which checks you have to remove.
use crate::PyFormatContext;

/// Returns `true` if the [`fix_power_op_line_length`](https://github.com/astral-sh/ruff/issues/8938) preview style is enabled.
pub(crate) const fn is_fix_power_op_line_length_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`hug_parens_with_braces_and_square_brackets`](https://github.com/astral-sh/ruff/issues/8279) preview style is enabled.
pub(crate) const fn is_hug_parens_with_braces_and_square_brackets_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`prefer_splitting_right_hand_side_of_assignments`](https://github.com/astral-sh/ruff/issues/6975) preview style is enabled.
pub(crate) const fn is_prefer_splitting_right_hand_side_of_assignments_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`no_blank_line_before_class_docstring`] preview style is enabled.
///
/// [`no_blank_line_before_class_docstring`]: https://github.com/astral-sh/ruff/issues/8888
pub(crate) const fn is_no_blank_line_before_class_docstring_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}
