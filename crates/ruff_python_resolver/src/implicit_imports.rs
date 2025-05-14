use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use crate::{native_module, py_typed};

/// A map of the submodules that are present in a namespace package.
///
/// Namespace packages lack an `__init__.py` file. So when resolving symbols from a namespace
/// package, the symbols must be present as submodules. This map contains the submodules that are
/// present in the namespace package, keyed by their module name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ImplicitImports(BTreeMap<String, ImplicitImport>);

impl ImplicitImports {
    /// Find the "implicit" imports within the namespace package at the given path.
    pub(crate) fn find(dir_path: &Path, exclusions: &[&Path]) -> io::Result<Self> {
        let mut submodules: BTreeMap<String, ImplicitImport> = BTreeMap::new();

        // Enumerate all files and directories in the path, expanding links.
        for entry in dir_path.read_dir()?.flatten() {
            let file_type = entry.file_type()?;

            let path = entry.path();
            if exclusions.contains(&path.as_path()) {
                continue;
            }

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
                } else if native_module::is_native_module_file_extension(extension) {
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

                // Always prefer stub files over non-stub files.
                if submodules
                    .get(name)
                    .is_none_or(|implicit_import| !implicit_import.is_stub_file)
                {
                    submodules.insert(
                        name.to_string(),
                        ImplicitImport {
                            is_stub_file: extension == "pyi",
                            is_native_lib,
                            path,
                            py_typed: None,
                        },
                    );
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
                submodules.insert(
                    name.to_string(),
                    ImplicitImport {
                        is_stub_file,
                        is_native_lib: false,
                        py_typed: py_typed::get_py_typed_info(&path),
                        path,
                    },
                );
            }
        }

        Ok(Self(submodules))
    }

    /// Filter [`ImplicitImports`] to only those symbols that were imported.
    pub(crate) fn filter(&self, imported_symbols: &[String]) -> Option<Self> {
        if self.is_empty() || imported_symbols.is_empty() {
            return None;
        }

        let filtered: BTreeMap<String, ImplicitImport> = self
            .iter()
            .filter(|(name, _)| imported_symbols.contains(name))
            .map(|(name, implicit_import)| (name.clone(), implicit_import.clone()))
            .collect();

        if filtered.len() == self.len() {
            return None;
        }

        Some(Self(filtered))
    }

    /// Returns `true` if the [`ImplicitImports`] resolves all the symbols requested by a
    /// module descriptor.
    pub(crate) fn resolves_namespace_package(&self, imported_symbols: &[String]) -> bool {
        if !imported_symbols.is_empty() {
            // TODO(charlie): Pyright uses:
            //
            // ```typescript
            // !Array.from(moduleDescriptor.importedSymbols.keys()).some((symbol) => implicitImports.has(symbol))`
            // ```
            //
            // However, that only checks if _any_ of the symbols are in the implicit imports.
            for symbol in imported_symbols {
                if !self.has(symbol) {
                    return false;
                }
            }
        } else if self.is_empty() {
            return false;
        }
        true
    }

    /// Returns `true` if the module is present in the namespace package.
    pub(crate) fn has(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Returns the number of implicit imports in the namespace package.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if there are no implicit imports in the namespace package.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over the implicit imports in the namespace package.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &ImplicitImport)> {
        self.0.iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImplicitImport {
    /// Whether the implicit import is a stub file.
    pub(crate) is_stub_file: bool,

    /// Whether the implicit import is a native module.
    pub(crate) is_native_lib: bool,

    /// The path to the implicit import.
    pub(crate) path: PathBuf,

    /// The `py.typed` information for the implicit import, if any.
    pub(crate) py_typed: Option<py_typed::PyTypedInfo>,
}
