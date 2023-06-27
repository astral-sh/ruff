use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{native_module, py_typed};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImplicitImport {
    /// Whether the implicit import is a stub file.
    pub(crate) is_stub_file: bool,

    /// Whether the implicit import is a native module.
    pub(crate) is_native_lib: bool,

    /// The name of the implicit import (e.g., `os`).
    pub(crate) name: String,

    /// The path to the implicit import.
    pub(crate) path: PathBuf,

    /// The `py.typed` information for the implicit import, if any.
    pub(crate) py_typed: Option<py_typed::PyTypedInfo>,
}

/// Find the "implicit" imports within the namespace package at the given path.
pub(crate) fn find(dir_path: &Path, exclusions: &[&Path]) -> BTreeMap<String, ImplicitImport> {
    let mut implicit_imports = BTreeMap::new();

    // Enumerate all files and directories in the path, expanding links.
    let Ok(entries) = fs::read_dir(dir_path) else {
        return implicit_imports;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if exclusions.contains(&path.as_path()) {
            continue;
        }

        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        // TODO(charlie): Support symlinks.
        if file_type.is_file() {
            // Add implicit file-based modules.
            let Some(extension) = path.extension() else {
                continue;
            };

            let (file_stem, is_native_lib) = if extension == "py" || extension == "pyi" {
                // E.g., `foo.py` becomes `foo`.
                let file_stem = path.file_stem().and_then(OsStr::to_str);
                let is_native_lib = false;
                (file_stem, is_native_lib)
            } else if native_module::is_native_module_file_extension(extension)
                && !path
                    .with_extension(format!("{}.py", extension.to_str().unwrap()))
                    .exists()
                && !path
                    .with_extension(format!("{}.pyi", extension.to_str().unwrap()))
                    .exists()
            {
                // E.g., `foo.abi3.so` becomes `foo`.
                let file_stem = native_module::native_module_name(&path);
                let is_native_lib = true;
                (file_stem, is_native_lib)
            } else {
                continue;
            };

            let Some(name) = file_stem else {
                continue;
            };

            let implicit_import = ImplicitImport {
                is_stub_file: extension == "pyi",
                is_native_lib,
                name: name.to_string(),
                path: path.clone(),
                py_typed: None,
            };

            // Always prefer stub files over non-stub files.
            if implicit_imports
                .get(&implicit_import.name)
                .map_or(true, |implicit_import| !implicit_import.is_stub_file)
            {
                implicit_imports.insert(implicit_import.name.clone(), implicit_import);
            }
        } else if file_type.is_dir() {
            // Add implicit directory-based modules.
            let py_file_path = path.join("__init__.py");
            let pyi_file_path = path.join("__init__.pyi");

            let (path, is_stub_file) = if py_file_path.exists() {
                (py_file_path, false)
            } else if pyi_file_path.exists() {
                (pyi_file_path, true)
            } else {
                continue;
            };

            let Some(name) = path.file_name().and_then(OsStr::to_str) else {
                continue;
            };

            let implicit_import = ImplicitImport {
                is_stub_file,
                is_native_lib: false,
                name: name.to_string(),
                path: path.clone(),
                py_typed: py_typed::get_py_typed_info(&path),
            };
            implicit_imports.insert(implicit_import.name.clone(), implicit_import);
        }
    }

    implicit_imports
}

/// Filter a map of implicit imports to only include those that were actually imported.
pub(crate) fn filter(
    implicit_imports: &BTreeMap<String, ImplicitImport>,
    imported_symbols: &[String],
) -> Option<BTreeMap<String, ImplicitImport>> {
    if implicit_imports.is_empty() || imported_symbols.is_empty() {
        return None;
    }

    let mut filtered_imports = BTreeMap::new();
    for implicit_import in implicit_imports.values() {
        if imported_symbols.contains(&implicit_import.name) {
            filtered_imports.insert(implicit_import.name.clone(), implicit_import.clone());
        }
    }

    if filtered_imports.len() == implicit_imports.len() {
        return None;
    }

    Some(filtered_imports)
}
