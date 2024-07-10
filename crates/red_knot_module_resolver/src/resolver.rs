use std::ops::Deref;
use std::sync::Arc;

use ruff_db::files::{File, FilePath};
use ruff_db::system::SystemPathBuf;

use crate::db::Db;
use crate::module::{Module, ModuleKind};
use crate::module_name::ModuleName;
use crate::path::ModuleResolutionPathBuf;
use crate::resolver::internal::ModuleResolverSettings;
use crate::state::ResolverState;
use crate::supported_py_version::TargetVersion;

/// Configures the module resolver settings.
///
/// Must be called before calling any other module resolution functions.
pub fn set_module_resolution_settings(db: &mut dyn Db, config: RawModuleResolutionSettings) {
    // There's no concurrency issue here because we hold a `&mut dyn Db` reference. No other
    // thread can mutate the `Db` while we're in this call, so using `try_get` to test if
    // the settings have already been set is safe.
    let resolved_settings = config.into_configuration_settings();
    if let Some(existing) = ModuleResolverSettings::try_get(db) {
        existing.set_settings(db).to(resolved_settings);
    } else {
        ModuleResolverSettings::new(db, resolved_settings);
    }
}

/// Resolves a module name to a module.
pub fn resolve_module(db: &dyn Db, module_name: ModuleName) -> Option<Module> {
    let interned_name = internal::ModuleNameIngredient::new(db, module_name);

    resolve_module_query(db, interned_name)
}

/// Salsa query that resolves an interned [`ModuleNameIngredient`] to a module.
///
/// This query should not be called directly. Instead, use [`resolve_module`]. It only exists
/// because Salsa requires the module name to be an ingredient.
#[salsa::tracked]
pub(crate) fn resolve_module_query<'db>(
    db: &'db dyn Db,
    module_name: internal::ModuleNameIngredient<'db>,
) -> Option<Module> {
    let _span = tracing::trace_span!("resolve_module", ?module_name).entered();

    let name = module_name.name(db);

    let (search_path, module_file, kind) = resolve_name(db, name)?;

    let module = Module::new(name.clone(), kind, search_path, module_file);

    Some(module)
}

/// Resolves the module for the given path.
///
/// Returns `None` if the path is not a module locatable via any of the known search paths.
#[allow(unused)]
pub(crate) fn path_to_module(db: &dyn Db, path: &FilePath) -> Option<Module> {
    // It's not entirely clear on first sight why this method calls `file_to_module` instead of
    // it being the other way round, considering that the first thing that `file_to_module` does
    // is to retrieve the file's path.
    //
    // The reason is that `file_to_module` is a tracked Salsa query and salsa queries require that
    // all arguments are Salsa ingredients (something stored in Salsa). `Path`s aren't salsa ingredients but
    // `VfsFile` is. So what we do here is to retrieve the `path`'s `VfsFile` so that we can make
    // use of Salsa's caching and invalidation.
    let file = path.to_file(db.upcast())?;
    file_to_module(db, file)
}

/// Resolves the module for the file with the given id.
///
/// Returns `None` if the file is not a module locatable via any of the known search paths.
#[salsa::tracked]
pub(crate) fn file_to_module(db: &dyn Db, file: File) -> Option<Module> {
    let _span = tracing::trace_span!("file_to_module", ?file).entered();

    let path = file.path(db.upcast());

    let resolver_settings = module_resolver_settings(db);

    let relative_path = resolver_settings
        .search_paths()
        .iter()
        .find_map(|root| root.relativize_path(path))?;

    let module_name = relative_path.to_module_name()?;

    // Resolve the module name to see if Python would resolve the name to the same path.
    // If it doesn't, then that means that multiple modules have the same name in different
    // root paths, but that the module corresponding to `path` is in a lower priority search path,
    // in which case we ignore it.
    let module = resolve_module(db, module_name)?;

    if file == module.file() {
        Some(module)
    } else {
        // This path is for a module with the same name but with a different precedence. For example:
        // ```
        // src/foo.py
        // src/foo/__init__.py
        // ```
        // The module name of `src/foo.py` is `foo`, but the module loaded by Python is `src/foo/__init__.py`.
        // That means we need to ignore `src/foo.py` even though it resolves to the same module name.
        None
    }
}

/// "Raw" configuration settings for module resolution: unvalidated, unnormalized
#[derive(Eq, PartialEq, Debug)]
pub struct RawModuleResolutionSettings {
    /// The target Python version the user has specified
    pub target_version: TargetVersion,

    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Vec<SystemPathBuf>,

