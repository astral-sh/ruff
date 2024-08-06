use std::iter::FusedIterator;
use std::ops::Deref;
use std::sync::Arc;

use rustc_hash::FxHashSet;
use salsa::Setter;

use ruff_db::files::File;

use crate::db::Db;
use crate::workspace::Package;

/// The indexed files of a package.
///
/// The indexing happens lazily, but the files are then cached for subsequent reads.
///
/// ## Implementation
/// The implementation uses internal mutability to transition between the lazy and indexed state
/// without triggering a new salsa revision. This is safe because the initial indexing happens on first access,
/// so no query can be depending on the contents of the indexed files before that. All subsequent mutations to
/// the indexed files must go through `IndexedFilesMut`, which uses the Salsa setter `package.set_file_set` to
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

    fn indexed(indexed_files: IndexedFiles) -> Self {
        Self {
            state: std::sync::Mutex::new(State::Indexed(indexed_files)),
        }
    }

    pub fn get(&self) -> Index {
        let state = self.state.lock().unwrap();

        match &*state {
            State::Lazy => Index::Lazy(LazyFiles { files: state }),
            State::Indexed(files) => Index::Indexed(files.clone()),
        }
    }

    pub fn is_lazy(&self) -> bool {
        matches!(*self.state.lock().unwrap(), State::Lazy)
    }

    /// Returns a mutable view on the index that allows cheap in-place mutations.
    ///
    /// The changes are automatically written back to the database once the view is dropped.
    pub fn indexed_mut(db: &mut dyn Db, package: Package) -> Option<IndexedFilesMut> {
        // Calling `runtime_mut` cancels all pending salsa queries. This ensures that there are no pending
        // reads to the file set.
        let _ = db.as_dyn_database_mut().zalsa_mut();

        let files = package.file_set(db);

        let indexed = match &*files.state.lock().unwrap() {
            State::Lazy => return None,
            State::Indexed(indexed) => indexed.clone(),
        };

        Some(IndexedFilesMut {
            db: Some(db),
            package,
            new_revision: indexed.revision,
            indexed,
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
    Indexed(IndexedFiles),
}

pub enum Index<'a> {
    /// The index has not yet been computed. Allows inserting the files.
    Lazy(LazyFiles<'a>),

    Indexed(IndexedFiles),
}

/// Package files that have not been indexed yet.
pub struct LazyFiles<'a> {
    files: std::sync::MutexGuard<'a, State>,
}

impl<'a> LazyFiles<'a> {
    /// Sets the indexed files of a package to `files`.
    pub fn set(mut self, files: FxHashSet<File>) -> IndexedFiles {
        let files = IndexedFiles::new(files);
        *self.files = State::Indexed(files.clone());
        files
    }
}

/// The indexed files of a package.
///
/// # Salsa integration
/// The type is cheap clonable and allows for in-place mutation of the files. The in-place mutation requires
/// extra care because the type is used as the result of Salsa queries and Salsa relies on a type's equality
/// to determine if the output has changed. This is accomplished by using a `revision` that gets incremented
/// whenever the files are changed. The revision ensures that salsa's comparison of the
/// previous [`IndexedFiles`] with the next [`IndexedFiles`] returns false even though they both
/// point to the same underlying hash set.
///
/// # Equality
/// Two [`IndexedFiles`] are only equal if they have the same revision and point to the **same** (identity) hash set.
#[derive(Debug, Clone)]
pub struct IndexedFiles {
    revision: u64,
    files: Arc<std::sync::Mutex<FxHashSet<File>>>,
}

impl IndexedFiles {
    fn new(files: FxHashSet<File>) -> Self {
        Self {
            files: Arc::new(std::sync::Mutex::new(files)),
            revision: 0,
        }
    }

    /// Locks the file index for reading.
    pub fn read(&self) -> IndexedFilesGuard {
        IndexedFilesGuard {
            guard: self.files.lock().unwrap(),
        }
    }
}

impl PartialEq for IndexedFiles {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision && Arc::ptr_eq(&self.files, &other.files)
    }
}

impl Eq for IndexedFiles {}

pub struct IndexedFilesGuard<'a> {
    guard: std::sync::MutexGuard<'a, FxHashSet<File>>,
}

impl Deref for IndexedFilesGuard<'_> {
    type Target = FxHashSet<File>;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a> IntoIterator for &'a IndexedFilesGuard<'a> {
    type Item = File;
    type IntoIter = IndexedFilesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        IndexedFilesIter {
            inner: self.guard.iter(),
        }
    }
}

/// Iterator over the indexed files.
///
/// # Locks
/// Holding on to the iterator locks the file index for reading.
pub struct IndexedFilesIter<'a> {
    inner: std::collections::hash_set::Iter<'a, File>,
}

impl<'a> Iterator for IndexedFilesIter<'a> {
    type Item = File;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().copied()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl FusedIterator for IndexedFilesIter<'_> {}

impl ExactSizeIterator for IndexedFilesIter<'_> {}

/// A Mutable view of a package's indexed files.
///
/// Allows in-place mutation of the files without deep cloning the hash set.
/// The changes are written back when the mutable view is dropped or by calling [`Self::set`] manually.
pub struct IndexedFilesMut<'db> {
    db: Option<&'db mut dyn Db>,
    package: Package,
    indexed: IndexedFiles,
    new_revision: u64,
}

impl IndexedFilesMut<'_> {
    pub fn insert(&mut self, file: File) -> bool {
        if self.indexed.files.lock().unwrap().insert(file) {
            self.new_revision += 1;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, file: File) -> bool {
        if self.indexed.files.lock().unwrap().remove(&file) {
            self.new_revision += 1;
            true
        } else {
            false
        }
    }

    /// Writes the changes back to the database.
    pub fn set(mut self) {
        self.set_impl();
    }

    fn set_impl(&mut self) {
        let Some(db) = self.db.take() else {
            return;
        };

        if self.indexed.revision != self.new_revision {
            self.package
                .set_file_set(db)
                .to(PackageFiles::indexed(IndexedFiles {
                    revision: self.new_revision,
                    files: self.indexed.files.clone(),
                }));
        }
    }
}

impl Drop for IndexedFilesMut<'_> {
    fn drop(&mut self) {
        self.set_impl();
    }
}
