//! Helpers to test if a specific preview style is enabled or not.
//!
//! The motivation for these functions isn't to avoid code duplication but to ease promoting preview behavior
//! to stable. The challenge with directly checking the `preview` attribute of [`LinterSettings`] is that it is unclear
//! which specific feature this preview check is for. Having named functions simplifies the promotion:
//! Simply delete the function and let Rust tell you which checks you have to remove.

use crate::settings::LinterSettings;

// https://github.com/astral-sh/ruff/issues/17412
// https://github.com/astral-sh/ruff/issues/11934
pub(crate) const fn is_semantic_errors_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/16429
pub(crate) const fn is_unsupported_syntax_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

pub(crate) const fn is_py314_support_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/16565
pub(crate) const fn is_full_path_match_source_strategy_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// Rule-specific behavior

// https://github.com/astral-sh/ruff/pull/17136
pub(crate) const fn is_shell_injection_only_trusted_input_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/15541
pub(crate) const fn is_suspicious_function_reference_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/7501
pub(crate) const fn is_bool_subtype_of_annotation_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/10759
pub(crate) const fn is_comprehension_with_min_max_sum_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/12657
pub(crate) const fn is_check_comprehensions_in_tuple_call_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/issues/15347
pub(crate) const fn is_bad_version_info_in_non_stub_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/12676
pub(crate) const fn is_fix_future_annotations_in_stub_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/11074
pub(crate) const fn is_only_add_return_none_at_end_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/12796
pub(crate) const fn is_simplify_ternary_to_binary_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/16719
pub(crate) const fn is_fix_manual_dict_comprehension_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/13919
pub(crate) const fn is_fix_manual_list_comprehension_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/11436
// https://github.com/astral-sh/ruff/pull/11168
pub(crate) const fn is_dunder_init_fix_unused_import_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/15313
pub(crate) const fn is_defer_optional_to_up045_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/8473
pub(crate) const fn is_unicode_to_unicode_confusables_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/17078
pub(crate) const fn is_support_slices_in_literal_concatenation_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/11370
pub(crate) const fn is_undefined_export_in_dunder_init_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/14236
pub(crate) const fn is_allow_nested_roots_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/17061
pub(crate) const fn is_check_file_level_directives_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/17644
pub(crate) const fn is_readlines_in_for_fix_safe_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

pub(crate) const fn multiple_with_statements_fix_safe_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}