    /// The root of the workspace, used for finding first-party modules.
    pub workspace_root: SystemPathBuf,

    /// Optional (already validated) path to standard-library typeshed stubs.
    /// If this is not provided, we will fallback to our vendored typeshed stubs
    /// bundled as a zip file in the binary
    pub custom_typeshed: Option<SystemPathBuf>,

    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    pub site_packages: Option<SystemPathBuf>,
}

impl RawModuleResolutionSettings {
    /// Implementation of the typing spec's [module resolution order]
    ///
    /// TODO(Alex): this method does multiple `.unwrap()` calls when it should really return an error.
    /// Each `.unwrap()` call is a point where we're validating a setting that the user would pass
    /// and transforming it into an internal representation for a validated path.
    /// Rather than panicking if a path fails to validate, we should display an error message to the user
    /// and exit the process with a nonzero exit code.
    /// This validation should probably be done outside of Salsa?
    ///
    /// [module resolution order]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
    fn into_configuration_settings(self) -> ModuleResolutionSettings {
        let RawModuleResolutionSettings {
            target_version,
            extra_paths,
            workspace_root,
            site_packages,
            custom_typeshed,
        } = self;

        let mut paths: Vec<ModuleResolutionPathBuf> = extra_paths
            .into_iter()
            .map(|fs_path| ModuleResolutionPathBuf::extra(fs_path).unwrap())
            .collect();

        paths.push(ModuleResolutionPathBuf::first_party(workspace_root).unwrap());

        paths.push(
            custom_typeshed.map_or_else(ModuleResolutionPathBuf::vendored_stdlib, |custom| {
                ModuleResolutionPathBuf::stdlib_from_custom_typeshed_root(&custom).unwrap()
            }),
        );

        // TODO vendor typeshed's third-party stubs as well as the stdlib and fallback to them as a final step
        if let Some(site_packages) = site_packages {
            paths.push(ModuleResolutionPathBuf::site_packages(site_packages).unwrap());
        }

        ModuleResolutionSettings {
            target_version,
            search_paths: OrderedSearchPaths(paths.into_iter().map(Arc::new).collect()),
        }
    }
}

/// A resolved module resolution order as per the [typing spec]
///
/// [typing spec]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct OrderedSearchPaths(Vec<Arc<ModuleResolutionPathBuf>>);

