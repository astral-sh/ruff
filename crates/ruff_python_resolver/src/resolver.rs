//! Resolves Python imports to their corresponding files on disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use log::debug;

use crate::config::Config;
use crate::execution_environment::ExecutionEnvironment;
use crate::implicit_imports::ImplicitImport;
use crate::import_result::{ImportResult, ImportType};
use crate::module_descriptor::ImportModuleDescriptor;
use crate::{host, implicit_imports, native_module, py_typed, search};

#[allow(clippy::fn_params_excessive_bools)]
fn resolve_module_descriptor(
    root: &Path,
    module_descriptor: &ImportModuleDescriptor,
    allow_partial: bool,
    allow_native_lib: bool,
    use_stub_package: bool,
    allow_pyi: bool,
    look_for_py_typed: bool,
) -> ImportResult {
    if use_stub_package {
        debug!("Attempting to resolve stub package using root path: {root:?}");
    } else {
        debug!("Attempting to resolve using root path: {root:?}");
    }

    // Starting at the specified path, walk the file system to find the specified module.
    let mut resolved_paths: Vec<PathBuf> = Vec::new();
    let mut dir_path = root.to_path_buf();
    let mut is_namespace_package = false;
    let mut is_init_file_present = false;
    let mut is_stub_package = false;
    let mut is_stub_file = false;
    let mut is_native_lib = false;
    let mut implicit_imports = BTreeMap::new();
    let mut package_directory = None;
    let mut py_typed_info = None;

    // Ex) `from . import foo`
    if module_descriptor.name_parts.is_empty() {
        let py_file_path = dir_path.join("__init__.py");
        let pyi_file_path = dir_path.join("__init__.pyi");

        if allow_pyi && pyi_file_path.is_file() {
            debug!("Resolved import with file: {pyi_file_path:?}");
            resolved_paths.push(pyi_file_path.clone());
        } else if py_file_path.is_file() {
            debug!("Resolved import with file: {py_file_path:?}");
            resolved_paths.push(py_file_path.clone());
        } else {
            debug!("Partially resolved import with directory: {dir_path:?}");

            // Add an empty path to indicate that the import is partially resolved.
            resolved_paths.push(PathBuf::new());
            is_namespace_package = true;
        }

        implicit_imports = implicit_imports::find(&dir_path, &[&py_file_path, &pyi_file_path]);
    } else {
        for (i, part) in module_descriptor.name_parts.iter().enumerate() {
            let is_first_part = i == 0;
            let is_last_part = i == module_descriptor.name_parts.len() - 1;

            // Extend the directory path with the next segment.
            if use_stub_package && is_first_part {
                dir_path = dir_path.join(format!("{part}-stubs"));
                is_stub_package = true;
            } else {
                dir_path = dir_path.join(part);
            }

            let found_directory = dir_path.is_dir();
            if found_directory {
                if is_first_part {
                    package_directory = Some(dir_path.clone());
                }

                // Look for an `__init__.py[i]` in the directory.
                let py_file_path = dir_path.join("__init__.py");
                let pyi_file_path = dir_path.join("__init__.pyi");
                is_init_file_present = false;

                if allow_pyi && pyi_file_path.is_file() {
                    debug!("Resolved import with file: {pyi_file_path:?}");
                    resolved_paths.push(pyi_file_path.clone());
                    if is_last_part {
                        is_stub_file = true;
                    }
                    is_init_file_present = true;
                } else if py_file_path.is_file() {
                    debug!("Resolved import with file: {py_file_path:?}");
                    resolved_paths.push(py_file_path.clone());
                    is_init_file_present = true;
                }

                if look_for_py_typed {
                    py_typed_info =
                        py_typed_info.or_else(|| py_typed::get_py_typed_info(&dir_path));
                }

                // We haven't reached the end of the import, and we found a matching directory.
                // Proceed to the next segment.
                if !is_last_part {
                    if !is_init_file_present {
                        resolved_paths.push(PathBuf::new());
                        is_namespace_package = true;
                        py_typed_info = None;
                    }
                    continue;
                }

                if is_init_file_present {
                    implicit_imports =
                        implicit_imports::find(&dir_path, &[&py_file_path, &pyi_file_path]);
                    break;
                }
            }

            // We couldn't find a matching directory, or the directory didn't contain an
            // `__init__.py[i]` file. Look for an `.py[i]` file with the same name as the
            // segment, in lieu of a directory.
            let py_file_path = dir_path.with_extension("py");
            let pyi_file_path = dir_path.with_extension("pyi");

            if allow_pyi && pyi_file_path.is_file() {
                debug!("Resolved import with file: {pyi_file_path:?}");
                resolved_paths.push(pyi_file_path);
                if is_last_part {
                    is_stub_file = true;
                }
            } else if py_file_path.is_file() {
                debug!("Resolved import with file: {py_file_path:?}");
                resolved_paths.push(py_file_path);
            } else {
                if allow_native_lib && dir_path.is_dir() {
                    // We couldn't find a `.py[i]` file; search for a native library.
                    if let Some(native_lib_path) = dir_path
                        .read_dir()
                        .unwrap()
                        .flatten()
                        .filter(|entry| entry.file_type().map_or(false, |ft| ft.is_file()))
                        .find(|entry| {
                            native_module::is_native_module_file_name(&dir_path, &entry.path())
                        })
                    {
                        debug!("Resolved import with file: {native_lib_path:?}");
                        is_native_lib = true;
                        resolved_paths.push(native_lib_path.path());
                    }
                }

                if !is_native_lib && found_directory {
                    debug!("Partially resolved import with directory: {dir_path:?}");
                    resolved_paths.push(PathBuf::new());
                    if is_last_part {
                        implicit_imports =
                            implicit_imports::find(&dir_path, &[&py_file_path, &pyi_file_path]);
                        is_namespace_package = true;
                    }
                } else if is_native_lib {
                    debug!("Did not find file {py_file_path:?} or {pyi_file_path:?}");
                }
            }
            break;
        }
    }

    let import_found = if allow_partial {
        !resolved_paths.is_empty()
    } else {
        resolved_paths.len() == module_descriptor.name_parts.len()
    };

    let is_partly_resolved =
        !resolved_paths.is_empty() && resolved_paths.len() < module_descriptor.name_parts.len();

    ImportResult {
        is_relative: false,
        is_import_found: import_found,
        is_partly_resolved,
        is_namespace_package,
        is_init_file_present,
        is_stub_package,
        import_type: ImportType::Local,
        resolved_paths,
        search_path: Some(root.into()),
        is_stub_file,
        is_native_lib,
        is_stdlib_typeshed_file: false,
        is_third_party_typeshed_file: false,
        is_local_typings_file: false,
        implicit_imports,
        filtered_implicit_imports: BTreeMap::default(),
        non_stub_import_result: None,
        py_typed_info,
        package_directory,
    }
}

