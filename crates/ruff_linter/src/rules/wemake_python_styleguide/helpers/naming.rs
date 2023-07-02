use std::path::Path;

const ALIAS_NAMES_ALLOWLIST: [&str; 7] = ["np", "pd", "df", "plt", "sns", "tf", "cv"];
const UNUSED_PLACEHOLDER: char = '_';

/// Checks whether the given ``name`` is unused.
fn is_unused(name: &str) -> bool {
    name.chars().all(|c| c == UNUSED_PLACEHOLDER)
}

/// Checks for too short names.
pub(in crate::rules::wemake_python_styleguide) fn is_too_short_name(
    name: &str,
    min_length: usize,
    trim: bool,
) -> bool {
    if ALIAS_NAMES_ALLOWLIST.contains(&name) {
        return false;
    }

    if is_unused(name) {
        return false;
    }

    let length = if trim {
        name.trim_matches(UNUSED_PLACEHOLDER).chars().count()
    } else {
        name.chars().count()
    };

    length < min_length
}

/// Return `true` if a [`Path`] should use the name of its parent directory as its
/// module name.
pub(in crate::rules::wemake_python_styleguide) fn is_module_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(std::ffi::OsStr::to_str),
        Some("__init__.py" | "__init__.pyi" | "__main__.py" | "__main__.pyi")
    )
}

#[cfg(test)]
mod tests {
    use super::{is_too_short_name, is_unused};

    #[test]
    fn test_is_too_short_name() {
        assert!(!is_too_short_name("test", 2, true));
        assert!(is_too_short_name("o", 2, true));
        assert!(!is_too_short_name("_", 2, true));
        assert!(!is_too_short_name("_", 1, true));
        assert!(!is_too_short_name("z1", 2, true));
        assert!(!is_too_short_name("z", 1, true));
        assert!(is_too_short_name("_z", 2, true));
        assert!(is_too_short_name("z_", 2, true));
        assert!(!is_too_short_name("z_", 2, false));
        assert!(is_too_short_name("__z", 2, true));
        assert!(!is_too_short_name("xy", 2, true));
        assert!(!is_too_short_name("np", 3, true));
    }

    #[test]
    fn test_is_unused() {
        assert!(is_unused("_"));
        assert!(is_unused("___"));
        assert!(!is_unused("_protected"));
        assert!(!is_unused("__private"));
    }
}