impl Deref for OrderedSearchPaths {
    type Target = [Arc<ModuleResolutionPathBuf>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModuleResolutionSettings {
    search_paths: OrderedSearchPaths,
    target_version: TargetVersion,
}

impl ModuleResolutionSettings {
    pub(crate) fn search_paths(&self) -> &[Arc<ModuleResolutionPathBuf>] {
        &self.search_paths
    }

    pub(crate) fn target_version(&self) -> TargetVersion {
        self.target_version
    }
}

// The singleton methods generated by salsa are all `pub` instead of `pub(crate)` which triggers
// `unreachable_pub`. Work around this by creating a module and allow `unreachable_pub` for it.
// Salsa also generates uses to `_db` variables for `interned` which triggers `clippy::used_underscore_binding`. Suppress that too
// TODO(micha): Contribute a fix for this upstream where the singleton methods have the same visibility as the struct.
#[allow(unreachable_pub, clippy::used_underscore_binding)]
pub(crate) mod internal {
    use crate::module_name::ModuleName;
    use crate::resolver::ModuleResolutionSettings;

    #[salsa::input(singleton)]
    pub(crate) struct ModuleResolverSettings {
        #[return_ref]
        pub(super) settings: ModuleResolutionSettings,
    }

    /// A thin wrapper around `ModuleName` to make it a Salsa ingredient.
    ///
    /// This is needed because Salsa requires that all query arguments are salsa ingredients.
    #[salsa::interned]
    pub(crate) struct ModuleNameIngredient<'db> {
        #[return_ref]
        pub(super) name: ModuleName,
    }
}

fn module_resolver_settings(db: &dyn Db) -> &ModuleResolutionSettings {
    ModuleResolverSettings::get(db).settings(db)
}

/// Given a module name and a list of search paths in which to lookup modules,
/// attempt to resolve the module name
fn resolve_name(
    db: &dyn Db,
    name: &ModuleName,
) -> Option<(Arc<ModuleResolutionPathBuf>, File, ModuleKind)> {
    let resolver_settings = module_resolver_settings(db);
    let resolver_state = ResolverState::new(db, resolver_settings.target_version());

    for search_path in resolver_settings.search_paths() {
        let mut components = name.components();
        let module_name = components.next_back()?;

        match resolve_package(search_path, components, &resolver_state) {
            Ok(resolved_package) => {
                let mut package_path = resolved_package.path;

                package_path.push(module_name);

                // Must be a `__init__.pyi` or `__init__.py` or it isn't a package.
                let kind = if package_path.is_directory(search_path, &resolver_state) {
                    package_path.push("__init__");
                    ModuleKind::Package
                } else {
                    ModuleKind::Module
                };

                // TODO Implement full https://peps.python.org/pep-0561/#type-checker-module-resolution-order resolution
                if let Some(stub) = package_path
                    .with_pyi_extension()
                    .to_file(search_path, &resolver_state)
                {
                    return Some((search_path.clone(), stub, kind));
                }

                if let Some(module) = package_path
                    .with_py_extension()
                    .and_then(|path| path.to_file(search_path, &resolver_state))
                {
                    return Some((search_path.clone(), module, kind));
                }

                // For regular packages, don't search the next search path. All files of that
                // package must be in the same location
                if resolved_package.kind.is_regular_package() {
                    return None;
                }
            }
            Err(parent_kind) => {
                if parent_kind.is_regular_package() {
                    // For regular packages, don't search the next search path.
                    return None;
                }
            }
        }
    }

    None
}

fn resolve_package<'a, 'db, I>(
    module_search_path: &ModuleResolutionPathBuf,
    components: I,
    resolver_state: &ResolverState<'db>,
) -> Result<ResolvedPackage, PackageKind>
where
    I: Iterator<Item = &'a str>,
{
    let mut package_path = module_search_path.clone();

    // `true` if inside a folder that is a namespace package (has no `__init__.py`).
    // Namespace packages are special because they can be spread across multiple search paths.
    // https://peps.python.org/pep-0420/
    let mut in_namespace_package = false;

    // `true` if resolving a sub-package. For example, `true` when resolving `bar` of `foo.bar`.
    let mut in_sub_package = false;

    // For `foo.bar.baz`, test that `foo` and `baz` both contain a `__init__.py`.
    for folder in components {
        package_path.push(folder);

        let is_regular_package =
            package_path.is_regular_package(module_search_path, resolver_state);

        if is_regular_package {
            in_namespace_package = false;
        } else if package_path.is_directory(module_search_path, resolver_state) {
            // A directory without an `__init__.py` is a namespace package, continue with the next folder.
            in_namespace_package = true;
        } else if in_namespace_package {
            // Package not found but it is part of a namespace package.
            return Err(PackageKind::Namespace);
        } else if in_sub_package {
            // A regular sub package wasn't found.
            return Err(PackageKind::Regular);
        } else {
            // We couldn't find `foo` for `foo.bar.baz`, search the next search path.
            return Err(PackageKind::Root);
        }

        in_sub_package = true;
    }

    let kind = if in_namespace_package {
        PackageKind::Namespace
    } else if in_sub_package {
        PackageKind::Regular
    } else {
        PackageKind::Root
    };

    Ok(ResolvedPackage {
        kind,
        path: package_path,
    })
}

#[derive(Debug)]
struct ResolvedPackage {
    path: ModuleResolutionPathBuf,
    kind: PackageKind,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum PackageKind {
    /// A root package or module. E.g. `foo` in `foo.bar.baz` or just `foo`.
    Root,

    /// A regular sub-package where the parent contains an `__init__.py`.
    ///
    /// For example, `bar` in `foo.bar` when the `foo` directory contains an `__init__.py`.
    Regular,

    /// A sub-package in a namespace package. A namespace package is a package without an `__init__.py`.
    ///
    /// For example, `bar` in `foo.bar` if the `foo` directory contains no `__init__.py`.
    Namespace,
}

impl PackageKind {
    const fn is_regular_package(self) -> bool {
        matches!(self, PackageKind::Regular)
    }
}

#[cfg(test)]
mod tests {
    use internal::ModuleNameIngredient;
    use ruff_db::files::{system_path_to_file, File, FilePath};
    use ruff_db::system::{DbWithTestSystem, OsSystem, SystemPath};
    use ruff_db::testing::assert_function_query_was_not_run;