/// Resolve an absolute module import based on the import resolution algorithm
/// defined in [PEP 420].
///
/// [PEP 420]: https://peps.python.org/pep-0420/
#[allow(clippy::fn_params_excessive_bools)]
fn resolve_absolute_import(
    root: &Path,
    module_descriptor: &ImportModuleDescriptor,
    allow_partial: bool,
    allow_native_lib: bool,
    use_stub_package: bool,
    allow_pyi: bool,
    look_for_py_typed: bool,
) -> ImportResult {
    if allow_pyi && use_stub_package {
        // Search for packaged stubs first. PEP 561 indicates that package authors can ship
        // stubs separately from the package implementation by appending `-stubs` to its
        // top-level directory name.
        let import_result = resolve_module_descriptor(
            root,
            module_descriptor,
            allow_partial,
            false,
            true,
            true,
            true,
        );

        if import_result.package_directory.is_some() {
            // If this is a namespace package that wasn't resolved, assume that
            // it's a partial stub package and continue looking for a real package.
            if !import_result.is_namespace_package || import_result.is_import_found {
                return import_result;
            }
        }
    }

    // Search for a "real" package.
    resolve_module_descriptor(
        root,
        module_descriptor,
        allow_partial,
        allow_native_lib,
        false,
        allow_pyi,
        look_for_py_typed,
    )
}

