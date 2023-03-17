use std::path::Path;

/// Return `true` if the [`Path`] appears to be that of a Python file.
pub fn is_python_file(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == "py" || ext == "pyi")
}

/// Return `true` if the [`Path`] appears to be that of a Python interface definition file (`.pyi`).
pub fn is_python_stub_file(path: &Path) -> bool {
    path.extension().map_or(false, |ext| ext == "pyi")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::path::is_python_file;

    #[test]
    fn inclusions() {
        let path = Path::new("foo/bar/baz.py");
        assert!(is_python_file(path));

        let path = Path::new("foo/bar/baz.pyi");
        assert!(is_python_file(path));

        let path = Path::new("foo/bar/baz.js");
        assert!(!is_python_file(path));

        let path = Path::new("foo/bar/baz");
        assert!(!is_python_file(path));
    }
}
