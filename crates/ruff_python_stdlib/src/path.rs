use std::path::Path;

/// Return `true` if the [`Path`] appears to be that of a Python file.
pub fn is_python_file(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == "py" || ext == "pyi")
}

/// Return `true` if the [`Path`] is named `pyproject.toml`.
pub fn is_project_toml(path: &Path) -> bool {
    path.file_name()
        .map_or(false, |name| name == "pyproject.toml")
}

/// Return `true` if the [`Path`] appears to be that of a Python interface definition file (`.pyi`).
pub fn is_python_stub_file(path: &Path) -> bool {
    path.extension().map_or(false, |ext| ext == "pyi")
}

/// Return `true` if the [`Path`] appears to be that of a Jupyter notebook (`.ipynb`).
pub fn is_jupyter_notebook(path: &Path) -> bool {
    path.extension().map_or(false, |ext| ext == "ipynb")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::path::{is_jupyter_notebook, is_python_file};

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

    #[test]
    fn test_is_jupyter_notebook() {
        let path = Path::new("foo/bar/baz.ipynb");
        assert!(is_jupyter_notebook(path));

        let path = Path::new("foo/bar/baz.py");
        assert!(!is_jupyter_notebook(path));
    }
}
