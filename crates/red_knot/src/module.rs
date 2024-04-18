#![allow(unreachable_pub)]

use std::collections::hash_map::Entry;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::files::{FileId, Files};
use filetime::FileTime;
use rustc_hash::FxHashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ModuleId(u32);

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form (never a relative module name).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ModuleName(smol_str::SmolStr);

#[allow(unused)]
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
        let name = if let Some(parent) = path.parent() {
            let mut name = String::new();

            for component in parent.components() {
                if !name.is_empty() {
                    name.push('.');
                }

                name.push_str(component.as_os_str().to_str()?);
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

    pub fn as_str(&self) -> &str {
        &self.0
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

#[allow(unused)]
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

#[allow(unused)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ModuleSearchPathKind {
    // Project dependency
    FirstParty,

    // e.g. site packages
    ThirdParty,

    // e.g. built-in modules, typeshed
    StandardLibrary,
}

#[derive(Clone, Debug)]
pub struct Module {
    name: ModuleName,
    path: ModulePath,
    last_modified: FileTime,
}

#[allow(unused)]
impl Module {
    pub fn name(&self) -> &ModuleName {
        &self.name
    }

    pub fn path(&self) -> &ModulePath {
        &self.path
    }

    pub fn last_modified(&self) -> FileTime {
        self.last_modified
    }
}

#[derive(Debug, Default)]
pub struct ModuleResolver {
    /// The search paths where modules are located (and searched). Corresponds to `sys.path` at runtime.
    search_paths: Vec<ModuleSearchPath>,

    files: Files,

    /// All known modules, indexed by the module id.
    modules: FxHashMap<ModuleId, Module>,

    /// Resolves a module name to it's module id.
    by_name: FxHashMap<ModuleName, ModuleId>,

    /// Lookup from absolute path to module.
    /// The same module might be reachable from different paths when symlinks are involved.
    by_path: FxHashMap<PathBuf, ModuleId>,
    next_module_id: u32,
}

#[allow(unused)]
impl ModuleResolver {
    pub fn new(search_paths: Vec<ModuleSearchPath>, files: Files) -> Self {
        Self {
            search_paths,
            files,
            modules: FxHashMap::default(),
            by_name: FxHashMap::default(),
            by_path: FxHashMap::default(),
            next_module_id: 0,
        }
    }

    /// Resolves a module name to a module id.
    // TODO I think resolving should take a referenc because we don't have a mutable `Modules` during type evaluation.
    pub fn resolve(&mut self, name: ModuleName) -> Option<ModuleId> {
        // we can't really accept `&mut Files` here but we also don't want to traverse the entire site packages and index all files eagerly.
        let entry = self.by_name.entry(name.clone());

        match entry {
            Entry::Occupied(existing) => Some(*existing.get()),
            Entry::Vacant(vacant) => {
                let (root_path, absolute_path) = resolve_name(&name, &self.search_paths)?;
                let normalized = absolute_path.canonicalize().ok()?;

                let file_id = self.files.intern(&normalized);

                let metadata = normalized.metadata().ok()?;
                let last_modified = FileTime::from_last_modification_time(&metadata);

                let path = ModulePath::new(root_path.clone(), file_id);

                let id = ModuleId(self.next_module_id);
                self.next_module_id += 1;

                self.modules.insert(
                    id,
                    Module {
                        name,
                        path,
                        last_modified,
                    },
                );

                // A path can map to multiple modules because of symlinks:
                // ```
                // foo.py
                // bar.py -> foo.py
                // ```
                // Here, both `foo` and `bar` resolve to the same module but through different paths.
                // That's why we need to insert the absolute path and not the normalized path here.
                self.by_path.insert(absolute_path, id);

                vacant.insert(id);
                Some(id)
            }
        }
    }

    pub fn id(&self, name: &ModuleName) -> Option<ModuleId> {
        self.by_name.get(name).copied()
    }

    /// Returns the module for a given id.
    pub fn module(&self, id: ModuleId) -> &Module {
        &self.modules[&id]
    }

    pub fn path(&self, id: ModuleId) -> Arc<Path> {
        self.files.path(self.module(id).path().file())
    }

    /// Updates the last modified time for the module corresponding to `path`.
    pub fn touch(&mut self, path: &Path) {
        let Some(id) = self.resolve_path(path) else {
            return;
        };

        let module = self.modules.get_mut(&id).unwrap();
        let module_path = self.files.path(module.path.file());

        if let Ok(metadata) = module_path.metadata() {
            module.last_modified = FileTime::from_last_modification_time(&metadata);
        };
    }

    pub fn add_module_path(&mut self, path: &Path) {
        self.resolve_path(path);
    }

    pub fn resolve_file(&mut self, file: FileId) -> Option<ModuleId> {
        let path = self.files.path(file);
        self.by_path.get(&*path).copied()
    }

    pub fn resolve_path(&mut self, path: &Path) -> Option<ModuleId> {
        debug_assert!(path.is_absolute());

        if let Some(existing) = self.by_path.get(path) {
            return Some(*existing);
        }

        let root_path = self
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
        let module_id = self.resolve(module_name)?;
        let module = &self.modules[&module_id];

        if module.path().root() == &root_path {
            let normalized = path.canonicalize().ok()?;
            let module_path = self.files.path(module.path().file());

            if !module_path.starts_with(normalized) {
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

    pub fn remove_module_path(&mut self, id: ModuleId) {
        let module = self.modules.remove(&id).unwrap();

        self.by_name.remove(&module.name);

        // This isn't fast but removing seems an uncommon operation where I don't think it's worth
        // to keep a reverse lookup just for it (requires more allocations, involves writing a lot of data).
        self.by_path.retain(|_, current_id| *current_id != id);
    }
}

fn resolve_name(
    name: &ModuleName,
    search_paths: &[ModuleSearchPath],
) -> Option<(ModuleSearchPath, PathBuf)> {
    'search_path: for search_path in search_paths {
        let mut package_path = search_path.path().to_path_buf();

        let mut components = name.components();
        let module_name = components.next_back()?;

        // For `foo.bar.baz`, test that `foo` and `baz` both contain a `__init__.py`.
        for folder in components {
            package_path.push(folder);

            // A folder on the path to the module. Test that it contains a `__init__.py`.
            if !package_path.join("__init__.py").is_file()
                && !package_path.join("__init__.pyi").is_file()
            {
                // Try the next search path
                continue 'search_path;
            }
        }

        package_path.push(module_name);

        // Must be a `__init__.pyi` or `__init__.py` or it isn't a package.
        if package_path.is_dir() {
            package_path.push("__init__");
        }

        let stub = package_path.with_extension("pyi");

        if stub.is_file() {
            return Some((search_path.clone(), stub));
        }

        let module = package_path.with_extension("py");

        if module.is_file() {
            return Some((search_path.clone(), module));
        }
    }

    None
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

#[cfg(test)]
mod tests {
    use crate::files::Files;
    use crate::module::{ModuleName, ModuleResolver, ModuleSearchPath, ModuleSearchPathKind};

    struct TestCase {
        temp_dir: tempfile::TempDir,
        resolver: ModuleResolver,
        files: Files,

        src: ModuleSearchPath,
        site_packages: ModuleSearchPath,
    }

    fn create_resolver() -> std::io::Result<TestCase> {
        let temp_dir = tempfile::tempdir()?;

        let src = temp_dir.path().join("src");
        let site_packages = temp_dir.path().join("site_packages");

        std::fs::create_dir(&src)?;
        std::fs::create_dir(&site_packages)?;

        let src = ModuleSearchPath::new(src, ModuleSearchPathKind::FirstParty);
        let site_packages = ModuleSearchPath::new(site_packages, ModuleSearchPathKind::ThirdParty);

        let roots = vec![src.clone(), site_packages.clone()];
        let files = Files::default();

        let resolver = ModuleResolver::new(roots, files.clone());

        Ok(TestCase {
            temp_dir,
            files,
            resolver,
            src,
            site_packages,
        })
    }

    #[test]
    fn first_party_module() -> std::io::Result<()> {
        let TestCase {
            mut resolver,
            src,
            files,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_path = src.path().join("foo.py");
        std::fs::write(&foo_path, "print('Hello, world!')")?;

        let foo_id = resolver.resolve(ModuleName::new("foo"));

        assert!(foo_id.is_some());
        assert_eq!(foo_id, resolver.resolve(ModuleName::new("foo")));

        let foo_module = resolver.module(foo_id.unwrap());

        assert_eq!(&ModuleName::new("foo"), foo_module.name());
        assert_eq!(&src, foo_module.path().root());
        assert_eq!(&foo_path, &*files.path(foo_module.path().file()));

        assert_eq!(foo_id, resolver.resolve_path(&foo_path));

        Ok(())
    }

    #[test]
    fn resolve_package() -> std::io::Result<()> {
        let TestCase {
            src,
            mut resolver,
            temp_dir: _temp_dir,
            files,
            ..
        } = create_resolver()?;

        let foo_dir = src.path().join("foo");
        let foo_path = foo_dir.join("__init__.py");
        std::fs::create_dir(&foo_dir)?;
        std::fs::write(&foo_path, "print('Hello, world!')")?;

        let foo_id = resolver.resolve(ModuleName::new("foo"));

        assert!(foo_id.is_some());

        let foo_module = resolver.module(foo_id.unwrap());

        assert_eq!(&ModuleName::new("foo"), foo_module.name());
        assert_eq!(&src, foo_module.path().root());
        assert_eq!(&foo_path, &*files.path(foo_module.path().file()));

        assert_eq!(foo_id, resolver.resolve_path(&foo_path));

        // TODO: Should resolving by the directory name resolve a module or not?
        assert_eq!(foo_id, resolver.resolve_path(&foo_dir));

        Ok(())
    }

    #[test]
    fn package_priority_over_module() -> std::io::Result<()> {
        let TestCase {
            mut resolver,
            temp_dir: _temp_dir,
            files,
            src,
            ..
        } = create_resolver()?;

        let foo_dir = src.path().join("foo");
        let foo_init = foo_dir.join("__init__.py");
        std::fs::create_dir(&foo_dir)?;
        std::fs::write(&foo_init, "print('Hello, world!')")?;

        let foo_py = src.path().join("foo.py");
        std::fs::write(&foo_py, "print('Hello, world!')")?;

        let foo_id = resolver.resolve(ModuleName::new("foo"));

        assert!(foo_id.is_some());

        let foo_module = resolver.module(foo_id.unwrap());

        assert_eq!(&src, foo_module.path().root());
        assert_eq!(&foo_init, &*files.path(foo_module.path().file()));

        assert_eq!(foo_id, resolver.resolve_path(&foo_init));
        assert_eq!(None, resolver.resolve_path(&foo_py));

        Ok(())
    }

    #[test]
    fn typing_stub_over_module() -> std::io::Result<()> {
        let TestCase {
            mut resolver,
            src,
            files,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_stub = src.path().join("foo.pyi");
        let foo_py = src.path().join("foo.py");
        std::fs::write(&foo_stub, "x: int")?;
        std::fs::write(&foo_py, "print('Hello, world!')")?;

        let foo_id = resolver.resolve(ModuleName::new("foo"));

        assert!(foo_id.is_some());

        let foo_module = resolver.module(foo_id.unwrap());

        assert_eq!(&src, foo_module.path().root());
        assert_eq!(&foo_stub, &*files.path(foo_module.path().file()));

        assert_eq!(foo_id, resolver.resolve_path(&foo_stub));
        assert_eq!(None, resolver.resolve_path(&foo_py));

        Ok(())
    }

    #[test]
    fn sub_packages() -> std::io::Result<()> {
        let TestCase {
            mut resolver,
            files,
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

        let baz_id = resolver.resolve(ModuleName::new("foo.bar.baz"));

        assert!(baz_id.is_some());

        let baz_module = resolver.module(baz_id.unwrap());

        assert_eq!(&src, baz_module.path().root());
        assert_eq!(&baz, &*files.path(baz_module.path().file()));

        assert_eq!(baz_id, resolver.resolve_path(&baz));

        Ok(())
    }

    #[test]
    fn folder_without_init_py() -> std::io::Result<()> {
        let mut test_case = create_resolver()?;

        let foo = test_case.src.path().join("foo");
        // The folder `bar` has no `__init__.py`
        let bar = foo.join("bar");
        let baz = bar.join("baz.py");

        std::fs::create_dir_all(&bar)?;
        std::fs::write(foo.join("__init__.py"), "")?;
        std::fs::write(&baz, "print('Hello, world!')")?;

        let baz_id = test_case.resolver.resolve(ModuleName::new("foo.bar.baz"));

        assert_eq!(None, baz_id);
        assert_eq!(None, test_case.resolver.resolve_path(&baz));

        Ok(())
    }

    #[test]
    fn module_search_path_priority() -> std::io::Result<()> {
        let TestCase {
            mut resolver,
            src,
            site_packages,
            files,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_src = src.path().join("foo.py");
        let foo_site_packages = site_packages.path().join("foo.py");

        std::fs::write(&foo_src, "")?;
        std::fs::write(&foo_site_packages, "")?;

        let foo_id = resolver.resolve(ModuleName::new("foo"));

        assert!(foo_id.is_some());

        let foo_module = resolver.module(foo_id.unwrap());

        assert_eq!(&src, foo_module.path().root());
        assert_eq!(&foo_src, &*files.path(foo_module.path().file()));

        assert_eq!(foo_id, resolver.resolve_path(&foo_src));
        assert_eq!(None, resolver.resolve_path(&foo_site_packages));

        Ok(())
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn symlink() -> std::io::Result<()> {
        let TestCase {
            mut resolver,
            files,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo = src.path().join("foo.py");
        let bar = src.path().join("bar.py");

        std::fs::write(&foo, "")?;
        std::os::unix::fs::symlink(&foo, src.path().join("bar.py"))?;

        let foo_id = resolver.resolve(ModuleName::new("foo"));
        let bar_id = resolver.resolve(ModuleName::new("bar"));

        assert!(foo_id.is_some());
        assert!(bar_id.is_some());

        // TODO: Is this the behavior we want? I think it is, but it means that the module appears as two
        //  different modules upstream, meaning we parse the file twice.
        //  I think we could solve this by using `FileId` instead of `PathBuf` in the `ModulePath`.
        //  This way, higher up layers that only care about the file's content (source text, parser, physical lines) can
        //  only index by file id and ignore the module id.
        assert_ne!(foo_id, bar_id);

        let foo_module = resolver.module(foo_id.unwrap());

        assert_eq!(&src, foo_module.path().root());
        assert_eq!(&foo, &*files.path(foo_module.path().file()));

        let bar_module = resolver.module(bar_id.unwrap());

        assert_eq!(&src, bar_module.path().root());
        assert_eq!(&foo, &*files.path(bar_module.path().file()));

        assert_eq!(foo_id, resolver.resolve_path(&foo));
        assert_eq!(bar_id, resolver.resolve_path(&bar));

        Ok(())
    }
}
