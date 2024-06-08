use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use ruff_python_ast::ModModule;
use ruff_python_parser::Parsed;

use crate::cache::KeyValueCache;
use crate::db::{QueryResult, SourceDb};
use crate::files::FileId;
use crate::source::source_text;

#[tracing::instrument(level = "debug", skip(db))]
pub(crate) fn parse(db: &dyn SourceDb, file_id: FileId) -> QueryResult<Arc<Parsed<ModModule>>> {
    let jar = db.jar()?;

    jar.parsed.get(&file_id, |file_id| {
        let source = source_text(db, *file_id)?;

        Ok(Arc::new(ruff_python_parser::parse_unchecked_source(
            source.text(),
            source.kind().into(),
        )))
    })
}

#[derive(Debug, Default)]
pub struct ParsedStorage(KeyValueCache<FileId, Arc<Parsed<ModModule>>>);

impl Deref for ParsedStorage {
    type Target = KeyValueCache<FileId, Arc<Parsed<ModModule>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ParsedStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
