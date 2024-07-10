use countme::Count;
use ruff_source_file::LineIndex;
use salsa::DebugWithDb;
use std::ops::Deref;
use std::sync::Arc;

use crate::files::File;
use crate::Db;

/// Reads the content of file.
#[salsa::tracked]
pub fn source_text(db: &dyn Db, file: File) -> SourceText {
    let _span = tracing::trace_span!("source_text", ?file).entered();

    let content = file.read_to_string(db);

    SourceText {
        inner: Arc::from(content),
        count: Count::new(),
    }
}

/// Computes the [`LineIndex`] for `file`.
#[salsa::tracked]
pub fn line_index(db: &dyn Db, file: File) -> LineIndex {
    let _span = tracing::trace_span!("line_index", file = ?file.debug(db)).entered();

    let source = source_text(db, file);

    LineIndex::from_source_text(&source)
}

/// The source text of a [`File`].
///
/// Cheap cloneable in `O(1)`.
#[derive(Clone, Eq, PartialEq)]
pub struct SourceText {
    inner: Arc<str>,
    count: Count<Self>,
}

impl SourceText {
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl Deref for SourceText {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Debug for SourceText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SourceText").field(&self.inner).finish()
    }
}

#[cfg(test)]
mod tests {
    use salsa::EventKind;

    use crate::files::system_path_to_file;
    use crate::source::{line_index, source_text};
    use crate::system::{DbWithTestSystem, SystemPath};
    use crate::tests::TestDb;
    use ruff_source_file::OneIndexed;
    use ruff_text_size::TextSize;

    #[test]
    fn re_runs_query_when_file_revision_changes() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10".to_string())?;

        let file = system_path_to_file(&db, path).unwrap();

        assert_eq!(&*source_text(&db, file), "x = 10");

        db.write_file(path, "x = 20".to_string()).unwrap();

        assert_eq!(&*source_text(&db, file), "x = 20");

        Ok(())
    }

    #[test]
    fn text_is_cached_if_revision_is_unchanged() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10".to_string())?;

        let file = system_path_to_file(&db, path).unwrap();

        assert_eq!(&*source_text(&db, file), "x = 10");

        // Change the file permission only
        file.set_permissions(&mut db).to(Some(0o777));

        db.clear_salsa_events();
        assert_eq!(&*source_text(&db, file), "x = 10");

        let events = db.take_salsa_events();

        assert!(!events
            .iter()
            .any(|event| matches!(event.kind, EventKind::WillExecute { .. })));

        Ok(())
    }

    #[test]
    fn line_index_for_source() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10\ny = 20".to_string())?;

        let file = system_path_to_file(&db, path).unwrap();
        let index = line_index(&db, file);
        let text = source_text(&db, file);

        assert_eq!(index.line_count(), 2);
        assert_eq!(
            index.line_start(OneIndexed::from_zero_indexed(0), &text),
            TextSize::new(0)
        );

        Ok(())
    }
}
