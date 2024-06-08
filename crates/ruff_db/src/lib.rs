use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;
use salsa::DbWithJar;

pub use file::{File, FileStatus, Files};

use crate::vfs::{Vfs, VfsPath};

mod file;
pub mod vfs;

pub(crate) type FxDashMap<K, V> = dashmap::DashMap<K, V, BuildHasherDefault<FxHasher>>;

#[salsa::jar(db=Db)]
pub struct Jar(File);

pub trait Db: DbWithJar<Jar> {
    /// Returns the virtual filesystem used by the database to read files.
    fn vfs(&self) -> &dyn Vfs;

    /// Interns a file path and returns a salsa `File` ingredient.
    ///
    /// The operation is guaranteed to always succeed, even if the path doesn't exist, isn't accessible, or if the path points to a directory.
    /// In these cases, a file with status [`FileStatus::Deleted`] is returned.
    fn file(&self, path: VfsPath) -> File;
}

/// Trait for upcasting a reference to a base trait object.
pub trait Upcast<T: ?Sized> {
    fn upcast(&self) -> &T;
}

#[cfg(test)]
mod tests {
    use crate::vfs::{MemoryFs, Vfs, VfsPath};
    use crate::{Db, File, Files, Jar};

    /// Database that can be used for testing.
    ///
    /// Uses an in memory filesystem.
    #[salsa::db(Jar)]
    pub struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        vfs: MemoryFs,
    }

    impl TestDb {
        #[allow(unused)]
        pub fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                files: Files::default(),
                vfs: MemoryFs::default(),
            }
        }

        /// Gives mutable access to the in memory filesystem.
        #[allow(unused)]
        pub fn vfs_mut(&mut self) -> &mut MemoryFs {
            &mut self.vfs
        }
    }

    impl Db for TestDb {
        fn vfs(&self) -> &dyn Vfs {
            &self.vfs
        }

        fn file(&self, path: VfsPath) -> File {
            self.files.lookup(self, path)
        }
    }

    impl salsa::Database for TestDb {}

    impl salsa::ParallelDatabase for TestDb {
        fn snapshot(&self) -> salsa::Snapshot<Self> {
            salsa::Snapshot::new(Self {
                storage: self.storage.snapshot(),
                files: self.files.snapshot(),
                vfs: self.vfs.snapshot(),
            })
        }
    }
}
