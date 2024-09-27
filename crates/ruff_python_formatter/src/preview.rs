//! Helpers to test if a specific preview style is enabled or not.
//!
//! The motivation for these functions isn't to avoid code duplication but to ease promoting preview styles
//! to stable. The challenge with directly using [`is_preview`](PyFormatContext::is_preview) is that it is unclear
//! for which specific feature this preview check is for. Having named functions simplifies the promotion:
//! Simply delete the function and let Rust tell you which checks you have to remove.

use crate::PyFormatContext;

/// Returns `true` if the [`hug_parens_with_braces_and_square_brackets`](https://github.com/astral-sh/ruff/issues/8279) preview style is enabled.
pub(crate) const fn is_hug_parens_with_braces_and_square_brackets_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`f-string formatting`](https://github.com/astral-sh/ruff/issues/7594) preview style is enabled.
pub(crate) fn is_f_string_formatting_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

pub(crate) fn is_with_single_item_pre_39_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// See [#12282](https://github.com/astral-sh/ruff/pull/12282).
pub(crate) fn is_comprehension_leading_expression_comments_same_line_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// See [#9447](https://github.com/astral-sh/ruff/issues/9447)
pub(crate) fn is_empty_parameters_no_unnecessary_parentheses_around_return_value_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// See [#6933](https://github.com/astral-sh/ruff/issues/6933).
/// This style also covers the black preview styles `remove_redundant_guard_parens` and `parens_for_long_if_clauses_in_case_block `.
/// WARNING: This preview style depends on `is_empty_parameters_no_unnecessary_parentheses_around_return_value_enabled`
/// because it relies on the new semantic of `IfBreaksParenthesized`.
pub(crate) fn is_match_case_parentheses_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// This preview style fixes a bug with the docstring's `line-length` calculation when using the `dynamic` mode.
/// The new style now respects the indent **inside** the docstring and reduces the `line-length` accordingly
/// so that the docstring's code block fits into the global line-length setting.
pub(crate) fn is_docstring_code_block_in_docstring_indent_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}
