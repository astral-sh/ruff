use std::collections::hash_map::Entry;
use std::collections::BTreeMap;
use std::fmt::Formatter;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustc_hash::FxHashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ModuleId(u32);

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form (never a relative module name).
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ModuleName(smol_str::SmolStr);

impl ModuleName {
    pub fn resolve(name: &str) -> Self {
        Self(smol_str::SmolStr::new(name))
    }

    pub fn relative(_dots: u32, name: &str, _to: &Path) -> Self {
        // FIXME: Take `to` and `dots` into account.
        Self(smol_str::SmolStr::new(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RootPath {
    inner: Arc<RootPathInner>,
}

impl RootPath {
    pub fn new(path: PathBuf, kind: RootPathKind) -> Self {
        Self {
            inner: Arc::new(RootPathInner { path, kind }),
        }
    }

    pub fn kind(&self) -> RootPathKind {
        self.inner.kind
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }
}

impl std::fmt::Debug for RootPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct RootPathInner {
    path: PathBuf,
    kind: RootPathKind,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum RootPathKind {
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
}

#[derive(Debug, Default)]
pub struct ModuleResolver {
    /// The root paths where modules are located (and searched). Corresponds to `sys.path` at runtime.
    roots: Vec<RootPath>,

    /// All known modules, indexed by the module id.
    modules: FxHashMap<ModuleId, Module>,

    /// Resolves a module name to it's module id.
    by_name: FxHashMap<ModuleName, ModuleId>,

    /// Lookup from absolute path to module.
    /// The same module might be reachable from different paths when symlinks are involved.
    by_path: BTreeMap<PathBuf, ModuleId>,
    next_module_id: u32,
}

impl ModuleResolver {
    pub fn new() -> Self {
        Self {
            roots: Vec::new(),
            modules: FxHashMap::default(),
            by_name: FxHashMap::default(),
            by_path: BTreeMap::default(),
            next_module_id: 0,
        }
    }

    /// Resolves a module name to a module id.
    pub fn resolve(&mut self, name: ModuleName) -> Option<ModuleId> {
        let entry = self.by_name.entry(name.clone());

        match entry {
            Entry::Occupied(existing) => Some(*existing.get()),
            Entry::Vacant(vacant) => {
                if let Some(resolved) = resolve_name(&name, &self.roots) {
                    let id = ModuleId(self.next_module_id);
                    self.next_module_id += 1;
                    let full_path = resolved.full().to_path_buf();
                    self.modules.insert(
                        id,
                        Module {
                            name,
                            path: resolved,
                        },
                    );
                    self.by_path.insert(full_path, id);

                    vacant.insert(id);
                    Some(id)
                } else {
                    // Micha: We could consider to cache not-found modules to avoid testing through
                    // all root paths again. But I don't think it's worth it, considering that unresolved
                    // paths should be rare.
                    None
                }
            }
        }
    }

    pub fn id(&self, name: &ModuleName) -> Option<ModuleId> {
        self.by_name.get(name).copied()
    }

    /// Returns the module for a givne id.
    pub fn name(&self, id: ModuleId) -> &Module {
        self.modules.get(&id).unwrap()
    }

    pub fn resolve_path(&mut self, path: &Path) -> Option<ModuleId> {
        if let Some(existing) = self.by_path.get(path) {
            return Some(*existing);
        }

        // Okay this is ugly.
        // A path can map to multiple module paths because of symlinks.
        // A path can map to multiple module paths because of

        // Issue one, it's possible to have multiple paths that map to the same name but resolve to entirely different modules.
        //
        let root_path = self
            .roots
            .iter()
            .find(|root| path.starts_with(root.path()))?
            .clone();

        let mut name = String::new();

        for component in path.strip_prefix(root_path.path()).unwrap().components() {
            name.push_str(component.as_os_str().to_str()?);
        }

        let module_name = ModuleName(smol_str::SmolStr::from(name));

        // Resolve the module name to see if Python would resolve the name to the same path.
        // If it doesn't, then that means that multiple modules have the same in different
        // root paths, but that the module corresponding to the past path is in a lower priority path,
        // in which case we ignore it.
        let resolved = self.resolve(module_name)?;

        if self.modules[&resolved].path.full() == root_path.path() {
            // Path has been inserted by `resolved`
            // TODO verify if that's also true if we have symlinks
            Some(resolved)
        } else {
            None
        }
    }
}

fn resolve_name(name: &ModuleName, roots: &[RootPath]) -> Option<ModulePath> {
    let name = name.as_str();
    for root in roots {
        // Must be a `__init__.pyi` or `__init__.py` or it isn't a package.
        if root.path().join(name).is_dir() {
            let stub = root.path().join(name).join("__init__.pyi");

            if stub.is_file() {
                return Some(ModulePath::new(root.clone(), stub));
            }

            // Reuse the allocation
            let module = stub.with_extension("py");

            if module.is_file() {
                return Some(ModulePath::new(root.clone(), module));
            }

            return None;
        }

        let stub = root.path().join(name).with_extension(".pyi");

        if stub.is_file() {
            return Some(ModulePath::new(root.clone(), stub));
        }

        // Reuse the allocation
        let module = stub.with_extension("py");

        if module.is_file() {
            return Some(ModulePath::new(root.clone(), module));
        }
    }

    None
}

impl ModuleName {
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('.')
    }
}

/// The resolved path of a module.
///
/// It should be highly likely that the file still exists when accessing but it isn't 100% guaranteed
/// because the file could have been deleted between resolving the module name and accessing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModulePath {
    root: RootPath,
    full_path: PathBuf,
}

impl ModulePath {
    pub fn new(root: RootPath, full_path: PathBuf) -> Self {
        debug_assert!(full_path.starts_with(root.path()));
        Self { root, full_path }
    }

    pub fn root(&self) -> &RootPath {
        &self.root
    }

    pub fn relative(&self) -> &Path {
        &self
            .full_path
            .strip_prefix(self.root.path())
            .expect("Expected root path to be a prefix of the full path.")
    }

    pub fn full(&self) -> &Path {
        &self.full_path
    }
}

#[cfg(test)]
mod tests {}
