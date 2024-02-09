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

/// Returns `true` if the [`parenthesize_long_type_hints`](https://github.com/astral-sh/ruff/issues/8894) preview style is enabled.
pub(crate) const fn is_parenthesize_long_type_hints_enabled(context: &PyFormatContext) -> bool {
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

/// Returns `true` if the [`wrap_multiple_context_managers_in_parens`](https://github.com/astral-sh/ruff/issues/8889) preview style is enabled.
///
/// Unlike Black, we re-use the same preview style feature flag for [`improved_async_statements_handling`](https://github.com/astral-sh/ruff/issues/8890)
pub(crate) const fn is_wrap_multiple_context_managers_in_parens_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`blank_line_after_nested_stub_class`](https://github.com/astral-sh/ruff/issues/8891) preview style is enabled.
pub(crate) const fn is_blank_line_after_nested_stub_class_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`module_docstring_newlines`](https://github.com/astral-sh/ruff/issues/7995) preview style is enabled.
pub(crate) const fn is_module_docstring_newlines_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`dummy_implementations`](https://github.com/astral-sh/ruff/issues/8357) preview style is enabled.
pub(crate) const fn is_dummy_implementations_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`hex_codes_in_unicode_sequences`](https://github.com/psf/black/pull/2916) preview style is enabled.
pub(crate) const fn is_hex_codes_in_unicode_sequences_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`multiline_string_handling`](https://github.com/astral-sh/ruff/issues/8896) preview style is enabled.
pub(crate) const fn is_multiline_string_handling_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`multiline_string_handling`](https://github.com/astral-sh/ruff/pull/9725) preview style is enabled.
/// Black does not [`format docstrings`](https://github.com/psf/black/issues/3493) so we keep this
/// preview for compatibility with Black.
pub(crate) const fn is_format_module_docstring_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}
