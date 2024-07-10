use std::path::Path;

use ruff_linter::settings::LinterSettings;
use ruff_workspace::resolver::{match_any_exclusion, match_any_inclusion};
use ruff_workspace::{FileResolverSettings, FormatterSettings};

/// Return `true` if the document at the given [`Path`] should be excluded.
///
/// The tool-specific settings should be provided if the request for the document is specific to
/// that tool. For example, a diagnostics request should provide the linter settings while the
/// formatting request should provide the formatter settings.
///
/// The logic for the resolution considers both inclusion and exclusion and is as follows:
/// 1. Check for global `exclude` and `extend-exclude` options along with tool specific `exclude`
///    option (`lint.exclude`, `format.exclude`).
/// 2. Check for global `include` and `extend-include` options.
pub(crate) fn is_document_excluded(
    path: &Path,
    resolver_settings: &FileResolverSettings,
    linter_settings: Option<&LinterSettings>,
    formatter_settings: Option<&FormatterSettings>,
) -> bool {
    if let Some(exclusion) = match_any_exclusion(
        path,
        &resolver_settings.exclude,
        &resolver_settings.extend_exclude,
        linter_settings.map(|s| &*s.exclude),
        formatter_settings.map(|s| &*s.exclude),
    ) {
        tracing::debug!("Ignored path via `{}`: {}", exclusion, path.display());
        return true;
    }

    if let Some(inclusion) = match_any_inclusion(
        path,
        &resolver_settings.include,
        &resolver_settings.extend_include,
    ) {
        tracing::debug!("Included path via `{}`: {}", inclusion, path.display());
        false
    } else {
        // Path is excluded by not being in the inclusion set.
        true
    }
}