    use crate::db::tests::TestDb;
    use crate::module::ModuleKind;
    use crate::module_name::ModuleName;
    use crate::testing::{FileSpec, MockedTypeshed, TestCase, TestCaseBuilder};

    use super::*;

    #[test]
    fn first_party_module() {
        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "print('Hello, world!')")])
            .build();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module(&db, foo_module_name.clone()).unwrap();

        assert_eq!(
            Some(&foo_module),
            resolve_module(&db, foo_module_name.clone()).as_ref()
        );

        assert_eq!("foo", foo_module.name());
        assert_eq!(&src, &foo_module.search_path());
        assert_eq!(ModuleKind::Module, foo_module.kind());

        let expected_foo_path = src.join("foo.py");
        assert_eq!(&expected_foo_path, foo_module.file().path(&db));
        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(expected_foo_path))
        );
    }

    #[test]
    fn stdlib() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
            versions: "functools: 3.8-",
        };

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_custom_typeshed(TYPESHED)
            .with_target_version(TargetVersion::Py38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let functools_module = resolve_module(&db, functools_module_name.clone()).unwrap();

        assert_eq!(
            Some(&functools_module),
            resolve_module(&db, functools_module_name).as_ref()
        );

        assert_eq!(&stdlib, &functools_module.search_path().to_path_buf());
        assert_eq!(ModuleKind::Module, functools_module.kind());

        let expected_functools_path = stdlib.join("functools.pyi");
        assert_eq!(&expected_functools_path, functools_module.file().path(&db));

        assert_eq!(
            Some(functools_module),
            path_to_module(&db, &FilePath::System(expected_functools_path))
        );
    }

    fn create_module_names(raw_names: &[&str]) -> Vec<ModuleName> {
        raw_names
            .iter()
            .map(|raw| ModuleName::new(raw).unwrap())
            .collect()
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py38_existing_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            functools: 3.8-             # Top-level single-file module
            xml: 3.8-3.8                # Namespace package on py38 only
        ";

        const STDLIB: &[FileSpec] = &[
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("functools.pyi", ""),
            ("xml/etree.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_custom_typeshed(TYPESHED)
            .with_target_version(TargetVersion::Py38)
            .build();

        let existing_modules = create_module_names(&["asyncio", "functools", "xml.etree"]);
        for module_name in existing_modules {
            let resolved_module = resolve_module(&db, module_name.clone()).unwrap_or_else(|| {
                panic!("Expected module {module_name} to exist in the mock stdlib")
            });
            let search_path = resolved_module.search_path();
            assert_eq!(
                &stdlib, &search_path,
                "Search path for {module_name} was unexpectedly {search_path:?}"
            );
            assert!(
                search_path.is_stdlib_search_path(),
                "Expected a stdlib search path, but got {search_path:?}"
            );
        }
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py38_nonexisting_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            collections: 3.9-           # 'Regular' package on py39+
            importlib: 3.9-             # Namespace package on py39+
            xml: 3.8-3.8                # Namespace package on 3.8 only
        ";

        const STDLIB: &[FileSpec] = &[
            ("collections/__init__.pyi", ""),
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("importlib/abc.pyi", ""),
            ("xml/etree.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_custom_typeshed(TYPESHED)
            .with_target_version(TargetVersion::Py38)
            .build();

        let nonexisting_modules = create_module_names(&[
            "collections",
            "importlib",
            "importlib.abc",
            "xml",
            "asyncio.tasks",
        ]);

        for module_name in nonexisting_modules {
            assert!(
                resolve_module(&db, module_name.clone()).is_none(),
                "Unexpectedly resolved a module for {module_name}"
            );
        }
    }

    #[test]
    fn stdlib_resolution_respects_versions_file_py39_existing_modules() {
        const VERSIONS: &str = "\
            asyncio: 3.8-               # 'Regular' package on py38+
            asyncio.tasks: 3.9-3.11     # Submodule on py39+ only
            collections: 3.9-           # 'Regular' package on py39+
            functools: 3.8-             # Top-level single-file module
            importlib: 3.9-             # Namespace package on py39+
        ";

        const STDLIB: &[FileSpec] = &[
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("collections/__init__.pyi", ""),
            ("functools.pyi", ""),
            ("importlib/abc.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_custom_typeshed(TYPESHED)
            .with_target_version(TargetVersion::Py39)
            .build();

        let existing_modules = create_module_names(&[
            "asyncio",
            "functools",
            "importlib.abc",
            "collections",
            "asyncio.tasks",
        ]);

        for module_name in existing_modules {
            let resolved_module = resolve_module(&db, module_name.clone()).unwrap_or_else(|| {
                panic!("Expected module {module_name} to exist in the mock stdlib")
            });
            let search_path = resolved_module.search_path();
            assert_eq!(
                &stdlib, &search_path,
                "Search path for {module_name} was unexpectedly {search_path:?}"
            );
            assert!(
                search_path.is_stdlib_search_path(),
                "Expected a stdlib search path, but got {search_path:?}"
            );
        }
    }
    #[test]
    fn stdlib_resolution_respects_versions_file_py39_nonexisting_modules() {
        const VERSIONS: &str = "\
            importlib: 3.9-   # Namespace package on py39+
            xml: 3.8-3.8      # Namespace package on 3.8 only
        ";

        const STDLIB: &[FileSpec] = &[("importlib/abc.pyi", ""), ("xml/etree.pyi", "")];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: STDLIB,
            versions: VERSIONS,
        };

        let TestCase { db, .. } = TestCaseBuilder::new()
            .with_custom_typeshed(TYPESHED)
            .with_target_version(TargetVersion::Py39)
            .build();

        let nonexisting_modules = create_module_names(&["importlib", "xml", "xml.etree"]);
        for module_name in nonexisting_modules {
            assert!(
                resolve_module(&db, module_name.clone()).is_none(),
                "Unexpectedly resolved a module for {module_name}"
            );
        }
    }

    #[test]
    fn first_party_precedence_over_stdlib() {
        const SRC: &[FileSpec] = &[("functools.py", "def update_wrapper(): ...")];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
            versions: "functools: 3.8-",
        };

        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(SRC)
            .with_custom_typeshed(TYPESHED)
            .with_target_version(TargetVersion::Py38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let functools_module = resolve_module(&db, functools_module_name.clone()).unwrap();

        assert_eq!(
            Some(&functools_module),
            resolve_module(&db, functools_module_name).as_ref()
        );
        assert_eq!(&src, &functools_module.search_path());
        assert_eq!(ModuleKind::Module, functools_module.kind());
        assert_eq!(&src.join("functools.py"), functools_module.file().path(&db));

        assert_eq!(
            Some(functools_module),
            path_to_module(&db, &FilePath::System(src.join("functools.py")))
        );
    }

    #[test]
    fn stdlib_uses_vendored_typeshed_when_no_custom_typeshed_supplied() {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_vendored_typeshed()
            .with_target_version(TargetVersion::default())
            .build();

        let pydoc_data_topics_name = ModuleName::new_static("pydoc_data.topics").unwrap();
        let pydoc_data_topics = resolve_module(&db, pydoc_data_topics_name).unwrap();

        assert_eq!("pydoc_data.topics", pydoc_data_topics.name());
        assert_eq!(pydoc_data_topics.search_path(), stdlib);
        assert_eq!(
            pydoc_data_topics.file().path(&db),
            &stdlib.join("pydoc_data/topics.pyi")
        );
    }

    #[test]
    fn resolve_package() {
        let TestCase { src, db, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo/__init__.py", "print('Hello, world!'")])
            .build();

        let foo_path = src.join("foo/__init__.py");
        let foo_module = resolve_module(&db, ModuleName::new_static("foo").unwrap()).unwrap();

        assert_eq!("foo", foo_module.name());
        assert_eq!(&src, &foo_module.search_path());
        assert_eq!(&foo_path, foo_module.file().path(&db));

        assert_eq!(
            Some(&foo_module),
            path_to_module(&db, &FilePath::System(foo_path)).as_ref()
        );

        // Resolving by directory doesn't resolve to the init file.
        assert_eq!(
            None,
            path_to_module(&db, &FilePath::System(src.join("foo")))
        );
    }

    #[test]
    fn package_priority_over_module() {
        const SRC: &[FileSpec] = &[
            ("foo/__init__.py", "print('Hello, world!')"),
            ("foo.py", "print('Hello, world!')"),
        ];

        let TestCase { db, src, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        let foo_module = resolve_module(&db, ModuleName::new_static("foo").unwrap()).unwrap();
        let foo_init_path = src.join("foo/__init__.py");

        assert_eq!(&src, &foo_module.search_path());
        assert_eq!(&foo_init_path, foo_module.file().path(&db));
        assert_eq!(ModuleKind::Package, foo_module.kind());

        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(foo_init_path))
        );
        assert_eq!(
            None,
            path_to_module(&db, &FilePath::System(src.join("foo.py")))
        );
    }

    #[test]
    fn typing_stub_over_module() {
        const SRC: &[FileSpec] = &[("foo.py", "print('Hello, world!')"), ("foo.pyi", "x: int")];

        let TestCase { db, src, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        let foo = resolve_module(&db, ModuleName::new_static("foo").unwrap()).unwrap();
        let foo_stub = src.join("foo.pyi");

        assert_eq!(&src, &foo.search_path());
        assert_eq!(&foo_stub, foo.file().path(&db));

        assert_eq!(Some(foo), path_to_module(&db, &FilePath::System(foo_stub)));
        assert_eq!(
            None,
            path_to_module(&db, &FilePath::System(src.join("foo.py")))
        );
    }

    #[test]
    fn sub_packages() {
        const SRC: &[FileSpec] = &[
            ("foo/__init__.py", ""),
            ("foo/bar/__init__.py", ""),
            ("foo/bar/baz.py", "print('Hello, world!)'"),
        ];

        let TestCase { db, src, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        let baz_module =
            resolve_module(&db, ModuleName::new_static("foo.bar.baz").unwrap()).unwrap();
        let baz_path = src.join("foo/bar/baz.py");

        assert_eq!(&src, &baz_module.search_path());
        assert_eq!(&baz_path, baz_module.file().path(&db));

        assert_eq!(
            Some(baz_module),
            path_to_module(&db, &FilePath::System(baz_path))
        );
    }

    #[test]
    fn namespace_package() {
        // From [PEP420](https://peps.python.org/pep-0420/#nested-namespace-packages).
        // But uses `src` for `project1` and `site-packages` for `project2`.
        // ```
        // src
        //   parent
        //     child
        //       one.py
        // site_packages
        //   parent
        //     child
        //       two.py
        // ```
        let TestCase {
            db,
            src,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_src_files(&[("parent/child/one.py", "print('Hello, world!')")])
            .with_site_packages_files(&[("parent/child/two.py", "print('Hello, world!')")])
            .build();

        let one_module_name = ModuleName::new_static("parent.child.one").unwrap();
        let one_module_path = FilePath::System(src.join("parent/child/one.py"));
        assert_eq!(
            resolve_module(&db, one_module_name),
            path_to_module(&db, &one_module_path)
        );

        let two_module_name = ModuleName::new_static("parent.child.two").unwrap();
        let two_module_path = FilePath::System(site_packages.join("parent/child/two.py"));
        assert_eq!(
            resolve_module(&db, two_module_name),
            path_to_module(&db, &two_module_path)
        );
    }

    #[test]
    fn regular_package_in_namespace_package() {
        // Adopted test case from the [PEP420 examples](https://peps.python.org/pep-0420/#nested-namespace-packages).
        // The `src/parent/child` package is a regular package. Therefore, `site_packages/parent/child/two.py` should not be resolved.
        // ```
        // src
        //   parent
        //     child
        //       one.py
        // site_packages
        //   parent
        //     child
        //       two.py
        // ```
        const SRC: &[FileSpec] = &[
            ("parent/child/__init__.py", "print('Hello, world!')"),
            ("parent/child/one.py", "print('Hello, world!')"),
        ];

        const SITE_PACKAGES: &[FileSpec] = &[("parent/child/two.py", "print('Hello, world!')")];

        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(SRC)
            .with_site_packages_files(SITE_PACKAGES)
            .build();

        let one_module_path = FilePath::System(src.join("parent/child/one.py"));
        let one_module_name =
            resolve_module(&db, ModuleName::new_static("parent.child.one").unwrap());
        assert_eq!(one_module_name, path_to_module(&db, &one_module_path));

        assert_eq!(
            None,
            resolve_module(&db, ModuleName::new_static("parent.child.two").unwrap())
        );
    }

    #[test]
    fn module_search_path_priority() {
        let TestCase {
            db,
            src,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "")])
            .with_site_packages_files(&[("foo.py", "")])
            .build();

        let foo_module = resolve_module(&db, ModuleName::new_static("foo").unwrap()).unwrap();
        let foo_src_path = src.join("foo.py");

        assert_eq!(&src, &foo_module.search_path());
        assert_eq!(&foo_src_path, foo_module.file().path(&db));
        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(foo_src_path))
        );

        assert_eq!(
            None,
            path_to_module(&db, &FilePath::System(site_packages.join("foo.py")))
        );
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn symlink() -> anyhow::Result<()> {
        let mut db = TestDb::new();

        let temp_dir = tempfile::tempdir()?;
        let root = SystemPath::from_std_path(temp_dir.path()).unwrap();
        db.use_os_system(OsSystem::new(root));

        let src = root.join("src");
        let site_packages = root.join("site-packages");
        let custom_typeshed = root.join("typeshed");

        let foo = src.join("foo.py");
        let bar = src.join("bar.py");

        std::fs::create_dir_all(src.as_std_path())?;
        std::fs::create_dir_all(site_packages.as_std_path())?;
        std::fs::create_dir_all(custom_typeshed.as_std_path())?;

        std::fs::write(foo.as_std_path(), "")?;
        std::os::unix::fs::symlink(foo.as_std_path(), bar.as_std_path())?;

        let settings = RawModuleResolutionSettings {
            target_version: TargetVersion::Py38,
            extra_paths: vec![],
            workspace_root: src.clone(),
            site_packages: Some(site_packages.clone()),
            custom_typeshed: Some(custom_typeshed.clone()),
        };

        set_module_resolution_settings(&mut db, settings);

        let foo_module = resolve_module(&db, ModuleName::new_static("foo").unwrap()).unwrap();
        let bar_module = resolve_module(&db, ModuleName::new_static("bar").unwrap()).unwrap();

        assert_ne!(foo_module, bar_module);

        assert_eq!(&src, &foo_module.search_path());
        assert_eq!(&foo, foo_module.file().path(&db));

        // `foo` and `bar` shouldn't resolve to the same file

        assert_eq!(&src, &bar_module.search_path());
        assert_eq!(&bar, bar_module.file().path(&db));
        assert_eq!(&foo, foo_module.file().path(&db));

        assert_ne!(&foo_module, &bar_module);

        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &FilePath::System(foo))
        );
        assert_eq!(
            Some(bar_module),
            path_to_module(&db, &FilePath::System(bar))
        );

        Ok(())
    }

    #[test]
    fn deleting_an_unrelated_file_doesnt_change_module_resolution() {
        let TestCase { mut db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[("foo.py", "x = 1"), ("bar.py", "x = 2")])
            .with_target_version(TargetVersion::Py38)
            .build();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module(&db, foo_module_name.clone()).unwrap();

        let bar_path = src.join("bar.py");
        let bar = system_path_to_file(&db, &bar_path).expect("bar.py to exist");

        db.clear_salsa_events();

        // Delete `bar.py`
        db.memory_file_system().remove_file(&bar_path).unwrap();
        bar.touch(&mut db);

        // Re-query the foo module. The foo module should still be cached because `bar.py` isn't relevant
        // for resolving `foo`.

        let foo_module2 = resolve_module(&db, foo_module_name);

        assert!(!db
            .take_salsa_events()
            .iter()
            .any(|event| { matches!(event.kind, salsa::EventKind::WillExecute { .. }) }));

        assert_eq!(Some(foo_module), foo_module2);
    }

    #[test]
    fn adding_file_on_which_module_resolution_depends_invalidates_previously_failing_query_that_now_succeeds(
    ) -> anyhow::Result<()> {
        let TestCase { mut db, src, .. } = TestCaseBuilder::new().build();
        let foo_path = src.join("foo.py");

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        assert_eq!(resolve_module(&db, foo_module_name.clone()), None);

        // Now write the foo file
        db.write_file(&foo_path, "x = 1")?;

        let foo_file = system_path_to_file(&db, &foo_path).expect("foo.py to exist");

        let foo_module = resolve_module(&db, foo_module_name).expect("Foo module to resolve");
        assert_eq!(foo_file, foo_module.file());

        Ok(())
    }

    #[test]
    fn removing_file_on_which_module_resolution_depends_invalidates_previously_successful_query_that_now_fails(
    ) -> anyhow::Result<()> {
        const SRC: &[FileSpec] = &[("foo.py", "x = 1"), ("foo/__init__.py", "x = 2")];

        let TestCase { mut db, src, .. } = TestCaseBuilder::new().with_src_files(SRC).build();

        let foo_module_name = ModuleName::new_static("foo").unwrap();
        let foo_module = resolve_module(&db, foo_module_name.clone()).expect("foo module to exist");
        let foo_init_path = src.join("foo/__init__.py");

        assert_eq!(&foo_init_path, foo_module.file().path(&db));

        // Delete `foo/__init__.py` and the `foo` folder. `foo` should now resolve to `foo.py`
        db.memory_file_system().remove_file(&foo_init_path)?;
        db.memory_file_system()
            .remove_directory(foo_init_path.parent().unwrap())?;
        File::touch_path(&mut db, &foo_init_path);

        let foo_module = resolve_module(&db, foo_module_name).expect("Foo module to resolve");
        assert_eq!(&src.join("foo.py"), foo_module.file().path(&db));

        Ok(())
    }

    #[test]
    fn adding_file_to_search_path_with_lower_priority_does_not_invalidate_query() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
        };

        let TestCase {
            mut db,
            stdlib,
            site_packages,
            ..
        } = TestCaseBuilder::new()
            .with_custom_typeshed(TYPESHED)
            .with_target_version(TargetVersion::Py38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let stdlib_functools_path = stdlib.join("functools.pyi");

        let functools_module = resolve_module(&db, functools_module_name.clone()).unwrap();
        assert_eq!(functools_module.search_path(), stdlib);
        assert_eq!(
            Some(functools_module.file()),
            system_path_to_file(&db, &stdlib_functools_path)
        );

        // Adding a file to site-packages does not invalidate the query,
        // since site-packages takes lower priority in the module resolution
        db.clear_salsa_events();
        let site_packages_functools_path = site_packages.join("functools.py");
        db.write_file(&site_packages_functools_path, "f: int")
            .unwrap();
        let functools_module = resolve_module(&db, functools_module_name.clone()).unwrap();
        let events = db.take_salsa_events();
        assert_function_query_was_not_run::<resolve_module_query, _, _>(
            &db,
            |res| &res.function,
            &ModuleNameIngredient::new(&db, functools_module_name.clone()),
            &events,
        );
        assert_eq!(functools_module.search_path(), stdlib);
        assert_eq!(
            Some(functools_module.file()),
            system_path_to_file(&db, &stdlib_functools_path)
        );
    }

    #[test]
    fn adding_file_to_search_path_with_higher_priority_invalidates_the_query() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
        };

        let TestCase {
            mut db,
            stdlib,
            src,
            ..
        } = TestCaseBuilder::new()
            .with_custom_typeshed(TYPESHED)
            .with_target_version(TargetVersion::Py38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let functools_module = resolve_module(&db, functools_module_name.clone()).unwrap();
        assert_eq!(functools_module.search_path(), stdlib);
        assert_eq!(
            Some(functools_module.file()),
            system_path_to_file(&db, stdlib.join("functools.pyi"))
        );

        // Adding a first-party file invalidates the query,
        // since first-party files take higher priority in module resolution:
        let src_functools_path = src.join("functools.py");
        db.write_file(&src_functools_path, "FOO: int").unwrap();
        let functools_module = resolve_module(&db, functools_module_name.clone()).unwrap();
        assert_eq!(functools_module.search_path(), src);
        assert_eq!(
            Some(functools_module.file()),
            system_path_to_file(&db, &src_functools_path)
        );
    }

    #[test]
    fn deleting_file_from_higher_priority_search_path_invalidates_the_query() {
        const SRC: &[FileSpec] = &[("functools.py", "FOO: int")];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "def update_wrapper(): ...")],
        };

        let TestCase {
            mut db,
            stdlib,
            src,
            ..
        } = TestCaseBuilder::new()
            .with_src_files(SRC)
            .with_custom_typeshed(TYPESHED)
            .with_target_version(TargetVersion::Py38)
            .build();

        let functools_module_name = ModuleName::new_static("functools").unwrap();
        let src_functools_path = src.join("functools.py");

        let functools_module = resolve_module(&db, functools_module_name.clone()).unwrap();
        assert_eq!(functools_module.search_path(), src);
        assert_eq!(
            Some(functools_module.file()),
            system_path_to_file(&db, &src_functools_path)
        );

        // If we now delete the first-party file,
        // it should resolve to the stdlib:
        db.memory_file_system()
            .remove_file(&src_functools_path)
            .unwrap();
        File::touch_path(&mut db, &src_functools_path);
        let functools_module = resolve_module(&db, functools_module_name.clone()).unwrap();
        assert_eq!(functools_module.search_path(), stdlib);
        assert_eq!(
            Some(functools_module.file()),
            system_path_to_file(&db, stdlib.join("functools.pyi"))
        );
    }
}