/// Resolve an absolute module import based on the import resolution algorithm,
/// taking into account the various competing files to which the import could
/// resolve.
///
/// For example, prefers local imports over third-party imports, and stubs over
/// non-stubs.
fn resolve_best_absolute_import<Host: host::Host>(
    execution_environment: &ExecutionEnvironment,
    module_descriptor: &ImportModuleDescriptor,
    allow_pyi: bool,
    config: &Config,
    host: &Host,
) -> Option<ImportResult> {
    let import_name = module_descriptor.name();

    // Search for local stub files (using `stub_path`).
    if allow_pyi {
        if let Some(stub_path) = config.stub_path.as_ref() {
            debug!("Looking in stub path: {}", stub_path.display());

            let mut typings_import = resolve_absolute_import(
                stub_path,
                module_descriptor,
                false,
                false,
                true,
                allow_pyi,
                false,
            );

            if typings_import.is_import_found {
                // Treat stub files as "local".
                typings_import.import_type = ImportType::Local;
                typings_import.is_local_typings_file = true;

                // If we resolved to a namespace package, ensure that all imported symbols are
                // present in the namespace package's "implicit" imports.
                if typings_import.is_namespace_package
                    && typings_import.resolved_paths[typings_import.resolved_paths.len() - 1]
                        .as_os_str()
                        .is_empty()
                {
                    if is_namespace_package_resolved(
                        module_descriptor,
                        &typings_import.implicit_imports,
                    ) {
                        return Some(typings_import);
                    }
                } else {
                    return Some(typings_import);
                }
            }

            return None;
        }
    }

    // Look in the root directory of the execution environment.
    debug!(
        "Looking in root directory of execution environment: {}",
        execution_environment.root.display()
    );

    let mut local_import = resolve_absolute_import(
        &execution_environment.root,
        module_descriptor,
        false,
        true,
        true,
        allow_pyi,
        false,
    );
    local_import.import_type = ImportType::Local;

    let mut best_result_so_far = Some(local_import);

    // Look in any extra paths.
    for extra_path in &execution_environment.extra_paths {
        debug!("Looking in extra path: {}", extra_path.display());

        let mut local_import = resolve_absolute_import(
            extra_path,
            module_descriptor,
            false,
            true,
            true,
            allow_pyi,
            false,
        );
        local_import.import_type = ImportType::Local;

        best_result_so_far = Some(pick_best_import(
            best_result_so_far,
            local_import,
            module_descriptor,
        ));
    }

    // Look for third-party imports in Python's `sys` path.
    for search_path in search::python_search_paths(config, host) {
        debug!("Looking in Python search path: {}", search_path.display());

        let mut third_party_import = resolve_absolute_import(
            &search_path,
            module_descriptor,
            false,
            true,
            true,
            allow_pyi,
            true,
        );
        third_party_import.import_type = ImportType::ThirdParty;

        best_result_so_far = Some(pick_best_import(
            best_result_so_far,
            third_party_import,
            module_descriptor,
        ));
    }

    // If a library is fully `py.typed`, prefer the current result. There's one exception:
    // we're executing from `typeshed` itself. In that case, use the `typeshed` lookup below,
    // rather than favoring `py.typed` libraries.
    if let Some(typeshed_root) = search::typeshed_root(config, host) {
        debug!(
            "Looking in typeshed root directory: {}",
            typeshed_root.display()
        );
        if typeshed_root != execution_environment.root {
            if best_result_so_far.as_ref().map_or(false, |result| {
                result.py_typed_info.is_some() && !result.is_partly_resolved
            }) {
                return best_result_so_far;
            }
        }
    }

    if allow_pyi && !module_descriptor.name_parts.is_empty() {
        // Check for a stdlib typeshed file.
        debug!("Looking for typeshed stdlib path: {}", import_name);
        if let Some(mut typeshed_stdilib_import) =
            find_typeshed_path(module_descriptor, true, config, host)
        {
            typeshed_stdilib_import.is_stdlib_typeshed_file = true;
            return Some(typeshed_stdilib_import);
        }

        // Check for a third-party typeshed file.
        debug!("Looking for typeshed third-party path: {}", import_name);
        if let Some(mut typeshed_third_party_import) =
            find_typeshed_path(module_descriptor, false, config, host)
        {
            typeshed_third_party_import.is_third_party_typeshed_file = true;

            best_result_so_far = Some(pick_best_import(
                best_result_so_far,
                typeshed_third_party_import,
                module_descriptor,
            ));
        }
    }

    // We weren't able to find an exact match, so return the best
    // partial match.
    best_result_so_far
}

/// Determines whether a namespace package resolves all of the symbols
/// requested in the module descriptor. Namespace packages have no "__init__.py"
/// file, so the only way that symbols can be resolved is if submodules
/// are present. If specific symbols were requested, make sure they
/// are all satisfied by submodules (as listed in the implicit imports).
fn is_namespace_package_resolved(
    module_descriptor: &ImportModuleDescriptor,
    implicit_imports: &BTreeMap<String, ImplicitImport>,
) -> bool {
    if !module_descriptor.imported_symbols.is_empty() {
        // Pyright uses `!Array.from(moduleDescriptor.importedSymbols.keys()).some((symbol) => implicitImports.has(symbol))`.
        // But that only checks if any of the symbols are in the implicit imports?
        for symbol in &module_descriptor.imported_symbols {
            if !implicit_imports.contains_key(symbol) {
                return false;
            }
        }
    } else if implicit_imports.is_empty() {
        return false;
    }
    true
}

/// Finds the `typeshed` path for the given module descriptor.
///
/// Supports both standard library and third-party `typeshed` lookups.
fn find_typeshed_path<Host: host::Host>(
    module_descriptor: &ImportModuleDescriptor,
    is_std_lib: bool,
    config: &Config,
    host: &Host,
) -> Option<ImportResult> {
    if is_std_lib {
        debug!("Looking for typeshed `stdlib` path");
    } else {
        debug!("Looking for typeshed `stubs` path");
    }

    let mut typeshed_paths = vec![];

    if is_std_lib {
        if let Some(path) = search::stdlib_typeshed_path(config, host) {
            typeshed_paths.push(path);
        }
    } else {
        if let Some(paths) =
            search::third_party_typeshed_package_paths(module_descriptor, config, host)
        {
            typeshed_paths.extend(paths);
        }
    }

    for typeshed_path in typeshed_paths {
        if typeshed_path.is_dir() {
            let mut import_info = resolve_absolute_import(
                &typeshed_path,
                module_descriptor,
                false,
                false,
                false,
                true,
                false,
            );
            if import_info.is_import_found {
                import_info.import_type = if is_std_lib {
                    ImportType::BuiltIn
                } else {
                    ImportType::ThirdParty
                };
                return Some(import_info);
            }
        }
    }

    debug!("Typeshed path not found");
    None
}

