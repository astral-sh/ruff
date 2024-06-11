use std::ops::Deref;
use std::sync::Arc;

use ruff_db::file_system::{FileSystem, FileSystemPath, FileSystemPathBuf};
use ruff_db::vfs::{file_system_path_to_file, vfs_path_to_file, VfsFile, VfsPath};

use crate::module::resolver::internal::ModuleResolverSearchPaths;
use crate::module::{
    Module, ModuleInner, ModuleKind, ModuleName, ModuleSearchPath, ModuleSearchPathKind,
};
use crate::Db;

const TYPESHED_STDLIB_DIRECTORY: &str = "stdlib";

/// Configures the module search paths for the module resolver.
///
/// Must be called before calling any other module resolution functions.
pub fn set_module_resolution_settings(db: &mut dyn Db, config: ModuleResolutionSettings) {
    // There's no concurrency issue here because we hold a `&mut dyn Db` reference. No other
    // thread can mutate the `Db` while we're in this call, so using `try_get` to test if
    // the settings have already been set is safe.
    if let Some(existing) = ModuleResolverSearchPaths::try_get(db) {
        existing
            .set_search_paths(db)
            .to(config.into_ordered_search_paths());
    } else {
        ModuleResolverSearchPaths::new(db, config.into_ordered_search_paths());
    }
}

/// Resolves a module name to a module.
#[tracing::instrument(level = "debug", skip(db))]
pub fn resolve_module(db: &dyn Db, module_name: ModuleName) -> Option<Module> {
    let interned_name = internal::ModuleNameIngredient::new(db, module_name);

    resolve_module_query(db, interned_name)
}

/// Salsa query that resolves an interned [`ModuleNameIngredient`] to a module.
///
/// This query should not be called directly. Instead, use [`resolve_module`]. It only exists
/// because Salsa requires the module name to be an ingredient.
#[salsa::tracked]
pub(crate) fn resolve_module_query(
    db: &dyn Db,
    module_name: internal::ModuleNameIngredient,
) -> Option<Module> {
    let name = module_name.name(db);

    let (search_path, module_file, kind) = resolve_name(db, name)?;

    let module = Module {
        inner: Arc::new(ModuleInner {
            name: name.clone(),
            kind,
            search_path,
            file: module_file,
        }),
    };

    Some(module)
}

/// Resolves the module for the given path.
///
/// Returns `None` if the path is not a module locatable via `sys.path`.
#[tracing::instrument(level = "debug", skip(db))]
pub fn path_to_module(db: &dyn Db, path: &VfsPath) -> Option<Module> {
    // It's a bit surprising to have `path_to_module` that resolves the file for `path` when the first thing that `file_to_module` does
    // is retrieving the path from `file`. The reason for this "odd" looking API is that
    // `file_to_module` is cached using Salsa and:
    // * Salsa only allows ingredients as query arguments
    // * `path` isn't a salsa ingredient, but `file` is
    let file = vfs_path_to_file(db.upcast(), path)?;
    file_to_module(db, file)
}

