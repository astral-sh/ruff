use std::fmt::Formatter;
use std::ops::Deref;
use std::sync::Arc;

use ruff_python_ast::ModModule;
use ruff_python_parser::{parse_unchecked_source, Parsed};

use crate::files::File;
use crate::source::source_text;
use crate::Db;

/// Returns the parsed AST of `file`, including its token stream.
///
/// The query uses Ruff's error-resilient parser. That means that the parser always succeeds to produce an
/// AST even if the file contains syntax errors. The parse errors
/// are then accessible through [`Parsed::errors`].
///
/// The query is only cached when the [`source_text()`] hasn't changed. This is because
/// comparing two ASTs is a non-trivial operation and every offset change is directly
/// reflected in the changed AST offsets.
/// The other reason is that Ruff's AST doesn't implement `Eq` which Sala requires
/// for determining if a query result is unchanged.
#[salsa::tracked(return_ref, no_eq)]
pub fn parsed_module(db: &dyn Db, file: File) -> ParsedModule {
    let _span = tracing::trace_span!("parsed_module", ?file).entered();

    let source = source_text(db, file);
    let ty = file.source_type(db);

    ParsedModule::new(parse_unchecked_source(&source, ty))
}

/// Cheap cloneable wrapper around the parsed module.
#[derive(Clone)]
pub struct ParsedModule {
    inner: Arc<Parsed<ModModule>>,
}

impl ParsedModule {
    pub fn new(parsed: Parsed<ModModule>) -> Self {
        Self {
            inner: Arc::new(parsed),
        }
    }

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

impl PartialEq for ParsedModule {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for ParsedModule {}

#[cfg(test)]
mod tests {
    use crate::files::{system_path_to_file, vendored_path_to_file};
    use crate::parsed::parsed_module;
    use crate::system::{
        DbWithTestSystem, DbWithWritableSystem as _, SystemPath, SystemVirtualPath,
    };
    use crate::tests::TestDb;
    use crate::vendored::{VendoredFileSystemBuilder, VendoredPath};
    use crate::Db;
    use zip::CompressionMethod;

    #[test]
    fn python_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = "test.py";

        db.write_file(path, "x = 10")?;

        let file = system_path_to_file(&db, path).unwrap();

        let parsed = parsed_module(&db, file);

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn python_ipynb_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.ipynb");

        db.write_file(path, "%timeit a = b")?;

        let file = system_path_to_file(&db, path).unwrap();

        let parsed = parsed_module(&db, file);

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn virtual_python_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemVirtualPath::new("untitled:Untitled-1");

        db.write_virtual_file(path, "x = 10");

        let virtual_file = db.files().virtual_file(&db, path);

        let parsed = parsed_module(&db, virtual_file.file());

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn virtual_ipynb_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemVirtualPath::new("untitled:Untitled-1.ipynb");

        db.write_virtual_file(path, "%timeit a = b");

        let virtual_file = db.files().virtual_file(&db, path);

        let parsed = parsed_module(&db, virtual_file.file());

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn vendored_file() {
        let mut db = TestDb::new();

        let mut vendored_builder = VendoredFileSystemBuilder::new(CompressionMethod::Stored);
        vendored_builder
            .add_file(
                "path.pyi",
                r#"
import sys

if sys.platform == "win32":
    from ntpath import *
    from ntpath import __all__ as __all__
else:
    from posixpath import *
    from posixpath import __all__ as __all__"#,
            )
            .unwrap();
        let vendored = vendored_builder.finish().unwrap();
        db.with_vendored(vendored);

        let file = vendored_path_to_file(&db, VendoredPath::new("path.pyi")).unwrap();

        let parsed = parsed_module(&db, file);

        assert!(parsed.has_valid_syntax());
    }
}
