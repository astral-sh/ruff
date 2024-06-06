use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::module::ModuleName;
use crate::salsa_db::semantic::symbol_table::Dependency;
use crate::salsa_db::source::File;

use super::Db;
use super::Jar;

/// ID uniquely identifying a module.
#[salsa::interned(jar=Jar)]
pub struct Module {
    #[return_ref]
    name: ModuleName,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ResolvedModule {
    inner: Arc<ResolveModuleInner>,
}

#[derive(Eq, PartialEq)]
struct ResolveModuleInner {
    module: Module,
    kind: ModuleKind,
    search_path: ModuleSearchPath,
    file: File,
}

impl std::fmt::Debug for ResolvedModule {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedModule")
            .field("module", &self.module())
            .field("kind", &self.kind())
            .field("search_path", &self.search_path())
            .field("file", &self.file())
            .finish()
    }
}

impl ResolvedModule {
    pub fn module(&self) -> Module {
        self.inner.module
    }

    pub fn kind(&self) -> ModuleKind {
        self.inner.kind
    }

    pub fn search_path(&self) -> &ModuleSearchPath {
        &self.inner.search_path
    }

    pub fn file(&self) -> File {
        self.inner.file
    }

    pub fn resolve_dependency(&self, db: &dyn Db, dependency: &Dependency) -> Option<ModuleName> {
        let (level, module) = match dependency {
            // FIXME use clone here
            Dependency::Module(module) => return Some(ModuleName::new(module)),
            Dependency::Relative { level, module } => (*level, module.as_deref()),
        };

        let mut components = self.module().name(db).components().peekable();

        let start = match self.kind() {
            // `.` resolves to the enclosing package
            ModuleKind::Module => 0,
            // `.` resolves to the current package
            ModuleKind::Package => 1,
        };

        // Skip over the relative parts.
        for _ in start..level.get() {
            components.next_back()?;
        }

        let mut name = String::new();

        for part in components.chain(module) {
            if !name.is_empty() {
                name.push('.');
            }

            name.push_str(part);
        }

        if name.is_empty() {
            None
        } else {
            Some(ModuleName::new(&name))
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ModuleKind {
    Module,

    /// A python package (a `__init__.py` or `__init__.pyi` file)
    Package,
}

/// A search path in which to search modules.
/// Corresponds to a path in [`sys.path`](https://docs.python.org/3/library/sys_path_init.html) at runtime.
///
/// Cloning a search path is cheap because it's an `Arc`.
#[derive(Clone, PartialEq, Eq)]
pub struct ModuleSearchPath {
    inner: Arc<ModuleSearchPathInner>,
}

impl ModuleSearchPath {
    #[allow(unused)]
    pub fn new(path: PathBuf, kind: ModuleSearchPathKind) -> Self {
        Self {
            inner: Arc::new(ModuleSearchPathInner { path, kind }),
        }
    }

    #[allow(unused)]
    pub fn kind(&self) -> ModuleSearchPathKind {
        self.inner.kind
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }
}

impl std::fmt::Debug for ModuleSearchPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ModuleSearchPathInner {
    path: PathBuf,
    kind: ModuleSearchPathKind,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[allow(unused)]
pub enum ModuleSearchPathKind {
    // Project dependency
    FirstParty,

    // e.g. site packages
    ThirdParty,

    // e.g. built-in modules, typeshed
    #[allow(unused)]
    StandardLibrary,
}

impl ModuleSearchPathKind {
    #[allow(unused)]
    pub const fn is_first_party(self) -> bool {
        matches!(self, Self::FirstParty)
    }
}
/// ID uniquely identifying a module.
#[salsa::input(jar=Jar, singleton)]
pub struct ModuleSearchPaths {
    #[return_ref]
    pub paths: Vec<ModuleSearchPath>,
}

pub fn module_search_paths(db: &dyn Db) -> &[ModuleSearchPath] {
    ModuleSearchPaths::get(db).paths(db).as_slice()
}

/// Changes the module search paths to `search_paths`.
#[allow(unused)]
pub fn set_module_search_paths(db: &mut dyn Db, search_paths: Vec<ModuleSearchPath>) {
    if let Some(existing) = ModuleSearchPaths::try_get(db) {
        existing.set_paths(db).to(search_paths);
    } else {
        ModuleSearchPaths::new(db, search_paths);
    }
}

#[allow(unused)]
pub fn resolve_module_name(db: &dyn Db, name: ModuleName) -> Option<ResolvedModule> {
    resolve_module(db, Module::new(db, name))
}

/// Resolves a module name to a module id
#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar)]
pub fn resolve_module(db: &dyn Db, module: Module) -> Option<ResolvedModule> {
    let name = module.name(db);

    let (root_path, resolved_file, kind) = resolve_module_path(db, name)?;

    let normalized = resolved_file
        .path(db.upcast())
        .canonicalize()
        .map(|path| db.file(path))
        .unwrap_or_else(|_| resolved_file);

    Some(ResolvedModule {
        inner: Arc::new(ResolveModuleInner {
            module,
            kind,
            search_path: root_path,
            file: normalized,
        }),
    })
}

/// Resolves the module id for the given path.
///
/// Returns `None` if the path is not a module in `sys.path`.
#[tracing::instrument(level = "debug", skip(db))]
pub fn path_to_module(db: &dyn Db, path: &Path) -> Option<ResolvedModule> {
    let file = db.file(path.to_path_buf());
    file_to_module(db, file)
}

/// Resolves the module id for the file with the given id.
#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar)]
pub fn file_to_module(db: &dyn Db, file: File) -> Option<ResolvedModule> {
    let path = file.path(db.upcast());
    debug_assert!(path.is_absolute());

    let (root_path, relative_path) = module_search_paths(db).iter().find_map(|root| {
        let relative_path = path.strip_prefix(root.path()).ok()?;
        Some((root.clone(), relative_path))
    })?;

    let module_name = ModuleName::from_relative_path(relative_path)?;

    // Resolve the module name to see if Python would resolve the name to the same path.
    // If it doesn't, then that means that multiple modules have the same in different
    // root paths, but that the module corresponding to the past path is in a lower priority path,
    // in which case we ignore it.
    let module = resolve_module(db, Module::new(db, module_name))?;

    if module.search_path() == &root_path {
        let normalized = path
            .canonicalize()
            .map(|path| db.file(path))
            .unwrap_or(file);

        if normalized != module.file() {
            // This path is for a module with the same name but with a different precedence. For example:
            // ```
            // src/foo.py
            // src/foo/__init__.py
            // ```
            // The module name of `src/foo.py` is `foo`, but the module loaded by Python is `src/foo/__init__.py`.
            // That means we need to ignore `src/foo.py` even though it resolves to the same module name.
            return None;
        }

        Some(module)
    } else {
        // This path is for a module with the same name but in a module search path with a lower priority.
        // Ignore it.
        None
    }
}

fn resolve_module_path(
    db: &dyn Db,
    name: &ModuleName,
) -> Option<(ModuleSearchPath, File, ModuleKind)> {
    let search_paths = module_search_paths(db);

    for search_path in search_paths {
        let mut components = name.components();
        let module_name = components.next_back()?;

        match resolve_package(db, search_path, components) {
            Ok(resolved_package) => {
                let mut package_path = resolved_package.path;

                package_path.push(module_name);

                // Must be a `__init__.pyi` or `__init__.py` or it isn't a package.
                let kind = if package_path.is_dir() {
                    package_path.push("__init__");
                    ModuleKind::Package
                } else {
                    ModuleKind::Module
                };

                // TODO Implement full https://peps.python.org/pep-0561/#type-checker-module-resolution-order resolution
                let stub = db.file(package_path.with_extension("pyi"));

                if stub.exists(db.upcast()) {
                    return Some((search_path.clone(), stub, kind));
                }

                let module = db.file(package_path.with_extension("py"));

                if module.exists(db.upcast()) {
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
    db: &dyn Db,
    module_search_path: &ModuleSearchPath,
    components: I,
) -> Result<ResolvedPackage, PackageKind>
where
    I: Iterator<Item = &'a str>,
{
    let mut package_path = module_search_path.path().to_path_buf();

    // `true` if inside a folder that is a namespace package (has no `__init__.py`).
    // Namespace packages are special because they can be spread across multiple search paths.
    // https://peps.python.org/pep-0420/
    let mut in_namespace_package = false;

    // `true` if resolving a sub-package. For example, `true` when resolving `bar` of `foo.bar`.
    let mut in_sub_package = false;

    // For `foo.bar.baz`, test that `foo` and `baz` both contain a `__init__.py`.
    for folder in components {
        package_path.push(folder);

        let has_init_py = db
            .file(package_path.join("__init__.py"))
            .exists(db.upcast())
            || db
                .file(package_path.join("__init__.pyi"))
                .exists(db.upcast());

        if has_init_py {
            in_namespace_package = false;
        } else if package_path.is_dir() {
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
pub struct ResolvedPackage {
    pub path: PathBuf,
    pub kind: PackageKind,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PackageKind {
    /// A root package or module. E.g. `foo` in `foo.bar.baz` or just `foo`.
    Root,

    /// A regular sub-package where the parent contains an `__init__.py`. For example `bar` in `foo.bar` when the `foo` directory contains an `__init__.py`.
    Regular,

    /// A sub-package in a namespace package. A namespace package is a package without an `__init__.py`.
    ///
    /// For example, `bar` in `foo.bar` if the `foo` directory contains no `__init__.py`.
    Namespace,
}

impl PackageKind {
    pub const fn is_regular_package(self) -> bool {
        matches!(self, PackageKind::Regular)
    }
}

#[cfg(test)]
mod tests {
    use crate::salsa_db::semantic::symbol_table::Dependency;
    use crate::salsa_db::source::Db;
    use salsa::DebugWithDb;
    use std::num::NonZeroU32;

    use crate::salsa_db::tests::TestDb;

    use super::{
        path_to_module, resolve_module_name, set_module_search_paths, ModuleKind, ModuleName,
        ModuleSearchPath, ModuleSearchPathKind,
    };

    struct TestCase {
        temp_dir: tempfile::TempDir,
        db: TestDb,

        src: ModuleSearchPath,
        site_packages: ModuleSearchPath,
    }

    fn create_resolver() -> std::io::Result<TestCase> {
        let temp_dir = tempfile::tempdir()?;

        let src = temp_dir.path().join("src");
        let site_packages = temp_dir.path().join("site_packages");

        std::fs::create_dir(&src)?;
        std::fs::create_dir(&site_packages)?;

        let src = ModuleSearchPath::new(src.canonicalize()?, ModuleSearchPathKind::FirstParty);
        let site_packages = ModuleSearchPath::new(
            site_packages.canonicalize()?,
            ModuleSearchPathKind::ThirdParty,
        );

        let roots = vec![src.clone(), site_packages.clone()];

        let mut db = TestDb::new();
        set_module_search_paths(&mut db, roots);

        Ok(TestCase {
            temp_dir,
            db,
            src,
            site_packages,
        })
    }

    #[test]
    fn first_party_module() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_path = src.path().join("foo.py");
        std::fs::write(&foo_path, "print('Hello, world!')")?;

        let foo_module = resolve_module_name(&db, ModuleName::new("foo")).unwrap();

        assert_eq!(
            Some(foo_module.clone()),
            resolve_module_name(&db, ModuleName::new("foo"))
        );

        assert_eq!(&ModuleName::new("foo"), foo_module.module().name(&db));
        assert_eq!(&src, foo_module.search_path());
        assert_eq!(ModuleKind::Module, foo_module.kind());
        assert_eq!(&foo_path, foo_module.file().path(&db));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_path));

        Ok(())
    }

    #[test]
    fn resolve_package() -> anyhow::Result<()> {
        let TestCase {
            src,
            db,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_dir = src.path().join("foo");
        let foo_path = foo_dir.join("__init__.py");
        std::fs::create_dir(&foo_dir)?;
        std::fs::write(&foo_path, "print('Hello, world!')")?;

        let foo_module = resolve_module_name(&db, ModuleName::new("foo")).unwrap();

        assert_eq!(&ModuleName::new("foo"), foo_module.module().name(&db));
        assert_eq!(&src, foo_module.search_path());
        assert_eq!(&foo_path, foo_module.file().path(&db));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_path));

        // Resolving by directory doesn't resolve to the init file.
        assert_eq!(None, path_to_module(&db, &foo_dir));

        Ok(())
    }

    #[test]
    fn package_priority_over_module() -> anyhow::Result<()> {
        let TestCase {
            db,
            temp_dir: _temp_dir,
            src,
            ..
        } = create_resolver()?;

        let foo_dir = src.path().join("foo");
        let foo_init = foo_dir.join("__init__.py");
        std::fs::create_dir(&foo_dir)?;
        std::fs::write(&foo_init, "print('Hello, world!')")?;

        let foo_py = src.path().join("foo.py");
        std::fs::write(&foo_py, "print('Hello, world!')")?;

        let foo_module = resolve_module_name(&db, ModuleName::new("foo")).unwrap();

        assert_eq!(&src, foo_module.search_path());
        assert_eq!(&foo_init, foo_module.file().path(&db));
        assert_eq!(ModuleKind::Package, foo_module.kind());

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_init));
        assert_eq!(None, path_to_module(&db, &foo_py));

        Ok(())
    }

    #[test]
    fn typing_stub_over_module() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_stub = src.path().join("foo.pyi");
        let foo_py = src.path().join("foo.py");
        std::fs::write(&foo_stub, "x: int")?;
        std::fs::write(&foo_py, "print('Hello, world!')")?;

        let foo = resolve_module_name(&db, ModuleName::new("foo")).unwrap();

        assert_eq!(&src, foo.search_path());
        assert_eq!(&foo_stub, foo.file().path(&db));

        assert_eq!(Some(foo), path_to_module(&db, &foo_stub));
        assert_eq!(None, path_to_module(&db, &foo_py));

        Ok(())
    }

    #[test]
    fn sub_packages() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo = src.path().join("foo");
        let bar = foo.join("bar");
        let baz = bar.join("baz.py");

        std::fs::create_dir_all(&bar)?;
        std::fs::write(foo.join("__init__.py"), "")?;
        std::fs::write(bar.join("__init__.py"), "")?;
        std::fs::write(&baz, "print('Hello, world!')")?;

        let baz_module = resolve_module_name(&db, ModuleName::new("foo.bar.baz")).unwrap();

        assert_eq!(&src, baz_module.search_path());
        assert_eq!(&baz, baz_module.file().path(&db));

        assert_eq!(Some(baz_module), path_to_module(&db, &baz));

        Ok(())
    }

    #[test]
    fn namespace_package() -> anyhow::Result<()> {
        let TestCase {
            db,
            temp_dir: _,
            src,
            site_packages,
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

        let parent1 = src.path().join("parent");
        let child1 = parent1.join("child");
        let one = child1.join("one.py");

        std::fs::create_dir_all(child1)?;
        std::fs::write(&one, "print('Hello, world!')")?;

        let parent2 = site_packages.path().join("parent");
        let child2 = parent2.join("child");
        let two = child2.join("two.py");

        std::fs::create_dir_all(&child2)?;
        std::fs::write(&two, "print('Hello, world!')")?;

        let one_module = resolve_module_name(&db, ModuleName::new("parent.child.one")).unwrap();

        assert_eq!(Some(one_module), path_to_module(&db, &one));

        let two_module = resolve_module_name(&db, ModuleName::new("parent.child.two")).unwrap();
        assert_eq!(Some(two_module), path_to_module(&db, &two));

        Ok(())
    }

    #[test]
    fn regular_package_in_namespace_package() -> anyhow::Result<()> {
        let TestCase {
            db,
            temp_dir: _,
            src,
            site_packages,
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

        let parent1 = src.path().join("parent");
        let child1 = parent1.join("child");
        let one = child1.join("one.py");

        std::fs::create_dir_all(&child1)?;
        std::fs::write(child1.join("__init__.py"), "print('Hello, world!')")?;
        std::fs::write(&one, "print('Hello, world!')")?;

        let parent2 = site_packages.path().join("parent");
        let child2 = parent2.join("child");
        let two = child2.join("two.py");

        std::fs::create_dir_all(&child2)?;
        std::fs::write(two, "print('Hello, world!')")?;

        let one_module = resolve_module_name(&db, ModuleName::new("parent.child.one")).unwrap();

        assert_eq!(Some(one_module), path_to_module(&db, &one));

        assert_eq!(
            None,
            resolve_module_name(&db, ModuleName::new("parent.child.two"))
        );
        Ok(())
    }

    #[test]
    fn module_search_path_priority() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            site_packages,
            temp_dir: _temp_dir,
        } = create_resolver()?;

        let foo_src = src.path().join("foo.py");
        let foo_site_packages = site_packages.path().join("foo.py");

        std::fs::write(&foo_src, "")?;
        std::fs::write(&foo_site_packages, "")?;

        let foo_module = resolve_module_name(&db, ModuleName::new("foo")).unwrap();

        assert_eq!(&src, foo_module.search_path());
        assert_eq!(&foo_src, foo_module.file().path(&db));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_src));
        assert_eq!(None, path_to_module(&db, &foo_site_packages));

        Ok(())
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn symlink() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo = src.path().join("foo.py");
        let bar = src.path().join("bar.py");

        std::fs::write(&foo, "")?;
        std::os::unix::fs::symlink(&foo, &bar)?;

        let foo_module = resolve_module_name(&db, ModuleName::new("foo")).unwrap();
        let bar_module = resolve_module_name(&db, ModuleName::new("bar")).unwrap();

        assert_ne!(foo_module, bar_module);

        assert_eq!(&src, foo_module.search_path());
        assert_eq!(&foo, foo_module.file().path(&db));

        // Bar has a different name but it should point to the same file.

        assert_eq!(&src, bar_module.search_path());
        assert_eq!(foo_module.file(), bar_module.file());
        assert_eq!(&foo, bar_module.file().path(&db));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo));
        assert_eq!(Some(bar_module), path_to_module(&db, &bar));

        Ok(())
    }

