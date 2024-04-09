use std::fmt::{Debug, Formatter};
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::path::{Path, PathBuf};

use hashbrown::hash_map::RawEntryMut;
use rustc_hash::FxHasher;

use ruff_index::{newtype_index, IndexVec};

type Map<K, V> = hashbrown::HashMap<K, V, BuildHasherDefault<rustc_hash::FxHasher>>;

#[newtype_index]
pub struct FileId;

// TODO we'll need a higher level virtual file system abstraction that allows testing if a file exists
// or retrieving its content (ideally lazily and in a way that the memory can be retained later)
// I suspect that we'll end up with a FileSystem trait and our own Path abstraction.
#[derive(Default, Clone)]
pub struct Files {
    by_path: Map<FileId, ()>,
    // TODO should we use a map here to reclaim the space for removed files?
    // TODO I think we should use our own path abstraction here to avoid having to normalize paths
    // and dealing with non-utf paths everywhere.
    by_id: IndexVec<FileId, PathBuf>,
}

impl Files {
    /// Inserts the path and returns a new id for it or returns the id if it is an existing path.
    // TODO should this accept Path or PathBuf?
    pub fn intern(&mut self, path: &Path) -> FileId {
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
    pub fn path(&self, id: FileId) -> &Path {
        self.by_id[id].as_path()
    }

    pub fn iter(&self) -> impl Iterator<Item = (FileId, &Path)> {
        self.by_path
            .iter()
            .map(move |(id, _)| (*id, self.by_id[*id].as_path()))
    }
}

impl Debug for Files {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_map();
        for item in self.iter() {
            debug.entry(&item.0, &item.1);
        }

        debug.finish()
    }
}
