use std::fmt::Formatter;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use dashmap::mapref::entry::Entry;
use smol_str::SmolStr;

use crate::db::{QueryResult, SemanticDb, SemanticJar};
use crate::files::FileId;
use crate::symbols::Dependency;
use crate::FxDashMap;

/// Representation of a Python module.
///
/// The inner type wrapped by this struct is a unique identifier for the module
/// that is used by the struct's methods to lazily query information about the module.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Module(u32);

impl Module {
    /// Return the absolute name of the module (e.g. `foo.bar`)
    pub fn name(&self, db: &dyn SemanticDb) -> QueryResult<ModuleName> {
        let jar: &SemanticJar = db.jar()?;
        let modules = &jar.module_resolver;

        Ok(modules.modules.get(self).unwrap().name.clone())
    }

    /// Return the path to the source code that defines this module
    pub fn path(&self, db: &dyn SemanticDb) -> QueryResult<ModulePath> {
        let jar: &SemanticJar = db.jar()?;
        let modules = &jar.module_resolver;

        Ok(modules.modules.get(self).unwrap().path.clone())
    }

    /// Determine whether this module is a single-file module or a package
    pub fn kind(&self, db: &dyn SemanticDb) -> QueryResult<ModuleKind> {
        let jar: &SemanticJar = db.jar()?;
        let modules = &jar.module_resolver;

        Ok(modules.modules.get(self).unwrap().kind)
    }

    /// Attempt to resolve a dependency of this module to an absolute [`ModuleName`].
    ///
    /// A dependency could be either absolute (e.g. the `foo` dependency implied by `from foo import bar`)
    /// or relative to this module (e.g. the `.foo` dependency implied by `from .foo import bar`)
    ///
    /// - Returns an error if the query failed.
    /// - Returns `Ok(None)` if the query succeeded,
    ///   but the dependency refers to a module that does not exist.
    /// - Returns `Ok(Some(ModuleName))` if the query succeeded,
    ///   and the dependency refers to a module that exists.
    pub fn resolve_dependency(
        &self,
        db: &dyn SemanticDb,
        dependency: &Dependency,
    ) -> QueryResult<Option<ModuleName>> {
        let (level, module) = match dependency {
            Dependency::Module(module) => return Ok(Some(module.clone())),
            Dependency::Relative { level, module } => (*level, module.as_deref()),
        };

        let name = self.name(db)?;
        let kind = self.kind(db)?;

        let mut components = name.components().peekable();

        let start = match kind {
            // `.` resolves to the enclosing package
            ModuleKind::Module => 0,
            // `.` resolves to the current package
            ModuleKind::Package => 1,
        };

        // Skip over the relative parts.
        for _ in start..level.get() {
            if components.next_back().is_none() {
                return Ok(None);
            }
        }

        let mut name = String::new();

        for part in components.chain(module) {
            if !name.is_empty() {
                name.push('.');
            }

            name.push_str(part);
        }

        Ok(if name.is_empty() {
            None
        } else {
            Some(ModuleName(SmolStr::new(name)))
        })
    }
}

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form
/// (never a relative module name, i.e., never `.foo`).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ModuleName(smol_str::SmolStr);

impl ModuleName {
    pub fn new(name: &str) -> Self {
        debug_assert!(!name.is_empty());

        Self(smol_str::SmolStr::new(name))
    }

    // TODO(Micha): Make private again when the Salsa db module resolution logic lives next to this module again.
    pub(crate) fn from_relative_path(path: &Path) -> Option<Self> {
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

    /// An iterator over the components of the module name:
    /// `foo.bar.baz` -> `foo`, `bar`, `baz`
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('.')
    }

    /// The name of this module's immediate parent, if it has a parent
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

impl Deref for ModuleName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ModuleKind {
    /// A single-file module (e.g. `foo.py` or `foo.pyi`)
    Module,

    /// A python package (`foo/__init__.py` or `foo/__init__.pyi`)
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
    pub fn new(path: PathBuf, kind: ModuleSearchPathKind) -> Self {
        Self {
            inner: Arc::new(ModuleSearchPathInner { path, kind }),
        }
    }

    /// Determine whether this is a first-party, third-party or standard-library search path
    pub fn kind(&self) -> ModuleSearchPathKind {
        self.inner.kind
    }