    #[test]
    fn resolve_dependency() -> anyhow::Result<()> {
        let TestCase {
            src,
            db,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_dir = src.path().join("foo");
        let foo_path = foo_dir.join("__init__.py");
        let bar_path = foo_dir.join("bar.py");

        std::fs::create_dir(&foo_dir)?;
        std::fs::write(foo_path, "from .bar import test")?;
        std::fs::write(bar_path, "test = 'Hello world'")?;

        let foo_module = resolve_module_name(&db, ModuleName::new("foo")).unwrap();
        let bar_module = resolve_module_name(&db, ModuleName::new("foo.bar")).unwrap();

        // `from . import bar` in `foo/__init__.py` resolves to `foo`
        assert_eq!(
            Some(ModuleName::new("foo")),
            foo_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: None,
                }
            )
        );

        // `from baz import bar` in `foo/__init__.py` should resolve to `baz.py`
        assert_eq!(
            Some(ModuleName::new("baz")),
            foo_module.resolve_dependency(
                &db,
                &Dependency::Module(crate::module::ModuleName::new("baz"))
            )
        );

        // from .bar import test in `foo/__init__.py` should resolve to `foo/bar.py`
        assert_eq!(
            Some(ModuleName::new("foo.bar")),
            foo_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: Some(crate::module::ModuleName::new("bar"))
                }
            )
        );

        // from .. import test in `foo/__init__.py` resolves to `` which is not a module
        assert_eq!(
            None,
            foo_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(2).unwrap(),
                    module: None
                }
            )
        );

        // `from . import test` in `foo/bar.py` resolves to `foo`
        assert_eq!(
            Some(ModuleName::new("foo")),
            bar_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: None
                }
            )
        );

        // `from baz import test` in `foo/bar.py` resolves to `baz`
        assert_eq!(
            Some(ModuleName::new("baz")),
            bar_module.resolve_dependency(
                &db,
                &Dependency::Module(crate::module::ModuleName::new("baz"))
            )
        );

        // `from .baz import test` in `foo/bar.py` resolves to `foo.baz`.
        assert_eq!(
            Some(ModuleName::new("foo.baz")),
            bar_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: Some(crate::module::ModuleName::new("baz"))
                }
            )
        );

        Ok(())
    }

    #[test]
    fn cache_invalidation() -> anyhow::Result<()> {
        let TestCase {
            mut db,
            temp_dir: _,
            src,
            site_packages,
        } = create_resolver()?;

        let parent1 = src.path().join("parent");
        let one = parent1.join("one.py");

        std::fs::create_dir_all(&parent1)?;
        std::fs::write(&one, "print('Hello, world!')")?;

        let parent2 = site_packages.path().join("parent");
        let two = parent2.join("two.py");

        std::fs::create_dir_all(&parent2)?;
        std::fs::write(&two, "print('Hello, world!')")?;

        let one_module = resolve_module_name(&db, ModuleName::new("parent.one")).unwrap();

        assert_eq!(Some(one_module), path_to_module(&db, &one));

        let two_module = resolve_module_name(&db, ModuleName::new("parent.two")).unwrap();
        assert_eq!(Some(two_module), path_to_module(&db, &two));

        // Add an `__init__.py` to the `site_packages/parent` folder, which makes it a non-namespace package and
        // should invalidate `two_module`.

        let parent1_init = parent1.join("__init__.py");
        std::fs::write(&parent1_init, "")?;

        let init_file = db.file(parent1_init);
        init_file.touch(&mut db);
        init_file.debug(&db);

        assert_eq!(
            None,
            resolve_module_name(&db, ModuleName::new("parent.two"))
        );
        assert_eq!(None, path_to_module(&db, &two));

        Ok(())
    }
}
