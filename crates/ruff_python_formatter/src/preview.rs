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
