use std::fmt::Formatter;
use std::hash::BuildHasherDefault;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::path::{Path, PathBuf};

use hashbrown::hash_map::{HashMap, RawEntryMut};
use rustc_hash::{FxHashSet, FxHasher};

use crate::files::FileId;
use ruff_index::{newtype_index, IndexVec};

pub mod ast_ids;
pub mod cache;
pub mod cancellation;
pub mod db;
pub mod files;
pub mod hir;
pub mod lint;
pub mod module;
mod parse;
pub mod program;
pub mod source;
mod symbols;
mod types;
pub mod watch;

pub(crate) type FxDashMap<K, V> = dashmap::DashMap<K, V, BuildHasherDefault<FxHasher>>;
#[allow(unused)]
pub(crate) type FxDashSet<V> = dashmap::DashSet<V, BuildHasherDefault<FxHasher>>;
pub(crate) type FxIndexSet<V> = indexmap::set::IndexSet<V, BuildHasherDefault<FxHasher>>;

#[derive(Debug, Clone)]
pub struct Workspace {
    /// TODO this should be a resolved path. We should probably use a newtype wrapper that guarantees that
    /// PATH is a UTF-8 path and is normalized.
    root: PathBuf,
    /// The files that are open in the workspace.
    ///
    /// * Editor: The files that are actively being edited in the editor (the user has a tab open with the file).
    /// * CLI: The resolved files passed as arguments to the CLI.
    open_files: FxHashSet<FileId>,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            open_files: FxHashSet::default(),
        }
    }

    pub fn root(&self) -> &Path {
        self.root.as_path()
    }

    // TODO having the content in workspace feels wrong.
    pub fn open_file(&mut self, file_id: FileId) {
        self.open_files.insert(file_id);
    }

    pub fn close_file(&mut self, file_id: FileId) {
        self.open_files.remove(&file_id);
    }

    // TODO introduce an `OpenFile` type instead of using an anonymous tuple.
    pub fn open_files(&self) -> impl Iterator<Item = FileId> + '_ {
        self.open_files.iter().copied()
    }

    pub fn is_file_open(&self, file_id: FileId) -> bool {
        self.open_files.contains(&file_id)
    }
}

// NOTE: Should we instead use existing ruff_python_semantic::model::NameId?
#[newtype_index]
pub struct NameId;

impl<'a> NameId {
    pub fn name(&self, names: &'a Names) -> &'a Name {
        names.name(*self)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Name(smol_str::SmolStr);

impl Name {
    #[inline]
    pub fn new(name: &str) -> Self {
        Self(smol_str::SmolStr::new(name))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Deref for Name {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<T> From<T> for Name
where
    T: Into<smol_str::SmolStr>,
{
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// NOTE: modeled after red_knot::files::FilesInner; not wrapped with Arc(s) for now, because it's
// not clear that's necessary
#[derive(Debug, Default)]
pub struct Names {
    by_name: HashMap<NameId, (), ()>,
    by_id: IndexVec<NameId, Name>,
}

impl Names {
    pub fn intern(&mut self, name: Name) -> NameId {
        let hash = Names::hash_name(&name);
        let entry = self
            .by_name
            .raw_entry_mut()
            .from_hash(hash, |id| self.by_id[*id] == name);
        match entry {
            RawEntryMut::Occupied(kv) => *kv.key(),
            RawEntryMut::Vacant(kv) => {
                let name_id = self.by_id.push(name);
                kv.insert_with_hasher(hash, name_id, (), |id| Names::hash_name(&self.by_id[*id]));
                name_id
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<NameId> {
        let hash = Names::hash_name(name);
        self.by_name
            .raw_entry()
            .from_hash(hash, |id| &*self.by_id[*id] == name)
            .map(|(id, ())| *id)
    }

    pub fn name(&self, id: NameId) -> &Name {
        &self.by_id[id]
    }

    fn hash_name(name: &str) -> u64 {
        let mut hasher = FxHasher::default();
        name.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_names() {
        let mut names = Names::default();
        assert_ne!(names.intern(Name::new("a")), names.intern(Name::new("b")));
        assert_eq!(names.intern(Name::new("a")), names.intern(Name::new("a")));
        assert_eq!(names.intern(Name::new("b")), names.intern(Name::new("b")));
        assert_eq!(names.by_name.len(), 2);
        assert_eq!(names.by_id.len(), 2);
    }
}
