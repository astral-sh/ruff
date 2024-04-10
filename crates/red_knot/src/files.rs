use std::fmt::{Debug, Formatter};
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use hashbrown::hash_map::RawEntryMut;
use parking_lot::RwLock;
use rustc_hash::FxHasher;

use ruff_index::{newtype_index, IndexVec};

type Map<K, V> = hashbrown::HashMap<K, V, BuildHasherDefault<rustc_hash::FxHasher>>;

#[newtype_index]
pub struct FileId;

// TODO we'll need a higher level virtual file system abstraction that allows testing if a file exists
// or retrieving its content (ideally lazily and in a way that the memory can be retained later)
// I suspect that we'll end up with a FileSystem trait and our own Path abstraction.
#[derive(Clone, Default)]
pub struct Files {
    inner: Arc<RwLock<FilesInner>>,
}

impl Files {
    pub fn intern(&self, path: &Path) -> FileId {
        self.inner.write().intern(path)
    }

    pub fn path(&self, id: FileId) -> PathBuf {
        // TODO this should return a unowned path
        self.inner.read().path(id).to_path_buf()
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

#[derive(Default)]
struct FilesInner {
    by_path: Map<FileId, ()>,
    // TODO should we use a map here to reclaim the space for removed files?
    // TODO I think we should use our own path abstraction here to avoid having to normalize paths
    // and dealing with non-utf paths everywhere.
    by_id: IndexVec<FileId, PathBuf>,
}

impl FilesInner {
    /// Inserts the path and returns a new id for it or returns the id if it is an existing path.
    // TODO should this accept Path or PathBuf?
    pub(crate) fn intern(&mut self, path: &Path) -> FileId {
        let mut hasher = FxHasher::default();
        path.hash(&mut hasher);
        let hash = hasher.finish();

        let entry = self
            .by_path
            .raw_entry_mut()
            .from_hash(hash, |existing_file| &self.by_id[*existing_file] == path);

        match entry {
            RawEntryMut::Occupied(entry) => *entry.key(),
            RawEntryMut::Vacant(entry) => {
                let id = self.by_id.push(path.to_owned());

                entry.insert_with_hasher(hash, id, (), |file| {
                    let mut hasher = FxHasher::default();
                    self.by_id[*file].hash(&mut hasher);
                    hasher.finish()
                });

                id
            }
        }
    }

    /// Returns the path for the file with the given id.
    pub(crate) fn path(&self, id: FileId) -> &Path {
        self.by_id[id].as_path()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (FileId, &Path)> {
        self.by_path
            .keys()
            .map(|id| (*id, self.by_id[*id].as_path()))
    }
}