/// Given a current "best" import and a newly discovered result, returns the
/// preferred result.
fn pick_best_import(
    best_import_so_far: Option<ImportResult>,
    new_import: ImportResult,
    module_descriptor: &ImportModuleDescriptor,
) -> ImportResult {
    let Some(best_import_so_far) = best_import_so_far else {
        return new_import;
    };

    if new_import.is_import_found {
        // Prefer traditional over namespace packages.
        let so_far_index = best_import_so_far
            .resolved_paths
            .iter()
            .position(|path| !path.as_os_str().is_empty());
        let new_index = new_import
            .resolved_paths
            .iter()
            .position(|path| !path.as_os_str().is_empty());
        if so_far_index != new_index {
            match (so_far_index, new_index) {
                (None, Some(_)) => return new_import,
                (Some(_), None) => return best_import_so_far,
                (Some(so_far_index), Some(new_index)) => {
                    return if so_far_index < new_index {
                        best_import_so_far
                    } else {
                        new_import
                    }
                }
                _ => {}
            }
        }

        // Prefer "found" over "not found".
        if !best_import_so_far.is_import_found {
            return new_import;
        }

        // If both results are namespace imports, prefer the result that resolves all
        // imported symbols.
        if best_import_so_far.is_namespace_package && new_import.is_namespace_package {
            if !module_descriptor.imported_symbols.is_empty() {
                if !is_namespace_package_resolved(
                    module_descriptor,
                    &best_import_so_far.implicit_imports,
                ) {
                    if is_namespace_package_resolved(
                        module_descriptor,
                        &new_import.implicit_imports,
                    ) {
                        return new_import;
                    }

                    // Prefer the namespace package that has an `__init__.py[i]` file present in the
                    // final directory over one that does not.
                    if best_import_so_far.is_init_file_present && !new_import.is_init_file_present {
                        return best_import_so_far;
                    }
                    if !best_import_so_far.is_init_file_present && new_import.is_init_file_present {
                        return new_import;
                    }
                }
            }
        }

        // Prefer "py.typed" over "non-py.typed".
        if best_import_so_far.py_typed_info.is_some() && new_import.py_typed_info.is_none() {
            return best_import_so_far;
        }
        if best_import_so_far.py_typed_info.is_none() && best_import_so_far.py_typed_info.is_some()
        {
            return new_import;
        }

        // Prefer stub files (`.pyi`) over non-stub files (`.py`).
        if best_import_so_far.is_stub_file && !new_import.is_stub_file {
            return best_import_so_far;
        }
        if !best_import_so_far.is_stub_file && new_import.is_stub_file {
            return new_import;
        }

        // If we're still tied, prefer a shorter resolution path.
        if best_import_so_far.resolved_paths.len() > new_import.resolved_paths.len() {
            return new_import;
        }
    } else if new_import.is_partly_resolved {
        let so_far_index = best_import_so_far
            .resolved_paths
            .iter()
            .position(|path| !path.as_os_str().is_empty());
        let new_index = new_import
            .resolved_paths
            .iter()
            .position(|path| !path.as_os_str().is_empty());
        if so_far_index != new_index {
            match (so_far_index, new_index) {
                (None, Some(_)) => return new_import,
                (Some(_), None) => return best_import_so_far,
                (Some(so_far_index), Some(new_index)) => {
                    return if so_far_index < new_index {
                        best_import_so_far
                    } else {
                        new_import
                    }
                }
                _ => {}
            }
        }
    }

    best_import_so_far
}

/// Resolve a relative import.
fn resolve_relative_import(
    source_file: &Path,
    module_descriptor: &ImportModuleDescriptor,
) -> Option<ImportResult> {
    // Determine which search path this file is part of.
    let mut directory = source_file;
    for _ in 0..module_descriptor.leading_dots {
        directory = directory.parent()?;
    }

    // Now try to match the module parts from the current directory location.
    let mut abs_import = resolve_absolute_import(
        directory,
        module_descriptor,
        false,
        true,
        false,
        true,
        false,
    );

    if abs_import.is_stub_file {
        // If we found a stub for a relative import, only search
        // the same folder for the real module. Otherwise, it will
        // error out on runtime.
        abs_import.non_stub_import_result = Some(Box::new(resolve_absolute_import(
            directory,
            module_descriptor,
            false,
            true,
            false,
            false,
            false,
        )));
    }

    Some(abs_import)
}

