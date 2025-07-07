//! Helpers to test if a specific preview style is enabled or not.
//!
//! The motivation for these functions isn't to avoid code duplication but to ease promoting preview behavior
//! to stable. The challenge with directly checking the `preview` attribute of [`LinterSettings`] is that it is unclear
//! which specific feature this preview check is for. Having named functions simplifies the promotion:
//! Simply delete the function and let Rust tell you which checks you have to remove.

use crate::settings::LinterSettings;

pub(crate) const fn is_py314_support_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/16565
pub(crate) const fn is_full_path_match_source_strategy_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// Rule-specific behavior

// https://github.com/astral-sh/ruff/pull/15541
pub(crate) const fn is_suspicious_function_reference_enabled(settings: &LinterSettings) -> bool {
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

// https://github.com/astral-sh/ruff/pull/16719
pub(crate) const fn is_fix_manual_dict_comprehension_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/13919
pub(crate) const fn is_fix_manual_list_comprehension_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/18763
pub(crate) const fn is_fix_os_path_getsize_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/11436
// https://github.com/astral-sh/ruff/pull/11168
pub(crate) const fn is_dunder_init_fix_unused_import_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/8473
pub(crate) const fn is_unicode_to_unicode_confusables_enabled(settings: &LinterSettings) -> bool {
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

// https://github.com/astral-sh/ruff/pull/18208
pub(crate) const fn is_multiple_with_statements_fix_safe_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/18400
pub(crate) const fn is_ignore_init_files_in_useless_alias_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/18572
pub(crate) const fn is_optional_as_none_in_union_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/18547
pub(crate) const fn is_invalid_async_mock_access_check_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/18867
pub(crate) const fn is_raise_exception_byte_string_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/18683
pub(crate) const fn is_safe_super_call_with_parameters_fix_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}
