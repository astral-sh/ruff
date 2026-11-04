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

/// Returns `true` if the
/// [`fluent_layout_split_first_call`](https://github.com/astral-sh/ruff/pull/21369) preview
/// style is enabled.
pub(crate) const fn is_fluent_layout_split_first_call_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Removes redundant parentheses around list displays and list comprehensions.
pub(crate) const fn is_remove_redundant_list_parentheses_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Removes redundant parentheses around generator expressions.
pub(crate) const fn is_remove_redundant_generator_parentheses_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Keeps simple power operators unspaced when their expression wraps.
pub(crate) const fn is_simplify_power_operator_hugging_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Parenthesizes the yielded tuple instead of the whole yield statement.
pub(crate) const fn is_parenthesize_tuple_in_yield_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Removes redundant parentheses around expression statements.
pub(crate) const fn is_remove_redundant_top_level_parentheses_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Removes redundant parentheses around subscript indices.
pub(crate) const fn is_remove_redundant_subscript_parentheses_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Removes redundant parentheses from assignment and unpacking targets.
pub(crate) const fn is_remove_redundant_unpacking_parentheses_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Removes redundant parentheses around simple comparison operands.
pub(crate) const fn is_remove_redundant_comparison_parentheses_enabled(
    context: &PyFormatContext,
) -> bool {
    context.is_preview()
}

/// Wraps long comprehension iterables when parentheses make them fit.
pub(crate) const fn is_wrap_comprehension_in_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Keeps a lone comparison operator with its left operand when the right side wraps.
pub(crate) const fn is_hug_comparator_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}

/// Keeps operators together when a following dictionary provides the line breaks.
pub(crate) const fn is_keep_dict_keys_with_operators_enabled(context: &PyFormatContext) -> bool {
    context.is_preview()
}
