//! Resolves Python imports to their corresponding files on disk.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use log::debug;

use crate::config::Config;
use crate::execution_environment::ExecutionEnvironment;
use crate::implicit_imports::ImplicitImports;
use crate::import_result::{ImportResult, ImportType};
use crate::module_descriptor::ImportModuleDescriptor;
use crate::{host, native_module, py_typed, search};

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
    let mut implicit_imports = None;
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

        implicit_imports = ImplicitImports::find(&dir_path, &[&py_file_path, &pyi_file_path]).ok();
    } else {
        for (i, part) in module_descriptor.name_parts.iter().enumerate() {
            let is_first_part = i == 0;
            let is_last_part = i == module_descriptor.name_parts.len() - 1;

            // Extend the directory path with the next segment.
            let module_dir_path = if use_stub_package && is_first_part {
                is_stub_package = true;
                dir_path.join(format!("{part}-stubs"))
            } else {
                dir_path.join(part)
            };

            let found_directory = module_dir_path.is_dir();
            if found_directory {
                if is_first_part {
                    package_directory = Some(module_dir_path.clone());
                }

                // Look for an `__init__.py[i]` in the directory.
                let py_file_path = module_dir_path.join("__init__.py");
                let pyi_file_path = module_dir_path.join("__init__.pyi");
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
                        py_typed_info.or_else(|| py_typed::get_py_typed_info(&module_dir_path));
                }

                // We haven't reached the end of the import, and we found a matching directory.
                // Proceed to the next segment.
                if !is_last_part {
                    if !is_init_file_present {
                        resolved_paths.push(PathBuf::new());
                        is_namespace_package = true;
                        py_typed_info = None;
                    }

                    dir_path = module_dir_path;
                    continue;
                }

                if is_init_file_present {
                    implicit_imports =
                        ImplicitImports::find(&module_dir_path, &[&py_file_path, &pyi_file_path])
                            .ok();
                    break;
                }
            }

            // We couldn't find a matching directory, or the directory didn't contain an
            // `__init__.py[i]` file. Look for an `.py[i]` file with the same name as the
            // segment, in lieu of a directory.
            let py_file_path = module_dir_path.with_extension("py");
            let pyi_file_path = module_dir_path.with_extension("pyi");

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
                    if let Some(module_name) = module_dir_path.file_name().and_then(OsStr::to_str) {
                        if let Ok(Some(native_lib_path)) =
                            native_module::find_native_module(module_name, &dir_path)
                        {
                            debug!("Resolved import with file: {native_lib_path:?}");
                            is_native_lib = true;
                            resolved_paths.push(native_lib_path);
                        }
                    }
                }

                if !is_native_lib && found_directory {
                    debug!("Partially resolved import with directory: {dir_path:?}");
                    resolved_paths.push(PathBuf::new());
                    if is_last_part {
                        implicit_imports =
                            ImplicitImports::find(&dir_path, &[&py_file_path, &pyi_file_path]).ok();
                        is_namespace_package = true;
                    }
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

    let is_partly_resolved = if resolved_paths.is_empty() {
        false
    } else {
        resolved_paths.len() < module_descriptor.name_parts.len()
    };

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
        implicit_imports: implicit_imports.unwrap_or_default(),
        filtered_implicit_imports: ImplicitImports::default(),
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
                    && typings_import
                        .resolved_paths
                        .last()
                        .is_some_and(|path| path.as_os_str().is_empty())
                {
                    if typings_import
                        .implicit_imports
                        .resolves_namespace_package(&module_descriptor.imported_symbols)
                    {
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
            if best_result_so_far
                .as_ref()
                .is_some_and(|result| result.py_typed_info.is_some() && !result.is_partly_resolved)
            {
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
                if !best_import_so_far
                    .implicit_imports
                    .resolves_namespace_package(&module_descriptor.imported_symbols)
                {
                    if new_import
                        .implicit_imports
                        .resolves_namespace_package(&module_descriptor.imported_symbols)
                    {
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
pub(crate) fn resolve_import<Host: host::Host>(
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
    let mut current = source_file;
    while let Some(parent) = current.parent() {
        if !parent.starts_with(root) {
            break;
        }

        debug!("Resolving absolute import in parent: {}", parent.display());

        let mut result =
            resolve_absolute_import(parent, module_descriptor, false, false, false, true, false);

        if result.is_import_found {
            if let Some(implicit_imports) = result
                .implicit_imports
                .filter(&module_descriptor.imported_symbols)
            {
                result.implicit_imports = implicit_imports;
            }
            return result;
        }

        current = parent;
    }

    ImportResult::not_found()
}