    /// Return the location of the search path on the file system
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

impl ModuleSearchPathKind {
    pub const fn is_first_party(self) -> bool {
        matches!(self, Self::FirstParty)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ModuleData {
    name: ModuleName,
    path: ModulePath,
    kind: ModuleKind,
}

//////////////////////////////////////////////////////
// Queries
//////////////////////////////////////////////////////

/// Resolves a module name to a module.
///
/// TODO: This would not work with Salsa because `ModuleName` isn't an ingredient
/// and, therefore, cannot be used as part of a query.
/// For this to work with salsa, it would be necessary to intern all `ModuleName`s.
#[tracing::instrument(level = "debug", skip(db))]
pub fn resolve_module(db: &dyn SemanticDb, name: ModuleName) -> QueryResult<Option<Module>> {
    let jar: &SemanticJar = db.jar()?;
    let modules = &jar.module_resolver;

    let entry = modules.by_name.entry(name.clone());

    match entry {
        Entry::Occupied(entry) => Ok(Some(*entry.get())),
        Entry::Vacant(entry) => {
            let Some((root_path, absolute_path, kind)) = resolve_name(&name, &modules.search_paths)
            else {
                return Ok(None);
            };
            let Ok(normalized) = absolute_path.canonicalize() else {
                return Ok(None);
            };

            let file_id = db.file_id(&normalized);
            let path = ModulePath::new(root_path.clone(), file_id);

            let module = Module(
                modules
                    .next_module_id
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            );

            modules
                .modules
                .insert(module, Arc::from(ModuleData { name, path, kind }));

            // A path can map to multiple modules because of symlinks:
            // ```
            // foo.py
            // bar.py -> foo.py
            // ```
            // Here, both `foo` and `bar` resolve to the same module but through different paths.
            // That's why we need to insert the absolute path and not the normalized path here.
            let absolute_file_id = if absolute_path == normalized {
                file_id
            } else {
                db.file_id(&absolute_path)
            };

            modules.by_file.insert(absolute_file_id, module);

            entry.insert_entry(module);

            Ok(Some(module))
        }
    }
}

/// Resolves the module for the given path.
///
/// Returns `None` if the path is not a module locatable via `sys.path`.
#[tracing::instrument(level = "debug", skip(db))]
pub fn path_to_module(db: &dyn SemanticDb, path: &Path) -> QueryResult<Option<Module>> {
    let file = db.file_id(path);
    file_to_module(db, file)
}

/// Resolves the module for the file with the given id.
///
/// Returns `None` if the file is not a module locatable via `sys.path`.
#[tracing::instrument(level = "debug", skip(db))]
pub fn file_to_module(db: &dyn SemanticDb, file: FileId) -> QueryResult<Option<Module>> {
    let jar: &SemanticJar = db.jar()?;
    let modules = &jar.module_resolver;

    if let Some(existing) = modules.by_file.get(&file) {
        return Ok(Some(*existing));
    }

    let path = db.file_path(file);

    debug_assert!(path.is_absolute());

    let Some((root_path, relative_path)) = modules.search_paths.iter().find_map(|root| {
        let relative_path = path.strip_prefix(root.path()).ok()?;
        Some((root.clone(), relative_path))
    }) else {
        return Ok(None);
    };

    let Some(module_name) = ModuleName::from_relative_path(relative_path) else {
        return Ok(None);
    };

    // Resolve the module name to see if Python would resolve the name to the same path.
    // If it doesn't, then that means that multiple modules have the same in different
    // root paths, but that the module corresponding to the past path is in a lower priority search path,
    // in which case we ignore it.
    let Some(module) = resolve_module(db, module_name)? else {
        return Ok(None);
    };
    let module_path = module.path(db)?;

    if module_path.root() == &root_path {
        let Ok(normalized) = path.canonicalize() else {
            return Ok(None);
        };
        let interned_normalized = db.file_id(&normalized);

        if interned_normalized != module_path.file() {
            // This path is for a module with the same name but with a different precedence. For example:
            // ```
            // src/foo.py
            // src/foo/__init__.py
            // ```
            // The module name of `src/foo.py` is `foo`, but the module loaded by Python is `src/foo/__init__.py`.
            // That means we need to ignore `src/foo.py` even though it resolves to the same module name.
            return Ok(None);
        }

        // Path has been inserted by `resolved`
        Ok(Some(module))
    } else {
        // This path is for a module with the same name but in a module search path with a lower priority.
        // Ignore it.
        Ok(None)
    }
}

//////////////////////////////////////////////////////
// Mutations
//////////////////////////////////////////////////////

/// Changes the module search paths to `search_paths`.
pub fn set_module_search_paths(db: &mut dyn SemanticDb, search_paths: Vec<ModuleSearchPath>) {
    let jar: &mut SemanticJar = db.jar_mut();

    jar.module_resolver = ModuleResolver::new(search_paths);
}

/// Adds a module located at `path` to the resolver.
///
/// Returns `None` if the path doesn't resolve to a module.
///
/// Returns `Some(module, other_modules)`, where `module` is the resolved module
/// with file location `path`, and `other_modules` is a `Vec` of `ModuleData` instances.
/// Each element in `other_modules` provides information regarding a single module that needs
/// re-resolving because it was part of a namespace package and might now resolve differently.
///
/// Note: This won't work with salsa because `Path` is not an ingredient.
pub fn add_module(db: &mut dyn SemanticDb, path: &Path) -> Option<(Module, Vec<Arc<ModuleData>>)> {
    // No locking is required because we're holding a mutable reference to `modules`.

    // TODO This needs tests

    // Note: Intentionally bypass caching here. Module should not be in the cache yet.
    let module = path_to_module(db, path).ok()??;

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

    let Some(parent_name) = module.name(db).ok()?.parent() else {
        return Some((module, Vec::new()));
    };

    let mut to_remove = Vec::new();

    let jar: &mut SemanticJar = db.jar_mut();
    let modules = &mut jar.module_resolver;

    modules.by_file.retain(|_, module| {
        if modules
            .modules
            .get(module)
            .unwrap()
            .name
            .starts_with(&parent_name)
        {
            to_remove.push(*module);
            false
        } else {
            true
        }
    });

    // TODO remove need for this vec
    let mut removed = Vec::with_capacity(to_remove.len());
    for module in &to_remove {
        removed.push(modules.remove_module(*module));
    }

    Some((module, removed))
}

#[derive(Default)]
pub struct ModuleResolver {
    /// The search paths where modules are located (and searched). Corresponds to `sys.path` at runtime.
    search_paths: Vec<ModuleSearchPath>,

    // Locking: Locking is done by acquiring a (write) lock on `by_name`. This is because `by_name` is the primary
    // lookup method. Acquiring locks in any other ordering can result in deadlocks.
    /// Looks up a module by name
    by_name: FxDashMap<ModuleName, Module>,

    /// A map of all known modules to data about those modules
    modules: FxDashMap<Module, Arc<ModuleData>>,

    /// Lookup from absolute path to module.
    /// The same module might be reachable from different paths when symlinks are involved.
    by_file: FxDashMap<FileId, Module>,
    next_module_id: AtomicU32,
}

impl ModuleResolver {
    pub fn new(search_paths: Vec<ModuleSearchPath>) -> Self {
        Self {
            search_paths,
            modules: FxDashMap::default(),
            by_name: FxDashMap::default(),
            by_file: FxDashMap::default(),
            next_module_id: AtomicU32::new(0),
        }
    }

    /// Remove a module from the inner cache
    pub(crate) fn remove_module_by_file(&mut self, file_id: FileId) {
        // No locking is required because we're holding a mutable reference to `self`.
        let Some((_, module)) = self.by_file.remove(&file_id) else {
            return;
        };

        self.remove_module(module);
    }

    fn remove_module(&mut self, module: Module) -> Arc<ModuleData> {
        let (_, module_data) = self.modules.remove(&module).unwrap();

        self.by_name.remove(&module_data.name).unwrap();

        // It's possible that multiple paths map to the same module.
        // Search all other paths referencing the same module.
        self.by_file
            .retain(|_, current_module| *current_module != module);

        module_data
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

    /// The search path that was used to locate the module
    pub fn root(&self) -> &ModuleSearchPath {
        &self.root
    }

    /// The file containing the source code for the module
    pub fn file(&self) -> FileId {
        self.file_id
    }
}

/// Given a module name and a list of search paths in which to lookup modules,
/// attempt to resolve the module name
fn resolve_name(
    name: &ModuleName,
    search_paths: &[ModuleSearchPath],
) -> Option<(ModuleSearchPath, PathBuf, ModuleKind)> {
    for search_path in search_paths {
        let mut components = name.components();
        let module_name = components.next_back()?;

        match resolve_package(search_path, components) {
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
                let stub = package_path.with_extension("pyi");

                if stub.is_file() {
                    return Some((search_path.clone(), stub, kind));
                }

                let module = package_path.with_extension("py");

                if module.is_file() {
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
pub struct ResolvedPackage {
    pub path: PathBuf,
    pub kind: PackageKind,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PackageKind {
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
    pub const fn is_regular_package(self) -> bool {
        matches!(self, PackageKind::Regular)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use crate::db::tests::TestDb;
    use crate::db::SourceDb;
    use crate::module::{
        path_to_module, resolve_module, set_module_search_paths, ModuleKind, ModuleName,
        ModuleSearchPath, ModuleSearchPathKind,
    };
    use crate::symbols::Dependency;

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

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(
            Some(foo_module),
            resolve_module(&db, ModuleName::new("foo"))?
        );

        assert_eq!(ModuleName::new("foo"), foo_module.name(&db)?);
        assert_eq!(&src, foo_module.path(&db)?.root());
        assert_eq!(ModuleKind::Module, foo_module.kind(&db)?);
        assert_eq!(&foo_path, &*db.file_path(foo_module.path(&db)?.file()));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_path)?);

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

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(ModuleName::new("foo"), foo_module.name(&db)?);
        assert_eq!(&src, foo_module.path(&db)?.root());
        assert_eq!(&foo_path, &*db.file_path(foo_module.path(&db)?.file()));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_path)?);

        // Resolving by directory doesn't resolve to the init file.
        assert_eq!(None, path_to_module(&db, &foo_dir)?);

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

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(&src, foo_module.path(&db)?.root());
        assert_eq!(&foo_init, &*db.file_path(foo_module.path(&db)?.file()));
        assert_eq!(ModuleKind::Package, foo_module.kind(&db)?);

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_init)?);
        assert_eq!(None, path_to_module(&db, &foo_py)?);

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

        let foo = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(&src, foo.path(&db)?.root());
        assert_eq!(&foo_stub, &*db.file_path(foo.path(&db)?.file()));

        assert_eq!(Some(foo), path_to_module(&db, &foo_stub)?);
        assert_eq!(None, path_to_module(&db, &foo_py)?);

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

        let baz_module = resolve_module(&db, ModuleName::new("foo.bar.baz"))?.unwrap();

        assert_eq!(&src, baz_module.path(&db)?.root());
        assert_eq!(&baz, &*db.file_path(baz_module.path(&db)?.file()));

        assert_eq!(Some(baz_module), path_to_module(&db, &baz)?);

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

        let one_module = resolve_module(&db, ModuleName::new("parent.child.one"))?.unwrap();

        assert_eq!(Some(one_module), path_to_module(&db, &one)?);

        let two_module = resolve_module(&db, ModuleName::new("parent.child.two"))?.unwrap();
        assert_eq!(Some(two_module), path_to_module(&db, &two)?);

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

        let one_module = resolve_module(&db, ModuleName::new("parent.child.one"))?.unwrap();

        assert_eq!(Some(one_module), path_to_module(&db, &one)?);

        assert_eq!(
            None,
            resolve_module(&db, ModuleName::new("parent.child.two"))?
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

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(&src, foo_module.path(&db)?.root());
        assert_eq!(&foo_src, &*db.file_path(foo_module.path(&db)?.file()));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_src)?);
        assert_eq!(None, path_to_module(&db, &foo_site_packages)?);

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

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();
        let bar_module = resolve_module(&db, ModuleName::new("bar"))?.unwrap();

        assert_ne!(foo_module, bar_module);

        assert_eq!(&src, foo_module.path(&db)?.root());
        assert_eq!(&foo, &*db.file_path(foo_module.path(&db)?.file()));

        // Bar has a different name but it should point to the same file.

        assert_eq!(&src, bar_module.path(&db)?.root());
        assert_eq!(foo_module.path(&db)?.file(), bar_module.path(&db)?.file());
        assert_eq!(&foo, &*db.file_path(bar_module.path(&db)?.file()));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo)?);
        assert_eq!(Some(bar_module), path_to_module(&db, &bar)?);

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

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();
        let bar_module = resolve_module(&db, ModuleName::new("foo.bar"))?.unwrap();

        // `from . import bar` in `foo/__init__.py` resolves to `foo`
        assert_eq!(
            Some(ModuleName::new("foo")),
            foo_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: None,
                }
            )?
        );

        // `from baz import bar` in `foo/__init__.py` should resolve to `baz.py`
        assert_eq!(
            Some(ModuleName::new("baz")),
            foo_module.resolve_dependency(&db, &Dependency::Module(ModuleName::new("baz")))?
        );

        // from .bar import test in `foo/__init__.py` should resolve to `foo/bar.py`
        assert_eq!(
            Some(ModuleName::new("foo.bar")),
            foo_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: Some(ModuleName::new("bar"))
                }
            )?
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
            )?
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
            )?
        );

        // `from baz import test` in `foo/bar.py` resolves to `baz`
        assert_eq!(
            Some(ModuleName::new("baz")),
            bar_module.resolve_dependency(&db, &Dependency::Module(ModuleName::new("baz")))?
        );

        // `from .baz import test` in `foo/bar.py` resolves to `foo.baz`.
        assert_eq!(
            Some(ModuleName::new("foo.baz")),
            bar_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: Some(ModuleName::new("baz"))
                }
            )?
        );

        Ok(())
    }
}