/// Resolves the module for the file with the given id.
///
/// Returns `None` if the file is not a module locatable via `sys.path`.
#[salsa::tracked]
#[tracing::instrument(level = "debug", skip(db))]
pub fn file_to_module(db: &dyn Db, file: VfsFile) -> Option<Module> {
    let path = file.path(db.upcast());

    let search_paths = module_search_paths(db);

    let relative_path = search_paths
        .iter()
        .find_map(|root| match (root.path(), path) {
            (VfsPath::FileSystem(root_path), VfsPath::FileSystem(path)) => {
                let relative_path = path.strip_prefix(root_path).ok()?;
                Some(relative_path)
            }
            (VfsPath::Vendored(_), VfsPath::Vendored(_)) => {
                todo!("Add support for vendored modules")
            }
            (VfsPath::Vendored(_), VfsPath::FileSystem(_))
            | (VfsPath::FileSystem(_), VfsPath::Vendored(_)) => None,
        })?;

    let module_name = ModuleName::from_relative_path(relative_path)?;

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

/// Configures the [`ModuleSearchPath`]s that are used to resolve modules.
#[derive(Eq, PartialEq, Debug)]
pub struct ModuleResolutionSettings {
    /// List of user-provided paths that should take first priority in the module resolution.
    /// Examples in other type checkers are mypy's MYPYPATH environment variable,
    /// or pyright's stubPath configuration setting.
    pub extra_paths: Vec<FileSystemPathBuf>,

    /// The root of the workspace, used for finding first-party modules.
    pub workspace_root: FileSystemPathBuf,

    /// The path to the user's `site-packages` directory, where third-party packages from ``PyPI`` are installed.
    pub site_packages: Option<FileSystemPathBuf>,

    /// Optional path to standard-library typeshed stubs.
    /// Currently this has to be a directory that exists on disk.
    ///
    /// (TODO: fall back to vendored stubs if no custom directory is provided.)
    pub custom_typeshed: Option<FileSystemPathBuf>,
}

impl ModuleResolutionSettings {
    /// Implementation of PEP 561's module resolution order
    /// (with some small, deliberate, differences)
    fn into_ordered_search_paths(self) -> OrderedSearchPaths {
        let ModuleResolutionSettings {
            extra_paths,
            workspace_root,
            site_packages,
            custom_typeshed,
        } = self;

        let mut paths: Vec<_> = extra_paths
            .into_iter()
            .map(|path| ModuleSearchPath::new(path, ModuleSearchPathKind::Extra))
            .collect();

        paths.push(ModuleSearchPath::new(
            workspace_root,
            ModuleSearchPathKind::FirstParty,
        ));

        // TODO fallback to vendored typeshed stubs if no custom typeshed directory is provided by the user
        if let Some(custom_typeshed) = custom_typeshed {
            paths.push(ModuleSearchPath::new(
                custom_typeshed.join(TYPESHED_STDLIB_DIRECTORY),
                ModuleSearchPathKind::StandardLibrary,
            ));
        }

        // TODO vendor typeshed's third-party stubs as well as the stdlib and fallback to them as a final step
        if let Some(site_packages) = site_packages {
            paths.push(ModuleSearchPath::new(
                site_packages,
                ModuleSearchPathKind::SitePackagesThirdParty,
            ));
        }

        OrderedSearchPaths(paths)
    }
}

/// A resolved module resolution order, implementing PEP 561
/// (with some small, deliberate differences)
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct OrderedSearchPaths(Vec<ModuleSearchPath>);

impl Deref for OrderedSearchPaths {
    type Target = [ModuleSearchPath];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// The singleton methods generated by salsa are all `pub` instead of `pub(crate)` which triggers
// `unreachable_pub`. Work around this by creating a module and allow `unreachable_pub` for it.
// Salsa also generates uses to `_db` variables for `interned` which triggers `clippy::used_underscore_binding`. Suppress that too
// TODO(micha): Contribute a fix for this upstream where the singleton methods have the same visibility as the struct.
#[allow(unreachable_pub, clippy::used_underscore_binding)]
pub(crate) mod internal {
    use crate::module::resolver::OrderedSearchPaths;
    use crate::module::ModuleName;

    #[salsa::input(singleton)]
    pub(crate) struct ModuleResolverSearchPaths {
        #[return_ref]
        pub(super) search_paths: OrderedSearchPaths,
    }

    /// A thin wrapper around `ModuleName` to make it a Salsa ingredient.
    ///
    /// This is needed because Salsa requires that all query arguments are salsa ingredients.
    #[salsa::interned]
    pub(crate) struct ModuleNameIngredient {
        #[return_ref]
        pub(super) name: ModuleName,
    }
}

fn module_search_paths(db: &dyn Db) -> &[ModuleSearchPath] {
    ModuleResolverSearchPaths::get(db).search_paths(db)
}

/// Given a module name and a list of search paths in which to lookup modules,
/// attempt to resolve the module name
fn resolve_name(db: &dyn Db, name: &ModuleName) -> Option<(ModuleSearchPath, VfsFile, ModuleKind)> {
    let search_paths = module_search_paths(db);

    for search_path in search_paths {
        let mut components = name.components();
        let module_name = components.next_back()?;

        let VfsPath::FileSystem(fs_search_path) = search_path.path() else {
            todo!("Vendored search paths are not yet supported");
        };

        match resolve_package(db.file_system(), fs_search_path, components) {
            Ok(resolved_package) => {
                let mut package_path = resolved_package.path;

                package_path.push(module_name);

                // Must be a `__init__.pyi` or `__init__.py` or it isn't a package.
                let kind = if db.file_system().is_directory(&package_path) {
                    package_path.push("__init__");
                    ModuleKind::Package
                } else {
                    ModuleKind::Module
                };

                // TODO Implement full https://peps.python.org/pep-0561/#type-checker-module-resolution-order resolution
                let stub = package_path.with_extension("pyi");

                if let Some(stub) = file_system_path_to_file(db.upcast(), &stub) {
                    return Some((search_path.clone(), stub, kind));
                }

                let module = package_path.with_extension("py");

                if let Some(module) = file_system_path_to_file(db.upcast(), &module) {
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

fn resolve_package<'a, I>(
    fs: &dyn FileSystem,
    module_search_path: &FileSystemPath,
    components: I,
) -> Result<ResolvedPackage, PackageKind>
where
    I: Iterator<Item = &'a str>,
{
    let mut package_path = module_search_path.to_path_buf();

    // `true` if inside a folder that is a namespace package (has no `__init__.py`).
    // Namespace packages are special because they can be spread across multiple search paths.
    // https://peps.python.org/pep-0420/
    let mut in_namespace_package = false;

    // `true` if resolving a sub-package. For example, `true` when resolving `bar` of `foo.bar`.
    let mut in_sub_package = false;

    // For `foo.bar.baz`, test that `foo` and `baz` both contain a `__init__.py`.
    for folder in components {
        package_path.push(folder);

        let has_init_py = fs.is_file(&package_path.join("__init__.py"))
            || fs.is_file(&package_path.join("__init__.pyi"));

        if has_init_py {
            in_namespace_package = false;
        } else if fs.is_directory(&package_path) {
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
    path: FileSystemPathBuf,
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

    use ruff_db::file_system::{FileSystemPath, FileSystemPathBuf};
    use ruff_db::vfs::{file_system_path_to_file, VfsFile, VfsPath};

    use crate::db::tests::TestDb;
    use crate::module::{ModuleKind, ModuleName};

    use super::{
        path_to_module, resolve_module, set_module_resolution_settings, ModuleResolutionSettings,
        TYPESHED_STDLIB_DIRECTORY,
    };

    struct TestCase {
        db: TestDb,

        src: FileSystemPathBuf,
        custom_typeshed: FileSystemPathBuf,
        site_packages: FileSystemPathBuf,
    }

    fn create_resolver() -> std::io::Result<TestCase> {
        let mut db = TestDb::new();

        let src = FileSystemPath::new("src").to_path_buf();
        let site_packages = FileSystemPath::new("site_packages").to_path_buf();
        let custom_typeshed = FileSystemPath::new("typeshed").to_path_buf();

        let fs = db.memory_file_system();

        fs.create_directory_all(&src)?;
        fs.create_directory_all(&site_packages)?;
        fs.create_directory_all(&custom_typeshed)?;

        let settings = ModuleResolutionSettings {
            extra_paths: vec![],
            workspace_root: src.clone(),
            site_packages: Some(site_packages.clone()),
            custom_typeshed: Some(custom_typeshed.clone()),
        };

        set_module_resolution_settings(&mut db, settings);

        Ok(TestCase {
            db,
            src,
            custom_typeshed,
            site_packages,
        })
    }

    #[test]
    fn first_party_module() -> anyhow::Result<()> {
        let TestCase { db, src, .. } = create_resolver()?;

        let foo_path = src.join("foo.py");
        db.memory_file_system()
            .write_file(&foo_path, "print('Hello, world!')")?;

        let foo_module = resolve_module(&db, ModuleName::new("foo")).unwrap();

        assert_eq!(
            Some(&foo_module),
            resolve_module(&db, ModuleName::new("foo")).as_ref()
        );

        assert_eq!("foo", foo_module.name());
        assert_eq!(&src, foo_module.search_path().path());
        assert_eq!(ModuleKind::Module, foo_module.kind());
        assert_eq!(&foo_path, foo_module.file().path(&db));

        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &VfsPath::FileSystem(foo_path))
        );

        Ok(())
    }

    #[test]
    fn stdlib() -> anyhow::Result<()> {
        let TestCase {
            db,
            custom_typeshed,
            ..
        } = create_resolver()?;

        let stdlib_dir = custom_typeshed.join(TYPESHED_STDLIB_DIRECTORY);
        let functools_path = stdlib_dir.join("functools.py");
        db.memory_file_system()
            .write_file(&functools_path, "def update_wrapper(): ...")?;

        let functools_module = resolve_module(&db, ModuleName::new("functools")).unwrap();

        assert_eq!(
            Some(&functools_module),
            resolve_module(&db, ModuleName::new("functools")).as_ref()
        );

        assert_eq!(&stdlib_dir, functools_module.search_path().path());
        assert_eq!(ModuleKind::Module, functools_module.kind());
        assert_eq!(&functools_path.clone(), functools_module.file().path(&db));

        assert_eq!(
            Some(functools_module),
            path_to_module(&db, &VfsPath::FileSystem(functools_path))
        );

        Ok(())
    }

    #[test]
    fn first_party_precedence_over_stdlib() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            custom_typeshed,
            ..
        } = create_resolver()?;

        let stdlib_dir = custom_typeshed.join(TYPESHED_STDLIB_DIRECTORY);
        let stdlib_functools_path = stdlib_dir.join("functools.py");
        let first_party_functools_path = src.join("functools.py");

        db.memory_file_system().write_files([
            (&stdlib_functools_path, "def update_wrapper(): ..."),
            (&first_party_functools_path, "def update_wrapper(): ..."),
        ])?;

        let functools_module = resolve_module(&db, ModuleName::new("functools")).unwrap();

        assert_eq!(
            Some(&functools_module),
            resolve_module(&db, ModuleName::new("functools")).as_ref()
        );
        assert_eq!(&src, functools_module.search_path().path());
        assert_eq!(ModuleKind::Module, functools_module.kind());
        assert_eq!(
            &first_party_functools_path.clone(),
            functools_module.file().path(&db)
        );

        assert_eq!(
            Some(functools_module),
            path_to_module(&db, &VfsPath::FileSystem(first_party_functools_path))
        );

        Ok(())
    }

    // TODO: Port typeshed test case. Porting isn't possible at the moment because the vendored zip
    //   is part of the red knot crate
    // #[test]
    // fn typeshed_zip_created_at_build_time() -> anyhow::Result<()> {
    //     // The file path here is hardcoded in this crate's `build.rs` script.
    //     // Luckily this crate will fail to build if this file isn't available at build time.
    //     const TYPESHED_ZIP_BYTES: &[u8] =
    //         include_bytes!(concat!(env!("OUT_DIR"), "/zipped_typeshed.zip"));
    //     assert!(!TYPESHED_ZIP_BYTES.is_empty());
    //     let mut typeshed_zip_archive = ZipArchive::new(Cursor::new(TYPESHED_ZIP_BYTES))?;
    //
    //     let path_to_functools = Path::new("stdlib").join("functools.pyi");
    //     let mut functools_module_stub = typeshed_zip_archive
    //         .by_name(path_to_functools.to_str().unwrap())
    //         .unwrap();
    //     assert!(functools_module_stub.is_file());
    //
    //     let mut functools_module_stub_source = String::new();
    //     functools_module_stub.read_to_string(&mut functools_module_stub_source)?;
    //
    //     assert!(functools_module_stub_source.contains("def update_wrapper("));
    //     Ok(())
    // }

    #[test]
    fn resolve_package() -> anyhow::Result<()> {
        let TestCase { src, db, .. } = create_resolver()?;

        let foo_dir = src.join("foo");
        let foo_path = foo_dir.join("__init__.py");

        db.memory_file_system()
            .write_file(&foo_path, "print('Hello, world!')")?;

        let foo_module = resolve_module(&db, ModuleName::new("foo")).unwrap();

        assert_eq!("foo", foo_module.name());
        assert_eq!(&src, foo_module.search_path().path());
        assert_eq!(&foo_path, foo_module.file().path(&db));

        assert_eq!(
            Some(&foo_module),
            path_to_module(&db, &VfsPath::FileSystem(foo_path)).as_ref()
        );

        // Resolving by directory doesn't resolve to the init file.
        assert_eq!(None, path_to_module(&db, &VfsPath::FileSystem(foo_dir)));

        Ok(())
    }

    #[test]
    fn package_priority_over_module() -> anyhow::Result<()> {
        let TestCase { db, src, .. } = create_resolver()?;

        let foo_dir = src.join("foo");
        let foo_init = foo_dir.join("__init__.py");

        db.memory_file_system()
            .write_file(&foo_init, "print('Hello, world!')")?;

        let foo_py = src.join("foo.py");
        db.memory_file_system()
            .write_file(&foo_py, "print('Hello, world!')")?;

        let foo_module = resolve_module(&db, ModuleName::new("foo")).unwrap();

        assert_eq!(&src, foo_module.search_path().path());
        assert_eq!(&foo_init, foo_module.file().path(&db));
        assert_eq!(ModuleKind::Package, foo_module.kind());

        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &VfsPath::FileSystem(foo_init))
        );
        assert_eq!(None, path_to_module(&db, &VfsPath::FileSystem(foo_py)));

        Ok(())
    }

    #[test]
    fn typing_stub_over_module() -> anyhow::Result<()> {
        let TestCase { db, src, .. } = create_resolver()?;

        let foo_stub = src.join("foo.pyi");
        let foo_py = src.join("foo.py");
        db.memory_file_system()
            .write_files([(&foo_stub, "x: int"), (&foo_py, "print('Hello, world!')")])?;

        let foo = resolve_module(&db, ModuleName::new("foo")).unwrap();

        assert_eq!(&src, foo.search_path().path());
        assert_eq!(&foo_stub, foo.file().path(&db));

        assert_eq!(
            Some(foo),
            path_to_module(&db, &VfsPath::FileSystem(foo_stub))
        );
        assert_eq!(None, path_to_module(&db, &VfsPath::FileSystem(foo_py)));

        Ok(())
    }

    #[test]
    fn sub_packages() -> anyhow::Result<()> {
        let TestCase { db, src, .. } = create_resolver()?;

        let foo = src.join("foo");
        let bar = foo.join("bar");
        let baz = bar.join("baz.py");

        db.memory_file_system().write_files([
            (&foo.join("__init__.py"), ""),
            (&bar.join("__init__.py"), ""),
            (&baz, "print('Hello, world!')"),
        ])?;

        let baz_module = resolve_module(&db, ModuleName::new("foo.bar.baz")).unwrap();

        assert_eq!(&src, baz_module.search_path().path());
        assert_eq!(&baz, baz_module.file().path(&db));

        assert_eq!(
            Some(baz_module),
            path_to_module(&db, &VfsPath::FileSystem(baz))
        );

        Ok(())
    }

    #[test]
    fn namespace_package() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            site_packages,
            ..
        } = create_resolver()?;

        // From [PEP420](https://peps.python.org/pep-0420/#nested-namespace-packages).
        // But uses `src` for `project1` and `site_packages2` for `project2`.
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

        let parent1 = src.join("parent");
        let child1 = parent1.join("child");
        let one = child1.join("one.py");

        let parent2 = site_packages.join("parent");
        let child2 = parent2.join("child");
        let two = child2.join("two.py");

        db.memory_file_system().write_files([
            (&one, "print('Hello, world!')"),
            (&two, "print('Hello, world!')"),
        ])?;

        let one_module = resolve_module(&db, ModuleName::new("parent.child.one")).unwrap();

        assert_eq!(
            Some(one_module),
            path_to_module(&db, &VfsPath::FileSystem(one))
        );

        let two_module = resolve_module(&db, ModuleName::new("parent.child.two")).unwrap();
        assert_eq!(
            Some(two_module),
            path_to_module(&db, &VfsPath::FileSystem(two))
        );

        Ok(())
    }

    #[test]
    fn regular_package_in_namespace_package() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            site_packages,
            ..
        } = create_resolver()?;

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

        let parent1 = src.join("parent");
        let child1 = parent1.join("child");
        let one = child1.join("one.py");

        let parent2 = site_packages.join("parent");
        let child2 = parent2.join("child");
        let two = child2.join("two.py");

        db.memory_file_system().write_files([
            (&child1.join("__init__.py"), "print('Hello, world!')"),
            (&one, "print('Hello, world!')"),
            (&two, "print('Hello, world!')"),
        ])?;

        let one_module = resolve_module(&db, ModuleName::new("parent.child.one")).unwrap();

        assert_eq!(
            Some(one_module),
            path_to_module(&db, &VfsPath::FileSystem(one))
        );

        assert_eq!(
            None,
            resolve_module(&db, ModuleName::new("parent.child.two"))
        );
        Ok(())
    }

    #[test]
    fn module_search_path_priority() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            site_packages,
            ..
        } = create_resolver()?;

        let foo_src = src.join("foo.py");
        let foo_site_packages = site_packages.join("foo.py");

        db.memory_file_system()
            .write_files([(&foo_src, ""), (&foo_site_packages, "")])?;

        let foo_module = resolve_module(&db, ModuleName::new("foo")).unwrap();

        assert_eq!(&src, foo_module.search_path().path());
        assert_eq!(&foo_src, foo_module.file().path(&db));

        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &VfsPath::FileSystem(foo_src))
        );
        assert_eq!(
            None,
            path_to_module(&db, &VfsPath::FileSystem(foo_site_packages))
        );

        Ok(())
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn symlink() -> anyhow::Result<()> {
        let TestCase {
            mut db,
            src,
            site_packages,
            custom_typeshed,
        } = create_resolver()?;

        db.with_os_file_system();

        let temp_dir = tempfile::tempdir()?;
        let root = FileSystemPath::from_std_path(temp_dir.path()).unwrap();

        let src = root.join(src);
        let site_packages = root.join(site_packages);
        let custom_typeshed = root.join(custom_typeshed);

        let foo = src.join("foo.py");
        let bar = src.join("bar.py");

        std::fs::create_dir_all(src.as_std_path())?;
        std::fs::create_dir_all(site_packages.as_std_path())?;
        std::fs::create_dir_all(custom_typeshed.as_std_path())?;

        std::fs::write(foo.as_std_path(), "")?;
        std::os::unix::fs::symlink(foo.as_std_path(), bar.as_std_path())?;

        let settings = ModuleResolutionSettings {
            extra_paths: vec![],
            workspace_root: src.clone(),
            site_packages: Some(site_packages),
            custom_typeshed: Some(custom_typeshed),
        };

        set_module_resolution_settings(&mut db, settings);

        let foo_module = resolve_module(&db, ModuleName::new("foo")).unwrap();
        let bar_module = resolve_module(&db, ModuleName::new("bar")).unwrap();

        assert_ne!(foo_module, bar_module);

        assert_eq!(&src, foo_module.search_path().path());
        assert_eq!(&foo, foo_module.file().path(&db));

        // `foo` and `bar` shouldn't resolve to the same file

        assert_eq!(&src, bar_module.search_path().path());
        assert_eq!(&bar, bar_module.file().path(&db));
        assert_eq!(&foo, foo_module.file().path(&db));

        assert_ne!(&foo_module, &bar_module);

        assert_eq!(
            Some(foo_module),
            path_to_module(&db, &VfsPath::FileSystem(foo))
        );
        assert_eq!(
            Some(bar_module),
            path_to_module(&db, &VfsPath::FileSystem(bar))
        );

        Ok(())
    }

    #[test]
    fn deleting_an_unrealted_file_doesnt_change_module_resolution() -> anyhow::Result<()> {
        let TestCase { mut db, src, .. } = create_resolver()?;

        let foo_path = src.join("foo.py");
        let bar_path = src.join("bar.py");

        db.memory_file_system()
            .write_files([(&foo_path, "x = 1"), (&bar_path, "y = 2")])?;

        let foo_module = resolve_module(&db, ModuleName::new("foo")).unwrap();

        let bar = file_system_path_to_file(&db, &bar_path).expect("bar.py to exist");

        db.clear_salsa_events();

        // Delete `bar.py`
        db.memory_file_system().remove_file(&bar_path)?;
        bar.touch(&mut db);

        // Re-query the foo module. The foo module should still be cached because `bar.py` isn't relevant
        // for resolving `foo`.

        let foo_module2 = resolve_module(&db, ModuleName::new("foo"));

        assert!(!db
            .take_sale_events()
            .iter()
            .any(|event| { matches!(event.kind, salsa::EventKind::WillExecute { .. }) }));

        assert_eq!(Some(foo_module), foo_module2);

        Ok(())
    }

    #[test]
    fn adding_a_file_on_which_the_module_resolution_depends_on_invalidates_the_query(
    ) -> anyhow::Result<()> {
        let TestCase { mut db, src, .. } = create_resolver()?;
        let foo_path = src.join("foo.py");

        assert_eq!(resolve_module(&db, ModuleName::new("foo")), None);

        // Now write the foo file
        db.memory_file_system().write_file(&foo_path, "x = 1")?;
        VfsFile::touch_path(&mut db, &VfsPath::FileSystem(foo_path.clone()));
        let foo_file = file_system_path_to_file(&db, &foo_path).expect("foo.py to exist");

        let foo_module =
            resolve_module(&db, ModuleName::new("foo")).expect("Foo module to resolve");
        assert_eq!(foo_file, foo_module.file());

        Ok(())
    }

    #[test]
    fn removing_a_file_that_the_module_resolution_depends_on_invalidates_the_query(
    ) -> anyhow::Result<()> {
        let TestCase { mut db, src, .. } = create_resolver()?;
        let foo_path = src.join("foo.py");
        let foo_init_path = src.join("foo/__init__.py");

        db.memory_file_system()
            .write_files([(&foo_path, "x = 1"), (&foo_init_path, "x = 2")])?;

        let foo_module = resolve_module(&db, ModuleName::new("foo")).expect("foo module to exist");

        assert_eq!(&foo_init_path, foo_module.file().path(&db));

        // Delete `foo/__init__.py` and the `foo` folder. `foo` should now resolve to `foo.py`
        db.memory_file_system().remove_file(&foo_init_path)?;
        db.memory_file_system()
            .remove_directory(foo_init_path.parent().unwrap())?;
        VfsFile::touch_path(&mut db, &VfsPath::FileSystem(foo_init_path.clone()));

        let foo_module =
            resolve_module(&db, ModuleName::new("foo")).expect("Foo module to resolve");
        assert_eq!(&foo_path, foo_module.file().path(&db));

        Ok(())
    }
}
