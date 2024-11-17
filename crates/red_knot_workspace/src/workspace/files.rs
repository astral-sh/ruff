use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

use rustc_hash::FxHashSet;
use salsa::Setter;

use ruff_db::files::File;

use crate::db::Db;
use crate::workspace::Package;

/// Cheap cloneable hash set of files.
type FileSet = Arc<FxHashSet<File>>;

/// The indexed files of a package.
///
/// The indexing happens lazily, but the files are then cached for subsequent reads.
///
/// ## Implementation
/// The implementation uses internal mutability to transition between the lazy and indexed state
/// without triggering a new salsa revision. This is safe because the initial indexing happens on first access,
/// so no query can be depending on the contents of the indexed files before that. All subsequent mutations to
/// the indexed files must go through `IndexedMut`, which uses the Salsa setter `package.set_file_set` to
/// ensure that Salsa always knows when the set of indexed files have changed.
#[derive(Debug)]
pub struct PackageFiles {
    state: std::sync::Mutex<State>,
}

impl PackageFiles {
    pub fn lazy() -> Self {
        Self {
            state: std::sync::Mutex::new(State::Lazy),
        }
    }

    fn indexed(files: FileSet) -> Self {
        Self {
            state: std::sync::Mutex::new(State::Indexed(files)),
        }
    }

    pub(super) fn get(&self) -> Index {
        let state = self.state.lock().unwrap();

        match &*state {
            State::Lazy => Index::Lazy(LazyFiles { files: state }),
            State::Indexed(files) => Index::Indexed(Indexed {
                files: Arc::clone(files),
                _lifetime: PhantomData,
            }),
        }
    }

    pub(super) fn is_lazy(&self) -> bool {
        matches!(*self.state.lock().unwrap(), State::Lazy)
    }

    /// Returns a mutable view on the index that allows cheap in-place mutations.
    ///
    /// The changes are automatically written back to the database once the view is dropped.
    pub(super) fn indexed_mut(db: &mut dyn Db, package: Package) -> Option<IndexedMut> {
        // Calling `zalsa_mut` cancels all pending salsa queries. This ensures that there are no pending
        // reads to the file set.
        // TODO: Use a non-internal API instead https://salsa.zulipchat.com/#narrow/stream/333573-salsa-3.2E0/topic/Expose.20an.20API.20to.20cancel.20other.20queries
        let _ = db.as_dyn_database_mut().zalsa_mut();

        // Replace the state with lazy. The `IndexedMut` guard restores the state
        // to `State::Indexed`  or sets a new `PackageFiles` when it gets dropped to ensure the state
        // is restored to how it has been before replacing the value.
        //
        // It isn't necessary to hold on to the lock after this point:
        // * The above call to `zalsa_mut` guarantees that there's exactly **one** DB reference.
        // * `Indexed` has a `'db` lifetime, and this method requires a `&mut db`.
        //   This means that there can't be any pending reference to `Indexed` because Rust
        //   doesn't allow borrowing `db` as mutable (to call this method) and immutable (`Indexed<'db>`) at the same time.
        //   There can't be any other `Indexed<'db>` references created by clones of this DB because
        //   all clones must have been dropped at this point and the `Indexed`
        //   can't outlive the database (constrained by the `db` lifetime).
        let state = {
            let files = package.file_set(db);
            let mut locked = files.state.lock().unwrap();
            std::mem::replace(&mut *locked, State::Lazy)
        };

        let indexed = match state {
            // If it's already lazy, just return. We also don't need to restore anything because the
            // replace above was a no-op.
            State::Lazy => return None,
            State::Indexed(indexed) => indexed,
        };

        Some(IndexedMut {
            db: Some(db),
            package,
            files: indexed,
            did_change: false,
        })
    }
}

impl Default for PackageFiles {
    fn default() -> Self {
        Self::lazy()
    }
}

#[derive(Debug)]
enum State {
    /// The files of a package haven't been indexed yet.
    Lazy,

    /// The files are indexed. Stores the known files of a package.
    Indexed(FileSet),
}

pub(super) enum Index<'db> {
    /// The index has not yet been computed. Allows inserting the files.
    Lazy(LazyFiles<'db>),

    Indexed(Indexed<'db>),
}

/// Package files that have not been indexed yet.
pub(super) struct LazyFiles<'db> {
    files: std::sync::MutexGuard<'db, State>,
}

