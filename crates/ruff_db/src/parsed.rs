use std::fmt::Formatter;
use std::ops::Deref;
use std::sync::Arc;

use ruff_python_ast::{ModModule, PySourceType};
use ruff_python_parser::{parse_unchecked_source, Parsed};

use crate::source::source_text;
use crate::vfs::{VfsFile, VfsPath};
use crate::Db;

/// Returns the parsed AST of `file`, including its token stream.
///
/// The query uses Ruff's error resilient parser. That means that the parser always succeeds to produce a
/// AST even if the file contains syntax errors. The syntax errors are Parsing the module succeeds even when the file contains syntax error. The parse errors
/// are then accessible through [`Parsed::errors`].
///
/// The parse tree is cached between invocations, but the query doesn't make use of Salsa's optimization
/// that skips dependent queries if the AST hasn't changed. Comparing two ASTs is a non-trivial operation
/// and every offset change is directly reflected in the changed AST offsets. Ruff's AST also doesn't implement `Eq`.
/// which is required to use the optimization.
#[salsa::tracked(return_ref, no_eq)]
pub fn parsed_module(db: &dyn Db, file: VfsFile) -> Parsed<ModModule> {
    let source = source_text(db, file);
    let path = file.path(db);

    let ty = match path {
        VfsPath::FileSystem(path) => path
            .extension()
            .map_or(PySourceType::Python, PySourceType::from_extension),
        VfsPath::Vendored(_) => PySourceType::Stub,
    };

    parse_unchecked_source(&source, ty)
}

/// Cheap cloneable wrapper around the parsed module.
#[derive(Clone, PartialEq)]
pub struct ParsedModule {
    inner: Arc<Parsed<ModModule>>,
}

impl ParsedModule {
    /// Consumes `self` and returns the Arc storing the parsed module.
    pub fn into_arc(self) -> Arc<Parsed<ModModule>> {
        self.inner
    }
}

impl Deref for ParsedModule {
    type Target = Parsed<ModModule>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::fmt::Debug for ParsedModule {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ParsedModule").field(&self.inner).finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::file_system::FileSystemPath;
    use crate::parsed::parsed_module;
    use crate::tests::TestDb;
    use crate::Db;

    #[test]
    fn python_file() {
        let mut db = TestDb::new();
        let path = FileSystemPath::new("test.py");

        db.file_system_mut().write_file(path, "x = 10".to_string());

        let file = db.file(path);

        let parsed = parsed_module(&db, file);

        assert!(parsed.is_valid());
    }

    #[test]
    fn python_ipynb_file() {
        let mut db = TestDb::new();
        let path = FileSystemPath::new("test.ipynb");

        db.file_system_mut()
            .write_file(path, "%timeit a = b".to_string());

        let file = db.file(path);

        let parsed = parsed_module(&db, file);

        assert!(parsed.is_valid());
    }
}
