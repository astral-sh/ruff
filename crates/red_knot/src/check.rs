use crate::files::{FileId, Files};
use crate::Workspace;
use parking_lot::lock_api::RwLockUpgradableReadGuard;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::collections::HashMap;

/// The basic idea of Salsa is that you give it all inputs (the source contents) and
/// you can then define queries on top of it.
///
/// That means we need to eagerly set the contents of all files that the analysis could reach, which means
/// we possibly need to load the entire program into memory, which is wasteful.
/// TODO: I don't want to eagerly load all the files. We should load them lazily.
/// TODO: Salsa also has no concept of a persistent cache
pub struct SourceDataBase<'a> {
    files: &'a Files, // TODO using a reference here is going to require internal mutability in files when it is time to update the files.

    // Stores the source of files loaded lazily from disk.
    source: RwLock<FxHashMap<FileId, String>>,

    parse_results: FxHashMap<FileId, ParseResult>,
}

impl SourceDataBase<'_> {
    fn source(&self, file_id: FileId) -> &str {
        let mut lock = self.source.upgradable_read();

        if let Some(source) = lock.get(&file_id) {
            return &source;
        }

        let mut upgraded = RwLockUpgradableReadGuard::upgrade(lock);
        let path = self.files.path(file_id);
        let source = std::fs::read_to_string(path).unwrap();

        upgraded.insert(file_id, source);

        &source
    }
}

struct ParseResult {
    program: ruff_python_ast::Mod,
    errors: Vec<ruff_python_parser::ParseError>,
}