/// Resolve an absolute or relative import.
fn resolve_import_strict<Host: host::Host>(
    source_file: &Path,
    execution_environment: &ExecutionEnvironment,
    module_descriptor: &ImportModuleDescriptor,
    config: &Config,
    host: &Host,
) -> ImportResult {
    let import_name = module_descriptor.name();

    if module_descriptor.leading_dots > 0 {
        debug!("Resolving relative import for: {import_name}");

        let relative_import = resolve_relative_import(source_file, module_descriptor);

        if let Some(mut relative_import) = relative_import {
            relative_import.is_relative = true;
            return relative_import;
        }
    } else {
        debug!("Resolving best absolute import for: {import_name}");

        let best_import = resolve_best_absolute_import(
            execution_environment,
            module_descriptor,
            true,
            config,
            host,
        );

        if let Some(mut best_import) = best_import {
            if best_import.is_stub_file {
                debug!("Resolving best non-stub absolute import for: {import_name}");

                best_import.non_stub_import_result = Some(Box::new(
                    resolve_best_absolute_import(
                        execution_environment,
                        module_descriptor,
                        false,
                        config,
                        host,
                    )
                    .unwrap_or_else(ImportResult::not_found),
                ));
            }
            return best_import;
        }
    }

    ImportResult::not_found()
}

