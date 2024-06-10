use std::ops::Deref;
use std::sync::Arc;

use crate::vfs::VfsFile;
use crate::Db;

/// Reads the content of file.
#[salsa::tracked]
pub fn source_text(db: &dyn Db, file: VfsFile) -> SourceText {
    let content = file.read(db);

    SourceText {
        inner: Arc::from(content),
    }
}

/// The source text of a [`VfsFile`](crate::File)
#[derive(Clone, Eq, PartialEq)]
pub struct SourceText {
    inner: Arc<str>,
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
    use crate::file_system::FileSystemPath;
    use crate::source::source_text;
    use crate::tests::TestDb;
    use crate::Db;
    use filetime::FileTime;
    use salsa::EventKind;

    #[test]
    fn re_runs_query_when_file_revision_changes() {
        let mut db = TestDb::new();
        let path = FileSystemPath::new("test.py");

        db.file_system_mut().write_file(path, "x = 10".to_string());

        let file = db.file(path);

        assert_eq!(&*source_text(&db, file), "x = 10");

        db.file_system_mut().write_file(path, "x = 20".to_string());
        file.set_revision(&mut db).to(FileTime::now().into());

        assert_eq!(&*source_text(&db, file), "x = 20");
    }

    #[test]
    fn text_is_cached_if_revision_is_unchanged() {
        let mut db = TestDb::new();
        let path = FileSystemPath::new("test.py");

        db.file_system_mut().write_file(path, "x = 10".to_string());

        let file = db.file(path);

        assert_eq!(&*source_text(&db, file), "x = 10");

        // Change the file permission only
        file.set_permissions(&mut db).to(Some(0o777));

        db.events().lock().unwrap().clear();
        assert_eq!(&*source_text(&db, file), "x = 10");

        let events = db.events();
        let events = events.lock().unwrap();

        assert!(!events
            .iter()
            .any(|event| matches!(event.kind, EventKind::WillExecute { .. })));
    }
}
