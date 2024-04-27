use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use dashmap::mapref::entry::Entry;

use crate::db::{HasJar, SemanticDb, SemanticJar};
use crate::files::FileId;
use crate::FxDashMap;

/// ID uniquely identifying a module.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Module(u32);

impl Module {
    pub fn name<Db>(&self, db: &Db) -> ModuleName
    where
        Db: HasJar<SemanticJar>,
    {
        let modules = &db.jar().module_resolver;

        modules.modules.get(self).unwrap().name.clone()
    }

    pub fn path<Db>(&self, db: &Db) -> ModulePath
    where
        Db: HasJar<SemanticJar>,
    {
        let modules = &db.jar().module_resolver;

        modules.modules.get(self).unwrap().path.clone()
    }
}

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form (never a relative module name).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ModuleName(smol_str::SmolStr);

impl ModuleName {
    pub fn new(name: &str) -> Self {
        debug_assert!(!name.is_empty());

        Self(smol_str::SmolStr::new(name))
    }

    pub fn relative(_dots: u32, name: &str, _to: &Path) -> Self {
        // FIXME: Take `to` and `dots` into account.
        Self(smol_str::SmolStr::new(name))
    }

    pub fn from_relative_path(path: &Path) -> Option<Self> {
        let path = if path.ends_with("__init__.py") || path.ends_with("__init__.pyi") {
            path.parent()?
        } else {
            path
        };

        let name = if let Some(parent) = path.parent() {
            let mut name = String::with_capacity(path.as_os_str().len());

            for component in parent.components() {
                name.push_str(component.as_os_str().to_str()?);
                name.push('.');
            }

            // SAFETY: Unwrap is safe here or `parent` would have returned `None`.
            name.push_str(path.file_stem().unwrap().to_str()?);

            smol_str::SmolStr::from(name)
        } else {
            smol_str::SmolStr::new(path.file_stem()?.to_str()?)
        };

        Some(Self(name))
    }

    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('.')
    }

    pub fn parent(&self) -> Option<ModuleName> {
        let (_, parent) = self.0.rsplit_once('.')?;

        Some(Self(smol_str::SmolStr::new(parent)))
    }

    pub fn starts_with(&self, other: &ModuleName) -> bool {
        self.0.starts_with(other.0.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
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
    pub fn new(path: PathBuf, kind: ModuleSearchPathKind) -> Self {
        Self {
            inner: Arc::new(ModuleSearchPathInner { path, kind }),
        }
    }

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
pub enum ModuleSearchPathKind {
    // Project dependency
    FirstParty,

    // e.g. site packages
    ThirdParty,

    // e.g. built-in modules, typeshed
    StandardLibrary,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ModuleData {
    name: ModuleName,
    path: ModulePath,
}

//////////////////////////////////////////////////////
// Queries
//////////////////////////////////////////////////////

/// Resolves a module name to a module id
/// TODO: This would not work with Salsa because `ModuleName` isn't an ingredient and, therefore, cannot be used as part of a query.
///  For this to work with salsa, it would be necessary to intern all `ModuleName`s.
#[tracing::instrument(level = "debug", skip(db))]
pub fn resolve_module<Db>(db: &Db, name: ModuleName) -> Option<Module>
where
    Db: SemanticDb + HasJar<SemanticJar>,
{
    let jar = db.jar();
    let modules = &jar.module_resolver;

    let entry = modules.by_name.entry(name.clone());

    match entry {
        Entry::Occupied(entry) => Some(*entry.get()),
        Entry::Vacant(entry) => {
            let (root_path, absolute_path) = resolve_name(&name, &modules.search_paths)?;
            let normalized = absolute_path.canonicalize().ok()?;

            let file_id = db.file_id(&normalized);
            let path = ModulePath::new(root_path.clone(), file_id);

            let id = Module(
                modules
                    .next_module_id
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            );

            modules
                .modules
                .insert(id, Arc::from(ModuleData { name, path }));

            // A path can map to multiple modules because of symlinks:
            // ```
            // foo.py
            // bar.py -> foo.py
            // ```
            // Here, both `foo` and `bar` resolve to the same module but through different paths.
            // That's why we need to insert the absolute path and not the normalized path here.
            modules.by_path.insert(absolute_path, id);

            entry.insert_entry(id);

            Some(id)
        }
    }
}

//////////////////////////////////////////////////////
// Mutations
//////////////////////////////////////////////////////

/// Changes the module search paths to `search_paths`.
pub fn set_module_search_paths<Db>(db: &mut Db, search_paths: Vec<ModuleSearchPath>)
where
    Db: SemanticDb + HasJar<SemanticJar>,
{
    let jar = db.jar_mut();

    jar.module_resolver = ModuleResolver::new(search_paths);
}

/// Resolves the module id for the file with the given id.
///
/// Returns `None` if the file is not a module in `sys.path`.
pub fn file_to_module<Db>(db: &mut Db, file: FileId) -> Option<Module>
where
    Db: SemanticDb + HasJar<SemanticJar>,
{
    let path = db.file_path(file);
    path_to_module(db, &path)
}

/// Resolves the module id for the given path.
///
/// Returns `None` if the path is not a module in `sys.path`.
// WARNING!: It's important that this method takes `&mut self`. Without, the implementation is prone to race conditions.
// Note: This won't work with salsa because `Path` is not an ingredient.
pub fn path_to_module<Db>(db: &mut Db, path: &Path) -> Option<Module>
where
    Db: SemanticDb + HasJar<SemanticJar>,
{
    let jar = db.jar_mut();
    let modules = &mut jar.module_resolver;
    debug_assert!(path.is_absolute());

    if let Some(existing) = modules.by_path.get(path) {
        return Some(*existing);
    }

    let root_path = modules
        .search_paths
        .iter()
        .find(|root| path.starts_with(root.path()))?
        .clone();

    // SAFETY: `strip_prefix` is guaranteed to succeed because we search the root that is a prefix of the path.
    let relative_path = path.strip_prefix(root_path.path()).unwrap();
    let module_name = ModuleName::from_relative_path(relative_path)?;

    // Resolve the module name to see if Python would resolve the name to the same path.
    // If it doesn't, then that means that multiple modules have the same in different
    // root paths, but that the module corresponding to the past path is in a lower priority path,
    // in which case we ignore it.
    let module_id = resolve_module(db, module_name)?;
    // Note: Guaranteed to be race-free because we're holding a mutable reference of `self` here.
    let module_path = module_id.path(db);

    if module_path.root() == &root_path {
        let normalized = path.canonicalize().ok()?;
        let interned_normalized = db.file_id(&normalized);

        if interned_normalized != module_path.file() {
            // This path is for a module with the same name but with a different precedence. For example:
            // ```
            // src/foo.py
            // src/foo/__init__.py
            // ```
            // The module name of `src/foo.py` is `foo`, but the module loaded by Python is `src/foo/__init__.py`.
            // That means we need to ignore `src/foo.py` even though it resolves to the same module name.
            return None;
        }

        // Path has been inserted by `resolved`
        Some(module_id)
    } else {
        // This path is for a module with the same name but in a module search path with a lower priority.
        // Ignore it.
        None
    }
}

/// Adds a module to the resolver.
///
/// Returns `None` if the path doesn't resolve to a module.
///
/// Returns `Some` with the id of the module and the ids of the modules that need re-resolving
/// because they were part of a namespace package and might now resolve differently.
/// Note: This won't work with salsa because `Path` is not an ingredient.
pub fn add_module<Db>(db: &mut Db, path: &Path) -> Option<(Module, Vec<Arc<ModuleData>>)>
where
    Db: SemanticDb + HasJar<SemanticJar>,
{
    // No locking is required because we're holding a mutable reference to `modules`.

    // TODO This needs tests

    // Note: Intentionally by-pass caching here. Module should not be in the cache yet.
    let module = path_to_module(db, path)?;

    // The code below is to handle the addition of `__init__.py` files.
    // When an `__init__.py` file is added, we need to remove all modules that are part of the same package.
    // For example, an `__init__.py` is added to `foo`, we need to remove `foo.bar`, `foo.baz`, etc.
    // because they were namespace packages before and could have been from different search paths.
    let Some(filename) = path.file_name() else {
        return Some((module, Vec::new()));
    };

    if !matches!(filename.to_str(), Some("__init__.py" | "__init__.pyi")) {
        return Some((module, Vec::new()));
    }

    let Some(parent_name) = module.name(db).parent() else {
        return Some((module, Vec::new()));
    };

    let mut to_remove = Vec::new();

    let jar = db.jar_mut();
    let modules = &mut jar.module_resolver;

    modules.by_path.retain(|_, id| {
        if modules
            .modules
            .get(id)
            .unwrap()
            .name
            .starts_with(&parent_name)
        {
            to_remove.push(*id);
            false
        } else {
            true
        }
    });

    // TODO remove need for this vec
    let mut removed = Vec::with_capacity(to_remove.len());
    for id in &to_remove {
        removed.push(modules.remove_module_by_id(*id));
    }

    Some((module, removed))
}

#[derive(Default)]
pub struct ModuleResolver {
    /// The search paths where modules are located (and searched). Corresponds to `sys.path` at runtime.
    search_paths: Vec<ModuleSearchPath>,

    // Locking: Locking is done by acquiring a (write) lock on `by_name`. This is because `by_name` is the primary
    // lookup method. Acquiring locks in any other ordering can result in deadlocks.
    /// Resolves a module name to it's module id.
    by_name: FxDashMap<ModuleName, Module>,

    /// All known modules, indexed by the module id.
    modules: FxDashMap<Module, Arc<ModuleData>>,

    /// Lookup from absolute path to module.
    /// The same module might be reachable from different paths when symlinks are involved.
    by_path: FxDashMap<PathBuf, Module>,
    next_module_id: AtomicU32,
}

impl ModuleResolver {
    pub fn new(search_paths: Vec<ModuleSearchPath>) -> Self {
        Self {
            search_paths,
            modules: FxDashMap::default(),
            by_name: FxDashMap::default(),
            by_path: FxDashMap::default(),
            next_module_id: AtomicU32::new(0),
        }
    }

    pub fn remove_module(&mut self, path: &Path) {
        // No locking is required because we're holding a mutable reference to `self`.
        let Some((_, id)) = self.by_path.remove(path) else {
            return;
        };

        self.remove_module_by_id(id);
    }

    fn remove_module_by_id(&mut self, id: Module) -> Arc<ModuleData> {
        let (_, module) = self.modules.remove(&id).unwrap();

        self.by_name.remove(&module.name).unwrap();

        // It's possible that multiple paths map to the same id. Search all other paths referencing the same module id.
        self.by_path.retain(|_, current_id| *current_id != id);

        module
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for ModuleResolver {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleResolver")
            .field("search_paths", &self.search_paths)
            .field("modules", &self.by_name)
            .finish()
    }
}

/// The resolved path of a module.
///
/// It should be highly likely that the file still exists when accessing but it isn't 100% guaranteed
/// because the file could have been deleted between resolving the module name and accessing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModulePath {
    root: ModuleSearchPath,
    file_id: FileId,
}

impl ModulePath {
    pub fn new(root: ModuleSearchPath, file_id: FileId) -> Self {
        Self { root, file_id }
    }

    pub fn root(&self) -> &ModuleSearchPath {
        &self.root
    }

    pub fn file(&self) -> FileId {
        self.file_id
    }
}

fn resolve_name(
    name: &ModuleName,
    search_paths: &[ModuleSearchPath],
) -> Option<(ModuleSearchPath, PathBuf)> {
    for search_path in search_paths {
        let mut components = name.components();
        let module_name = components.next_back()?;

        match resolve_package(search_path, components) {
            Ok(resolved_package) => {
                let mut package_path = resolved_package.path;

                package_path.push(module_name);

                // Must be a `__init__.pyi` or `__init__.py` or it isn't a package.
                if package_path.is_dir() {
                    package_path.push("__init__");
                }

                // TODO Implement full https://peps.python.org/pep-0561/#type-checker-module-resolution-order resolution
                let stub = package_path.with_extension("pyi");

                if stub.is_file() {
                    return Some((search_path.clone(), stub));
                }

                let module = package_path.with_extension("py");

                if module.is_file() {
                    return Some((search_path.clone(), module));
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

        let has_init_py = package_path.join("__init__.py").is_file()
            || package_path.join("__init__.pyi").is_file();

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
struct ResolvedPackage {
    path: PathBuf,
    kind: PackageKind,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum PackageKind {
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
    const fn is_regular_package(self) -> bool {
        matches!(self, PackageKind::Regular)
    }
}

#[cfg(test)]
mod tests {
    use crate::db::tests::TestDb;
    use crate::db::{SemanticDb, SourceDb};
    use crate::module::{ModuleName, ModuleSearchPath, ModuleSearchPathKind};

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

        let mut db = TestDb::default();
        db.set_module_search_paths(roots);

        Ok(TestCase {
            temp_dir,
            db,
            src,
            site_packages,
        })
    }

    #[test]
    fn first_party_module() -> std::io::Result<()> {
        let TestCase {
            mut db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_path = src.path().join("foo.py");
        std::fs::write(&foo_path, "print('Hello, world!')")?;

        let foo_module = db.resolve_module(ModuleName::new("foo")).unwrap();

        assert_eq!(Some(foo_module), db.resolve_module(ModuleName::new("foo")));

        assert_eq!(ModuleName::new("foo"), foo_module.name(&db));
        assert_eq!(&src, foo_module.path(&db).root());
        assert_eq!(&foo_path, &*db.file_path(foo_module.path(&db).file()));

        assert_eq!(Some(foo_module), db.path_to_module(&foo_path));

        Ok(())
    }

    #[test]
    fn resolve_package() -> std::io::Result<()> {
        let TestCase {
            src,
            mut db,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_dir = src.path().join("foo");
        let foo_path = foo_dir.join("__init__.py");
        std::fs::create_dir(&foo_dir)?;
        std::fs::write(&foo_path, "print('Hello, world!')")?;

        let foo_module = db.resolve_module(ModuleName::new("foo")).unwrap();

        assert_eq!(ModuleName::new("foo"), foo_module.name(&db));
        assert_eq!(&src, foo_module.path(&db).root());
        assert_eq!(&foo_path, &*db.file_path(foo_module.path(&db).file()));

        assert_eq!(Some(foo_module), db.path_to_module(&foo_path));

        // Resolving by directory doesn't resolve to the init file.
        assert_eq!(None, db.path_to_module(&foo_dir));

        Ok(())
    }

    #[test]
    fn package_priority_over_module() -> std::io::Result<()> {
        let TestCase {
            mut db,
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

        let foo_module = db.resolve_module(ModuleName::new("foo")).unwrap();

        assert_eq!(&src, foo_module.path(&db).root());
        assert_eq!(&foo_init, &*db.file_path(foo_module.path(&db).file()));

        assert_eq!(Some(foo_module), db.path_to_module(&foo_init));
        assert_eq!(None, db.path_to_module(&foo_py));

        Ok(())
    }

    #[test]
    fn typing_stub_over_module() -> std::io::Result<()> {
        let TestCase {
            mut db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_stub = src.path().join("foo.pyi");
        let foo_py = src.path().join("foo.py");
        std::fs::write(&foo_stub, "x: int")?;
        std::fs::write(&foo_py, "print('Hello, world!')")?;

        let foo = db.resolve_module(ModuleName::new("foo")).unwrap();

        assert_eq!(&src, foo.path(&db).root());
        assert_eq!(&foo_stub, &*db.file_path(foo.path(&db).file()));

        assert_eq!(Some(foo), db.path_to_module(&foo_stub));
        assert_eq!(None, db.path_to_module(&foo_py));

        Ok(())
    }

    #[test]
    fn sub_packages() -> std::io::Result<()> {
        let TestCase {
            mut db,
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

        let baz_module = db.resolve_module(ModuleName::new("foo.bar.baz")).unwrap();

        assert_eq!(&src, baz_module.path(&db).root());
        assert_eq!(&baz, &*db.file_path(baz_module.path(&db).file()));

        assert_eq!(Some(baz_module), db.path_to_module(&baz));

        Ok(())
    }

    #[test]
    fn namespace_package() -> std::io::Result<()> {
        let TestCase {
            mut db,
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

        let one_module = db
            .resolve_module(ModuleName::new("parent.child.one"))
            .unwrap();

        assert_eq!(Some(one_module), db.path_to_module(&one));

        let two_module = db
            .resolve_module(ModuleName::new("parent.child.two"))
            .unwrap();
        assert_eq!(Some(two_module), db.path_to_module(&two));

        Ok(())
    }

    #[test]
    fn regular_package_in_namespace_package() -> std::io::Result<()> {
        let TestCase {
            mut db,
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

        let one_module = db
            .resolve_module(ModuleName::new("parent.child.one"))
            .unwrap();

        assert_eq!(Some(one_module), db.path_to_module(&one));

        assert_eq!(None, db.resolve_module(ModuleName::new("parent.child.two")));
        Ok(())
    }

    #[test]
    fn module_search_path_priority() -> std::io::Result<()> {
        let TestCase {
            mut db,
            src,
            site_packages,
            temp_dir: _temp_dir,
        } = create_resolver()?;

        let foo_src = src.path().join("foo.py");
        let foo_site_packages = site_packages.path().join("foo.py");

        std::fs::write(&foo_src, "")?;
        std::fs::write(&foo_site_packages, "")?;

        let foo_module = db.resolve_module(ModuleName::new("foo")).unwrap();

        assert_eq!(&src, foo_module.path(&db).root());
        assert_eq!(&foo_src, &*db.file_path(foo_module.path(&db).file()));

        assert_eq!(Some(foo_module), db.path_to_module(&foo_src));
        assert_eq!(None, db.path_to_module(&foo_site_packages));

        Ok(())
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn symlink() -> std::io::Result<()> {
        let TestCase {
            mut db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo = src.path().join("foo.py");
        let bar = src.path().join("bar.py");

        std::fs::write(&foo, "")?;
        std::os::unix::fs::symlink(&foo, &bar)?;

        let foo_module = db.resolve_module(ModuleName::new("foo")).unwrap();
        let bar_module = db.resolve_module(ModuleName::new("bar")).unwrap();

        assert_ne!(foo_module, bar_module);

        assert_eq!(&src, foo_module.path(&db).root());
        assert_eq!(&foo, &*db.file_path(foo_module.path(&db).file()));

        // Bar has a different name but it should point to the same file.

        assert_eq!(&src, bar_module.path(&db).root());
        assert_eq!(foo_module.path(&db).file(), bar_module.path(&db).file());
        assert_eq!(&foo, &*db.file_path(bar_module.path(&db).file()));

        assert_eq!(Some(foo_module), db.path_to_module(&foo));
        assert_eq!(Some(bar_module), db.path_to_module(&bar));

        Ok(())
    }
}