/// Resolves an import, given the current file and the import descriptor.
///
/// The algorithm is as follows:
///
/// 1. If the import is relative, convert it to an absolute import.
/// 2. Find the "best" match for the import, allowing stub files. Search local imports, any
///    configured search paths, the Python path, the typeshed path, etc.
/// 3. If a stub file was found, find the "best" match for the import, disallowing stub files.
/// 4. If the import wasn't resolved, try to resolve it in the parent directory, then the parent's
///    parent, and so on, until the import root is reached.
fn resolve_import<Host: host::Host>(
    source_file: &Path,
    execution_environment: &ExecutionEnvironment,
    module_descriptor: &ImportModuleDescriptor,
    config: &Config,
    host: &Host,
) -> ImportResult {
    let import_result = resolve_import_strict(
        source_file,
        execution_environment,
        module_descriptor,
        config,
        host,
    );
    if import_result.is_import_found || module_descriptor.leading_dots > 0 {
        return import_result;
    }

    // If we weren't able to resolve an absolute import, try resolving it in the
    // importing file's directory, then the parent directory, and so on, until the
    // import root is reached.
    let root = execution_environment.root.as_path();
    if source_file.starts_with(root) {
        let mut current = source_file;
        while let Some(parent) = current.parent() {
            if parent == root {
                break;
            }

            debug!("Resolving absolute import in parent: {}", parent.display());

            let mut result = resolve_absolute_import(
                parent,
                module_descriptor,
                false,
                false,
                false,
                true,
                false,
            );

            if result.is_import_found {
                if let Some(implicit_imports) = implicit_imports::filter(
                    &result.implicit_imports,
                    &module_descriptor.imported_symbols,
                ) {
                    result.implicit_imports = implicit_imports;
                }
                return result;
            }

            current = parent;
        }
    }

    ImportResult::not_found()
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use std::fs::{create_dir_all, File};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};

    use log::debug;
    use tempfile::TempDir;

    use crate::config::Config;
    use crate::execution_environment::ExecutionEnvironment;
    use crate::host;
    use crate::import_result::{ImportResult, ImportType};
    use crate::module_descriptor::ImportModuleDescriptor;
    use crate::python_platform::PythonPlatform;
    use crate::python_version::PythonVersion;
    use crate::resolver::resolve_import;

    /// Create a file at the given path with the given content.
    fn create(path: PathBuf, content: &str) -> io::Result<PathBuf> {
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        let mut f = File::create(&path)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;

        Ok(path)
    }

    /// Create an empty file at the given path.
    fn empty(path: PathBuf) -> io::Result<PathBuf> {
        create(path, "")
    }

    /// Create a partial `py.typed` file at the given path.
    fn partial(path: PathBuf) -> io::Result<PathBuf> {
        create(path, "partial\n")
    }

    /// Create a `py.typed` file at the given path.
    fn typed(path: PathBuf) -> io::Result<PathBuf> {
        create(path, "# typed")
    }

    #[derive(Debug, Default)]
    struct ResolverOptions {
        extra_paths: Vec<PathBuf>,
        library: Option<PathBuf>,
        stub_path: Option<PathBuf>,
        typeshed_path: Option<PathBuf>,
        venv_path: Option<PathBuf>,
        venv: Option<PathBuf>,
    }

    fn resolve_options(
        source_file: impl AsRef<Path>,
        name: &str,
        root: impl Into<PathBuf>,
        options: ResolverOptions,
    ) -> ImportResult {
        let ResolverOptions {
            extra_paths,
            library,
            stub_path,
            typeshed_path,
            venv_path,
            venv,
        } = options;

        let execution_environment = ExecutionEnvironment {
            root: root.into(),
            python_version: PythonVersion::Py37,
            python_platform: PythonPlatform::Darwin,
            extra_paths,
        };

        let module_descriptor = ImportModuleDescriptor {
            leading_dots: name.chars().take_while(|c| *c == '.').count(),
            name_parts: name
                .chars()
                .skip_while(|c| *c == '.')
                .collect::<String>()
                .split('.')
                .map(std::string::ToString::to_string)
                .collect(),
            imported_symbols: Vec::new(),
        };

        let config = Config {
            typeshed_path,
            stub_path,
            venv_path,
            venv,
        };

        let host = host::StaticHost::new(if let Some(library) = library {
            vec![library]
        } else {
            Vec::new()
        });

        resolve_import(
            source_file.as_ref(),
            &execution_environment,
            &module_descriptor,
            &config,
            &host,
        )
    }

    fn setup() {
        env_logger::builder().is_test(true).try_init().ok();
    }

    #[test]
    fn partial_stub_file_exists() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(library.join("myLib-stubs/py.typed"))?;
        let partial_stub_pyi = empty(library.join("myLib-stubs").join("partialStub.pyi"))?;
        let partial_stub_py = empty(library.join("myLib/partialStub.py"))?;

        let result = resolve_options(
            partial_stub_py,
            "myLib.partialStub",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.import_type, ImportType::ThirdParty);
        assert_eq!(
            result.resolved_paths,
            // TODO(charlie): Pyright matches on `libraryRoot, 'myLib', 'partialStub.pyi'` here.
            // But that file doesn't exist. There's some kind of transform.
            vec![PathBuf::new(), partial_stub_pyi]
        );

        Ok(())
    }

    #[test]
    fn partial_stub_init_exists() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(library.join("myLib-stubs/py.typed"))?;
        let partial_stub_init_pyi = empty(library.join("myLib-stubs/__init__.pyi"))?;
        let partial_stub_init_py = empty(library.join("myLib/__init__.py"))?;

        let result = resolve_options(
            partial_stub_init_py,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.import_type, ImportType::ThirdParty);
        assert_eq!(
            result.resolved_paths,
            // TODO(charlie): Pyright matches on `libraryRoot, 'myLib', '__init__.pyi'` here.
            // But that file doesn't exist. There's some kind of transform.
            vec![partial_stub_init_pyi]
        );

        Ok(())
    }

    #[test]
    fn side_by_side_files() -> io::Result<()> {
        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(library.join("myLib-stubs/py.typed"))?;
        empty(library.join("myLib/partialStub.pyi"))?;
        empty(library.join("myLib/partialStub.py"))?;
        empty(library.join("myLib/partialStub2.py"))?;
        let my_file = empty(root.join("myFile.py"))?;
        let side_by_side_stub_file = empty(library.join("myLib-stubs/partialStub.pyi"))?;
        let partial_stub_file = empty(library.join("myLib-stubs/partialStub2.pyi"))?;

        // Stub package wins over original package (per PEP 561 rules).
        let side_by_side_result = resolve_options(
            &my_file,
            "myLib.partialStub",
            root,
            ResolverOptions {
                library: Some(library.clone()),
                ..Default::default()
            },
        );
        assert!(side_by_side_result.is_import_found);
        assert!(side_by_side_result.is_stub_file);
        assert_eq!(
            side_by_side_result.resolved_paths,
            vec![PathBuf::new(), side_by_side_stub_file]
        );

        // Side by side stub doesn't completely disable partial stub.
        let partial_stub_result = resolve_options(
            &my_file,
            "myLib.partialStub2",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );
        assert!(partial_stub_result.is_import_found);
        assert!(partial_stub_result.is_stub_file);
        assert_eq!(
            partial_stub_result.resolved_paths,
            vec![PathBuf::new(), partial_stub_file]
        );

        Ok(())
    }

    #[test]
    fn stub_package() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("myLib-stubs/stub.pyi"))?;
        empty(library.join("myLib-stubs/__init__.pyi"))?;
        let partial_stub_py = empty(library.join("myLib/partialStub.py"))?;

        let result = resolve_options(
            partial_stub_py,
            "myLib.partialStub",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        // If fully typed stub package exists, that wins over the real package.
        assert!(!result.is_import_found);

        Ok(())
    }

    #[test]
    fn stub_namespace_package() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("myLib-stubs/stub.pyi"))?;
        let partial_stub_py = empty(library.join("myLib/partialStub.py"))?;

        let result = resolve_options(
            partial_stub_py.clone(),
            "myLib.partialStub",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        // If fully typed stub package exists, that wins over the real package.
        assert!(result.is_import_found);
        assert!(!result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![PathBuf::new(), partial_stub_py]);

        Ok(())
    }

    #[test]
    fn stub_in_typing_folder_over_partial_stub_package() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typing_folder = root.join("typing");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(library.join("myLib-stubs/py.typed"))?;
        empty(library.join("myLib-stubs/__init__.pyi"))?;
        let my_lib_pyi = empty(typing_folder.join("myLib.pyi"))?;
        let my_lib_init_py = empty(library.join("myLib/__init__.py"))?;

        let result = resolve_options(
            my_lib_init_py,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                stub_path: Some(typing_folder),
                ..Default::default()
            },
        );

        // If the package exists in typing folder, that gets picked up first (so we resolve to
        // `myLib.pyi`).
        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![my_lib_pyi]);

        Ok(())
    }

    #[test]
    fn partial_stub_package_in_typing_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typing_folder = root.join("typing");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        partial(typing_folder.join("myLib-stubs/py.typed"))?;
        let my_lib_stubs_init_pyi = empty(typing_folder.join("myLib-stubs/__init__.pyi"))?;
        let my_lib_init_py = empty(library.join("myLib/__init__.py"))?;

        let result = resolve_options(
            my_lib_init_py,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                stub_path: Some(typing_folder),
                ..Default::default()
            },
        );

        // If the package exists in typing folder, that gets picked up first (so we resolve to
        // `myLib.pyi`).
        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![my_lib_stubs_init_pyi]);

        Ok(())
    }

    #[test]
    fn typeshed_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typeshed_folder = root.join("ts");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(typeshed_folder.join("stubs/myLibPackage/myLib.pyi"))?;
        partial(library.join("myLib-stubs/py.typed"))?;
        let my_lib_stubs_init_pyi = empty(library.join("myLib-stubs/__init__.pyi"))?;
        let my_lib_init_py = empty(library.join("myLib/__init__.py"))?;

        let result = resolve_options(
            my_lib_init_py,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                typeshed_path: Some(typeshed_folder),
                ..Default::default()
            },
        );

        // Stub packages win over typeshed.
        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![my_lib_stubs_init_pyi]);

        Ok(())
    }

    #[test]
    fn py_typed_file() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("myLib/__init__.py"))?;
        partial(library.join("myLib-stubs/py.typed"))?;
        let partial_stub_init_pyi = empty(library.join("myLib-stubs/__init__.pyi"))?;
        let package_py_typed = typed(library.join("myLib/py.typed"))?;

        let result = resolve_options(
            package_py_typed,
            "myLib",
            root,
            ResolverOptions {
                library: Some(library),
                ..Default::default()
            },
        );

        // Partial stub package always overrides original package.
        assert!(result.is_import_found);
        assert!(result.is_stub_file);
        assert_eq!(result.resolved_paths, vec![partial_stub_init_pyi]);

        Ok(())
    }

    #[test]
    fn py_typed_library() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typeshed_folder = root.join("ts");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        typed(library.join("os/py.typed"))?;
        let init_py = empty(library.join("os/__init__.py"))?;
        let typeshed_init_pyi = empty(typeshed_folder.join("stubs/os/os/__init__.pyi"))?;

        let result = resolve_options(
            typeshed_init_pyi,
            "os",
            root,
            ResolverOptions {
                library: Some(library),
                typeshed_path: Some(typeshed_folder),
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert_eq!(result.resolved_paths, vec![init_py]);

        Ok(())
    }

    #[test]
    fn non_py_typed_library() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();
        let typeshed_folder = root.join("ts");

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("os/__init__.py"))?;
        let typeshed_init_pyi = empty(typeshed_folder.join("stubs/os/os/__init__.pyi"))?;

        let result = resolve_options(
            typeshed_init_pyi.clone(),
            "os",
            root,
            ResolverOptions {
                library: Some(library),
                typeshed_path: Some(typeshed_folder),
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::ThirdParty);
        assert_eq!(result.resolved_paths, vec![typeshed_init_pyi]);

        Ok(())
    }

    #[test]
    fn import_side_by_side_file_root() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let file1 = empty(root.join("file1.py"))?;
        let file2 = empty(root.join("file2.py"))?;

        let result = resolve_options(file2, "file1", root, ResolverOptions::default());

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![file1]);

        Ok(())
    }

    #[test]
    fn import_side_by_side_file_sub_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let test_init = empty(root.join("test/__init__.py"))?;
        let test_file1 = empty(root.join("test/file1.py"))?;
        let test_file2 = empty(root.join("test/file2.py"))?;

        let result = resolve_options(test_file2, "test.file1", root, ResolverOptions::default());

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![test_init, test_file1]);

        Ok(())
    }

    #[test]
    fn import_side_by_side_file_sub_under_src_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let nested_init = empty(root.join("src/nested/__init__.py"))?;
        let nested_file1 = empty(root.join("src/nested/file1.py"))?;
        let nested_file2 = empty(root.join("src/nested/file2.py"))?;

        let result = resolve_options(
            nested_file2,
            "nested.file1",
            root,
            ResolverOptions::default(),
        );

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![nested_init, nested_file1]);

        Ok(())
    }

    #[test]
    fn import_file_sub_under_containing_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let nested_file1 = empty(root.join("src/nested/file1.py"))?;
        let nested_file2 = empty(root.join("src/nested/nested2/file2.py"))?;

        let result = resolve_options(nested_file2, "file1", root, ResolverOptions::default());

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![nested_file1]);

        Ok(())
    }

    #[test]
    fn import_side_by_side_file_sub_under_lib_folder() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let temp_dir = TempDir::new()?;
        let library = temp_dir.path().join("lib").join("site-packages");

        empty(library.join("myLib/file1.py"))?;
        let file2 = empty(library.join("myLib/file2.py"))?;

        let result = resolve_options(file2, "file1", root, ResolverOptions::default());

        debug!("result: {:?}", result);

        assert!(!result.is_import_found);

        Ok(())
    }

    #[test]
    fn nested_namespace_package_1() -> io::Result<()> {
        // See: https://github.com/microsoft/pyright/issues/5089.
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let file = empty(root.join("package1/a/b/c/d.py"))?;
        let package1_init = empty(root.join("package1/a/__init__.py"))?;
        let package2_init = empty(root.join("package2/a/__init__.py"))?;

        let package1 = root.join("package1");
        let package2 = root.join("package2");

        let result = resolve_options(
            package2_init,
            "a.b.c.d",
            root,
            ResolverOptions {
                extra_paths: vec![package1, package2],
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(
            result.resolved_paths,
            vec![package1_init, PathBuf::new(), PathBuf::new(), file]
        );

        Ok(())
    }

    #[test]
    fn nested_namespace_package_2() -> io::Result<()> {
        // See: https://github.com/microsoft/pyright/issues/5089.
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let file = empty(root.join("package1/a/b/c/d.py"))?;
        let package1_init = empty(root.join("package1/a/b/c/__init__.py"))?;
        let package2_init = empty(root.join("package2/a/b/c/__init__.py"))?;

        let package1 = root.join("package1");
        let package2 = root.join("package2");

        let result = resolve_options(
            package2_init,
            "a.b.c.d",
            root,
            ResolverOptions {
                extra_paths: vec![package1, package2],
                ..Default::default()
            },
        );

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(
            result.resolved_paths,
            vec![PathBuf::new(), PathBuf::new(), package1_init, file]
        );

        Ok(())
    }

    #[test]
    fn nested_namespace_package_3() -> io::Result<()> {
        // See: https://github.com/microsoft/pyright/issues/5089.
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        empty(root.join("package1/a/b/c/d.py"))?;
        let package2_init = empty(root.join("package2/a/__init__.py"))?;

        let package1 = root.join("package1");
        let package2 = root.join("package2");

        let result = resolve_options(
            package2_init,
            "a.b.c.d",
            root,
            ResolverOptions {
                extra_paths: vec![package1, package2],
                ..Default::default()
            },
        );

        assert!(!result.is_import_found);

        Ok(())
    }

    #[test]
    fn nested_namespace_package_4() -> io::Result<()> {
        // See: https://github.com/microsoft/pyright/issues/5089.
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        empty(root.join("package1/a/b/__init__.py"))?;
        empty(root.join("package1/a/b/c.py"))?;
        empty(root.join("package2/a/__init__.py"))?;
        let package2_a_b_init = empty(root.join("package2/a/b/__init__.py"))?;

        let package1 = root.join("package1");
        let package2 = root.join("package2");

        let result = resolve_options(
            package2_a_b_init,
            "a.b.c",
            root,
            ResolverOptions {
                extra_paths: vec![package1, package2],
                ..Default::default()
            },
        );

        assert!(!result.is_import_found);

        Ok(())
    }

    // New tests, don't exist upstream.
    #[test]
    fn relative_import_side_by_side_file_root() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        let file1 = empty(root.join("file1.py"))?;
        let file2 = empty(root.join("file2.py"))?;

        let result = resolve_options(file2, ".file1", root, ResolverOptions::default());

        assert!(result.is_import_found);
        assert_eq!(result.import_type, ImportType::Local);
        assert_eq!(result.resolved_paths, vec![file1]);

        Ok(())
    }

    #[test]
    fn invalid_relative_import_side_by_side_file_root() -> io::Result<()> {
        setup();

        let temp_dir = TempDir::new()?;
        let root = temp_dir.path();

        empty(root.join("file1.py"))?;
        let file2 = empty(root.join("file2.py"))?;

        let result = resolve_options(file2, "..file1", root, ResolverOptions::default());

        assert!(!result.is_import_found);

        Ok(())
    }

    #[test]
    fn airflow_standard_library() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "os",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot!(result);
    }

    #[test]
    fn airflow_first_party() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "airflow.jobs.scheduler_job_runner",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot!(result);
    }

    #[test]
    fn airflow_stub_file() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "airflow.compat.functools",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot!(result);
    }

    #[test]
    fn airflow_namespace_package() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "airflow.providers.google.cloud.hooks.gcs",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot!(result);
    }

    #[test]
    fn airflow_third_party() {
        setup();

        let root = PathBuf::from("./resources/test/airflow");
        let source_file = root.join("airflow/api/common/mark_tasks.py");

        let result = resolve_options(
            source_file,
            "sqlalchemy.orm",
            root.clone(),
            ResolverOptions {
                venv_path: Some(root),
                venv: Some(PathBuf::from("venv")),
                ..Default::default()
            },
        );

        assert_debug_snapshot!(result);
    }
}
