//! Helpers to test if a specific preview style is enabled or not.
//!
//! The motivation for these functions isn't to avoid code duplication but to ease promoting preview behavior
//! to stable. The challenge with directly checking the `preview` attribute of [`LinterSettings`] is that it is unclear
//! which specific feature this preview check is for. Having named functions simplifies the promotion:
//! Simply delete the function and let Rust tell you which checks you have to remove.

use crate::settings::LinterSettings;

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

/// <https://github.com/astral-sh/ruff/pull/19303>
pub(crate) const fn is_fix_f_string_logging_enabled(settings: &LinterSettings) -> bool {
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
// https://github.com/astral-sh/ruff/pull/18922
pub(crate) const fn is_fix_os_path_getmtime_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/18922
pub(crate) const fn is_fix_os_path_getatime_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/18922
pub(crate) const fn is_fix_os_path_getctime_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_path_abspath_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_rmdir_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_unlink_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_remove_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_path_exists_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_path_expanduser_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_path_isdir_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_path_isfile_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_path_islink_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_path_isabs_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_readlink_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_path_basename_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19213
pub(crate) const fn is_fix_os_path_dirname_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19404
pub(crate) const fn is_fix_os_chmod_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19404
pub(crate) const fn is_fix_os_rename_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19404
pub(crate) const fn is_fix_os_replace_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19404
pub(crate) const fn is_fix_os_path_samefile_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19245
pub(crate) const fn is_fix_os_getcwd_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19514
pub(crate) const fn is_fix_os_mkdir_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19514
pub(crate) const fn is_fix_os_makedirs_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/20009
pub(crate) const fn is_fix_os_symlink_enabled(settings: &LinterSettings) -> bool {
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

// https://github.com/astral-sh/ruff/pull/18572
pub(crate) const fn is_optional_as_none_in_union_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/20659
pub(crate) const fn is_future_required_preview_generics_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/18683
pub(crate) const fn is_safe_super_call_with_parameters_fix_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19851
pub(crate) const fn is_maxsplit_without_separator_fix_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/20027
pub(crate) const fn is_unnecessary_default_type_args_stubs_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/20343
pub(crate) const fn is_sim910_expanded_key_support_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/20169
pub(crate) const fn is_fix_builtin_open_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/20178
pub(crate) const fn is_a003_class_scope_shadowing_expansion_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/20200
pub(crate) const fn is_refined_submodule_import_match_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/20660
pub(crate) const fn is_type_var_default_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// github.com/astral-sh/ruff/issues/20004
pub(crate) const fn is_b006_check_guaranteed_mutable_expr_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

// github.com/astral-sh/ruff/issues/20004
pub(crate) const fn is_b006_unsafe_fix_preserve_assignment_expr_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}

pub(crate) const fn is_typing_extensions_str_alias_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/19045
pub(crate) const fn is_extended_i18n_function_matching_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/21374
pub(crate) const fn is_extended_snmp_api_path_detection_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/21395
pub(crate) const fn is_enumerate_for_loop_int_index_enabled(settings: &LinterSettings) -> bool {
    settings.preview.is_enabled()
}

// https://github.com/astral-sh/ruff/pull/21469
pub(crate) const fn is_s310_resolve_string_literal_bindings_enabled(
    settings: &LinterSettings,
) -> bool {
    settings.preview.is_enabled()
}
