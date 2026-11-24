use std::ffi::OsStr;
use std::path::Path;

/// Return `true` if a [`Path`] should use the name of its parent directory as its module name.
pub fn is_module_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(OsStr::to_str),
        Some("__init__.py" | "__init__.pyi" | "__main__.py" | "__main__.pyi")
    )
}
