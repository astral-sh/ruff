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

/// Returns `true` if the [`no_chaperone_for_escaped_quote_in_triple_quoted_docstring`](https://github.com/astral-sh/ruff/pull/17216) preview style is enabled.
pub(crate) const fn is_no_chaperone_for_escaped_quote_in_triple_quoted_docstring_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Returns `true` if the [`blank_line_before_decorated_class_in_stub`](https://github.com/astral-sh/ruff/issues/18865) preview style is enabled.
pub(crate) const fn is_blank_line_before_decorated_class_in_stub_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Returns `true` if the
/// [`remove_parens_around_except_types`](https://github.com/astral-sh/ruff/pull/20768) preview
/// style is enabled.
pub(crate) const fn is_remove_parens_around_except_types_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Returns `true` if the
/// [`allow_newline_after_block_open`](https://github.com/astral-sh/ruff/pull/21110) preview style
/// is enabled.
pub(crate) const fn is_allow_newline_after_block_open_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Returns `true` if the
/// [`avoid_parens_for_long_as_captures`](https://github.com/astral-sh/ruff/pull/21176) preview
/// style is enabled.
pub(crate) const fn is_avoid_parens_for_long_as_captures_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Returns `true` if the
/// [`parenthesize_lambda_bodies`](https://github.com/astral-sh/ruff/pull/21385) preview style is
/// enabled.
pub(crate) const fn is_parenthesize_lambda_bodies_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Returns `true` if the
/// [`fluent_layout_split_first_call`](https://github.com/astral-sh/ruff/pull/21369) preview
/// style is enabled.
pub(crate) const fn is_fluent_layout_split_first_call_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}
