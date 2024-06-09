use std::hash::BuildHasherDefault;

use rustc_hash::FxHasher;
use salsa::DbWithJar;

use crate::vfs::VfsFile;

pub mod vfs;

pub(crate) type FxDashMap<K, V> = dashmap::DashMap<K, V, BuildHasherDefault<FxHasher>>;

#[salsa::jar(db=Db)]
pub struct Jar(VfsFile);

pub trait Db: DbWithJar<Jar> {
    /// Interns a file path and returns a salsa `File` ingredient.
    ///
    /// The operation is guaranteed to always succeed, even if the path doesn't exist, isn't accessible, or if the path points to a directory.
    /// In these cases, a file with status [`FileStatus::Deleted`] is returned.
    fn file(&self, path: &camino::Utf8Path) -> VfsFile;

    /// Interns a path to a vendored file and returns a salsa `File` ingredient.
    fn vendored_file(&self, path: &camino::Utf8Path) -> Option<VfsFile>;
}

/// Trait for upcasting a reference to a base trait object.
pub trait Upcast<T: ?Sized> {
    fn upcast(&self) -> &T;
}

#[cfg(test)]
mod tests {
    use crate::vfs::Vfs;
    use crate::{Db, Jar, VfsFile};

    /// Database that can be used for testing.
    ///
    /// Uses an in memory filesystem.
    #[salsa::db(Jar)]
    pub struct TestDb {
        storage: salsa::Storage<Self>,
        vfs: Vfs,
    }

    impl TestDb {
        #[allow(unused)]
        pub fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                vfs: Vfs::default(),
            }
        }
    }

    impl Db for TestDb {
        fn file(&self, path: &camino::Utf8Path) -> VfsFile {
            self.vfs.fs(self, path)
        }

        fn vendored_file(&self, path: &camino::Utf8Path) -> Option<VfsFile> {
            self.vfs.vendored(self, path)
        }
    }

    impl salsa::Database for TestDb {}

    impl salsa::ParallelDatabase for TestDb {
        fn snapshot(&self) -> salsa::Snapshot<Self> {
            salsa::Snapshot::new(Self {
                storage: self.storage.snapshot(),
                vfs: self.vfs.snapshot(),
            })
        }
    }
}