impl<'db> LazyFiles<'db> {
    /// Sets the indexed files of a package to `files`.
    pub(super) fn set(mut self, files: FxHashSet<File>) -> Indexed<'db> {
        let files = Indexed {
            files: Arc::new(files),
            _lifetime: PhantomData,
        };
        *self.files = State::Indexed(Arc::clone(&files.files));
        files
    }
}

/// The indexed files of a package.
///
/// Note: This type is intentionally non-cloneable. Making it cloneable requires
/// revisiting the locking behavior in [`PackageFiles::indexed_mut`].
#[derive(Debug, PartialEq, Eq)]
pub struct Indexed<'db> {
    files: FileSet,
    // Preserve the lifetime of `PackageFiles`.
    _lifetime: PhantomData<&'db ()>,
}

impl Deref for Indexed<'_> {
    type Target = FxHashSet<File>;

    fn deref(&self) -> &Self::Target {
        &self.files
    }
}

pub(super) type IndexedIter<'a> = std::iter::Copied<std::collections::hash_set::Iter<'a, File>>;

impl<'a> IntoIterator for &'a Indexed<'_> {
    type Item = File;
    type IntoIter = IndexedIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.files.iter().copied()
    }
}

/// A Mutable view of a package's indexed files.
///
/// Allows in-place mutation of the files without deep cloning the hash set.
/// The changes are written back when the mutable view is dropped or by calling [`Self::set`] manually.
pub(super) struct IndexedMut<'db> {
    db: Option<&'db mut dyn Db>,
    package: Package,
    files: FileSet,
    did_change: bool,
}

impl IndexedMut<'_> {
    pub(super) fn insert(&mut self, file: File) -> bool {
        if self.files_mut().insert(file) {
            self.did_change = true;
            true
        } else {
            false
        }
    }

    pub(super) fn remove(&mut self, file: File) -> bool {
        if self.files_mut().remove(&file) {
            self.did_change = true;
            true
        } else {
            false
        }
    }

    fn files_mut(&mut self) -> &mut FxHashSet<File> {
        Arc::get_mut(&mut self.files).expect("All references to `FilesSet` to have been dropped")
    }

    fn set_impl(&mut self) {
        let Some(db) = self.db.take() else {
            return;
        };

        let files = Arc::clone(&self.files);

        if self.did_change {
            // If there are changes, set the new file_set to trigger a salsa revision change.
            self.package
                .set_file_set(db)
                .to(PackageFiles::indexed(files));
        } else {
            // The `indexed_mut` replaced the `state` with Lazy. Restore it back to the indexed state.
            *self.package.file_set(db).state.lock().unwrap() = State::Indexed(files);
        }
    }
}

impl Drop for IndexedMut<'_> {
    fn drop(&mut self) {
        self.set_impl();
    }
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashSet;

    use crate::db::tests::TestDb;
    use crate::db::Db;
    use crate::workspace::files::Index;
    use crate::workspace::WorkspaceMetadata;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_python_ast::name::Name;

    #[test]
    fn re_entrance() -> anyhow::Result<()> {
        let metadata = WorkspaceMetadata::single_package(
            Name::new_static("test"),
            SystemPathBuf::from("/test"),
        );
        let mut db = TestDb::new(metadata);

        db.write_file("test.py", "")?;

        let package = db
            .workspace()
            .package(&db, "/test")
            .expect("test package to exist");

        let file = system_path_to_file(&db, "test.py").unwrap();

        let files = match package.file_set(&db).get() {
            Index::Lazy(lazy) => lazy.set(FxHashSet::from_iter([file])),
            Index::Indexed(files) => files,
        };

        // Calling files a second time should not dead-lock.
        // This can e.g. happen when `check_file` iterates over all files and
        // `is_file_open` queries the open files.
        let files_2 = package.file_set(&db).get();

        match files_2 {
            Index::Lazy(_) => {
                panic!("Expected indexed files, got lazy files");
            }
            Index::Indexed(files_2) => {
                assert_eq!(
                    files_2.iter().collect::<Vec<_>>(),
                    files.iter().collect::<Vec<_>>()
                );
            }
        }

        Ok(())
    }
}
