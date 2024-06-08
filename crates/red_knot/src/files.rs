use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;

use hashbrown::hash_map::RawEntryMut;
use parking_lot::RwLock;
use rustc_hash::FxHasher;

use ruff_index::{newtype_index, IndexVec};

type Map<K, V> = hashbrown::HashMap<K, V, ()>;

#[newtype_index]
pub struct FileId;

// TODO we'll need a higher level virtual file system abstraction that allows testing if a file exists
//  or retrieving its content (ideally lazily and in a way that the memory can be retained later)
//  I suspect that we'll end up with a FileSystem trait and our own Path abstraction.
#[derive(Default)]
pub struct Files {
    inner: Arc<RwLock<FilesInner>>,
}

impl Files {
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn intern(&self, path: &Path) -> FileId {
        self.inner.write().intern(path)
    }

    pub fn try_get(&self, path: &Path) -> Option<FileId> {
        self.inner.read().try_get(path)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn path(&self, id: FileId) -> Arc<Path> {
        self.inner.read().path(id)
    }

    /// Snapshots files for a new database snapshot.
    ///
    /// This method should not be used outside a database snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Files {
        Files {
            inner: self.inner.clone(),
        }
    }
}

impl Debug for Files {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let files = self.inner.read();
        let mut debug = f.debug_map();
        for item in files.iter() {
            debug.entry(&item.0, &item.1);
        }

        debug.finish()
    }
}

impl PartialEq for Files {
    fn eq(&self, other: &Self) -> bool {
        self.inner.read().eq(&other.inner.read())
    }
}

impl Eq for Files {}

#[derive(Default)]
struct FilesInner {
    by_path: Map<FileId, ()>,
    // TODO should we use a map here to reclaim the space for removed files?
    // TODO I think we should use our own path abstraction here to avoid having to normalize paths
    //  and dealing with non-utf paths everywhere.
    by_id: IndexVec<FileId, Arc<Path>>,
}

impl FilesInner {
    /// Inserts the path and returns a new id for it or returns the id if it is an existing path.
    // TODO should this accept Path or PathBuf?
    pub(crate) fn intern(&mut self, path: &Path) -> FileId {
        let hash = FilesInner::hash_path(path);

        let entry = self
            .by_path
            .raw_entry_mut()
            .from_hash(hash, |existing_file| &*self.by_id[*existing_file] == path);

        match entry {
            RawEntryMut::Occupied(entry) => *entry.key(),
            RawEntryMut::Vacant(entry) => {
                let id = self.by_id.push(Arc::from(path));
                entry.insert_with_hasher(hash, id, (), |file| {
                    FilesInner::hash_path(&self.by_id[*file])
                });
                id
            }
        }
    }

    fn hash_path(path: &Path) -> u64 {
        let mut hasher = FxHasher::default();
        path.hash(&mut hasher);
        hasher.finish()
    }

    pub(crate) fn try_get(&self, path: &Path) -> Option<FileId> {
        let mut hasher = FxHasher::default();
        path.hash(&mut hasher);
        let hash = hasher.finish();

        Some(
            *self
                .by_path
                .raw_entry()
                .from_hash(hash, |existing_file| &*self.by_id[*existing_file] == path)?
                .0,
        )
    }

    /// Returns the path for the file with the given id.
    pub(crate) fn path(&self, id: FileId) -> Arc<Path> {
        self.by_id[id].clone()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (FileId, Arc<Path>)> + '_ {
        self.by_path.keys().map(|id| (*id, self.by_id[*id].clone()))
    }
}

impl PartialEq for FilesInner {
    fn eq(&self, other: &Self) -> bool {
        self.by_id == other.by_id
    }
}

impl Eq for FilesInner {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn insert_path_twice_same_id() {
        let files = Files::default();
        let path = PathBuf::from("foo/bar");
        let id1 = files.intern(&path);
        let id2 = files.intern(&path);
        assert_eq!(id1, id2);
    }

    #[test]
    fn insert_different_paths_different_ids() {
        let files = Files::default();
        let path1 = PathBuf::from("foo/bar");
        let path2 = PathBuf::from("foo/bar/baz");
        let id1 = files.intern(&path1);
        let id2 = files.intern(&path2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn four_files() {
        let files = Files::default();
        let foo_path = PathBuf::from("foo");
        let foo_id = files.intern(&foo_path);
        let bar_path = PathBuf::from("bar");
        files.intern(&bar_path);
        let baz_path = PathBuf::from("baz");
        files.intern(&baz_path);
        let qux_path = PathBuf::from("qux");
        files.intern(&qux_path);

        let foo_id_2 = files.try_get(&foo_path).expect("foo_path to be found");
        assert_eq!(foo_id_2, foo_id);
    }
}
