//! Interface that describes the output of the import resolver.

use std::path::PathBuf;

use crate::implicit_imports::ImplicitImports;
use crate::py_typed::PyTypedInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ImportResult {
    /// Whether the import name was relative (e.g., ".foo").
    pub(crate) is_relative: bool,

    /// Whether the import was resolved to a file or module.
    pub(crate) is_import_found: bool,

    /// The path was partially resolved, but the specific submodule
    /// defining the import was not found. For example, `foo.bar` was
    /// not found, but `foo` was found.
    pub(crate) is_partly_resolved: bool,

    /// The import refers to a namespace package (i.e., a folder without
    /// an `__init__.py[i]` file at the final level of resolution). By
    /// convention, we insert empty `PathBuf` segments into the resolved
    /// paths vector to indicate intermediary namespace packages.
    pub(crate) is_namespace_package: bool,

    /// The final resolved directory contains an `__init__.py[i]` file.
    pub(crate) is_init_file_present: bool,

    /// The import resolved to a stub (`.pyi`) file within a stub package.
    pub(crate) is_stub_package: bool,

    /// The import resolved to a built-in, local, or third-party module.
    pub(crate) import_type: ImportType,

    /// A vector of resolved absolute paths for each file in the module
    /// name. Typically includes a sequence of `__init__.py` files, followed
    /// by the Python file defining the import itself, though the exact
    /// structure can vary. For example, namespace packages will be represented
    /// by empty `PathBuf` segments in the vector.
    ///
    /// For example, resolving `import foo.bar` might yield `./foo/__init__.py` and `./foo/bar.py`,
    /// or `./foo/__init__.py` and `./foo/bar/__init__.py`.
    pub(crate) resolved_paths: Vec<PathBuf>,

    /// The search path used to resolve the module.
    pub(crate) search_path: Option<PathBuf>,

    /// The resolved file is a type hint (i.e., a `.pyi` file), rather
    /// than a Python (`.py`) file.
    pub(crate) is_stub_file: bool,

    /// The resolved file is a native library.
    pub(crate) is_native_lib: bool,

    /// The resolved file is a hint hint (i.e., a `.pyi` file) from
    /// `typeshed` in the standard library.
    pub(crate) is_stdlib_typeshed_file: bool,

    /// The resolved file is a hint hint (i.e., a `.pyi` file) from
    /// `typeshed` in third-party stubs.
    pub(crate) is_third_party_typeshed_file: bool,

    /// The resolved file is a type hint (i.e., a `.pyi` file) from
    /// the configured typing directory.
    pub(crate) is_local_typings_file: bool,

    /// A map from file to resolved path, for all implicitly imported
    /// modules that are part of a namespace package.
    pub(crate) implicit_imports: ImplicitImports,

    /// Any implicit imports whose symbols were explicitly imported (i.e., via
    /// a `from x import y` statement).
    pub(crate) filtered_implicit_imports: ImplicitImports,

    /// If the import resolved to a type hint (i.e., a `.pyi` file), then
    /// a non-type-hint resolution will be stored here.
    #[allow(clippy::struct_field_names)]
    pub(crate) non_stub_import_result: Option<Box<ImportResult>>,

    /// Information extracted from the `py.typed` in the package used to
    /// resolve the import, if any.
    pub(crate) py_typed_info: Option<PyTypedInfo>,

    /// The directory of the package, if any.
    pub(crate) package_directory: Option<PathBuf>,
}

impl ImportResult {
    /// An import result that indicates that the import was not found.
    pub(crate) fn not_found() -> Self {
        Self {
            is_relative: false,
            is_import_found: false,
            is_partly_resolved: false,
            is_namespace_package: false,
            is_init_file_present: false,
            is_stub_package: false,
            import_type: ImportType::Local,
            resolved_paths: vec![],
            search_path: None,
            is_stub_file: false,
            is_native_lib: false,
            is_stdlib_typeshed_file: false,
            is_third_party_typeshed_file: false,
            is_local_typings_file: false,
            implicit_imports: ImplicitImports::default(),
            filtered_implicit_imports: ImplicitImports::default(),
            non_stub_import_result: None,
            py_typed_info: None,
            package_directory: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportType {
    BuiltIn,
    ThirdParty,
    Local,
}
