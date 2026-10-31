use std::fmt::Formatter;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use get_size2::GetSize;
use ruff_python_ast::{
    ModExpression, ModModule, NodeIndex, NodeIndexError, PythonVersion, StringLiteral,
};
use ruff_python_parser::{
    ParseError, ParseErrorType, ParseOptions, Parsed, parse_cells_unchecked,
    parse_string_annotation, parse_unchecked,
};
use ruff_text_size::Ranged;

use crate::files::File;
use crate::source::source_text;
use crate::{Db, PythonFile};

/// Returns the parsed AST of `file`, including its token stream.
///
/// The query uses Ruff's error-resilient parser. That means that the parser always succeeds to produce an
/// AST even if the file contains syntax errors. The parse errors
/// are then accessible through [`Parsed::errors`].
///
/// The query is only cached when the [`source_text()`] hasn't changed. This is because
/// comparing two ASTs is a non-trivial operation and every offset change is directly
/// reflected in the changed AST offsets.
/// The other reason is that Ruff's AST doesn't implement `Eq` which Salsa requires
/// for determining if a query result is unchanged.
///
/// The LRU capacity of 200 was picked without any empirical evidence that it's optimal,
/// instead it's a wild guess that it should be unlikely that incremental changes involve
/// more than 200 modules. Parsed ASTs within the same revision are never evicted by Salsa.
#[salsa::tracked(returns(ref), no_eq, heap_size=ruff_memory_usage::heap_size, lru=200)]
pub fn parsed_module(db: &dyn Db, file: PythonFile<'_>) -> ParsedModule {
    let source_file = file.file(db);
    let python_version = file.python_version(db);
    let _span = tracing::trace_span!("parsed_module", ?source_file, %python_version).entered();

    let parsed = parsed_module_impl(db, source_file, python_version);

    ParsedModule::new(source_file, python_version, parsed)
}

pub(super) fn disable_lru(db: &mut dyn Db) {
    parsed_module::set_lru_capacity(db, 0);
}

fn parsed_module_impl(db: &dyn Db, file: File, target_version: PythonVersion) -> Parsed<ModModule> {
    let source = source_text(db, file);
    let ty = file.source_type(db);

    let options = ParseOptions::from(ty).with_target_version(target_version);

    // Notebooks parse each cell as an independent module so a syntax error confined to one cell is
    // surfaced instead of being masked by a later cell's content. Regular files take the existing
    // single-parse path.
    if let Some(notebook) = source.as_notebook() {
        parse_cells_unchecked(&source, notebook.cell_offsets().content_ranges(), &options)
    } else {
        parse_unchecked(&source, options)
            .try_into_module()
            .expect("PySourceType always parses into a module")
    }
}

pub fn parsed_string_annotation(
    source: &str,
    string: ruff_python_ast::flat::View<'_, StringLiteral>,
) -> Result<Parsed<ModExpression>, ParseError> {
    let mut expr = parse_string_annotation(source, string)?;

    expr.index_string_annotation(string.node_index()).map_err(|err| {
        let message = match err {
            NodeIndexError::NoParent => {
                "Internal error: string annotation's parent had no NodeIndex"
            }
            NodeIndexError::TooNested => {
                "Too many levels of nested string annotations; \
                remove the redundant nested quotes"
            }
            NodeIndexError::OverflowedIndices => {
                "File too long for string annotations; either break up the file \
                or don't use string annotations"
            }
            NodeIndexError::OverflowedSubIndices => {
                "File too long for nested string annotations; remove the redundant nested quotes"
            }
            NodeIndexError::ExhaustedSubIndices => {
                "String annotation is too long; consider introducing type aliases to simplify"
            }
            NodeIndexError::ExhaustedSubSubIndices => {
                "Nested string annotation is too long; remove the redundant nested quotes"
            }
        };

        ParseError {
            error: ParseErrorType::StringAnnotationError(message),
            location: string.range(),
        }
    })?;

    Ok(expr)
}

/// A wrapper around a parsed module.
///
/// This type manages instances of the module AST. A particular instance of the AST
/// is represented with the [`ParsedModuleRef`] type.
#[derive(Clone, get_size2::GetSize)]
pub struct ParsedModule {
    file: File,
    python_version: PythonVersion,
    #[get_size(size_fn = arc_swap_size)]
    inner: Arc<ArcSwapOption<Parsed<ModModule>>>,
}

impl ParsedModule {
    fn new(file: File, python_version: PythonVersion, parsed: Parsed<ModModule>) -> Self {
        Self {
            file,
            python_version,
            inner: Arc::new(ArcSwapOption::new(Some(cache_module(parsed)))),
        }
    }
    /// Loads a reference to the parsed module.
    ///
    /// Note that holding on to the reference will prevent garbage collection
    /// of the AST. This method will reparse the module if it has been collected.
    pub fn load(&self, db: &dyn Db) -> ParsedModuleRef {
        let parsed = match self.inner.load_full() {
            Some(parsed) => parsed,
            None => {
                // Re-parse the file.
                let parsed = cache_module(parsed_module_impl(db, self.file, self.python_version));
                tracing::debug!(
                    "File `{}` was reparsed after being collected in the current Salsa revision",
                    self.file.path(db)
                );

                self.inner.store(Some(parsed.clone()));
                parsed
            }
        };

        ParsedModuleRef {
            module: self.clone(),
            parsed,
        }
    }

    /// Loads the parsed module and simultaneously notes whether the underlying
    /// file is closed. If so, we proactively clear its AST from the Salsa cache
    /// when the returned guard is dropped.
    ///
    /// This is intended for use in language server operations that scan all of
    /// the files in a workspace, thereby often exceeding the LRU cache for parsed
    /// modules. Clearing ASTs proactively helps mitigate unpredictable pauses
    /// that would otherwise occur on the next database update, but it can also
    /// reduce the efficiency of our cache, so we should replace this with a more
    /// robust solution eventually: <https://github.com/astral-sh/ty/issues/3909>.
    pub fn load_then_clear_on_drop(&self, db: &dyn Db) -> ParsedModuleGuard {
        ParsedModuleGuard {
            module: self.load(db),
            clear_on_drop: !db.system().is_file_open(self.file.path(db)),
        }
    }

    /// Clear the parsed module, dropping the AST once all references to it are dropped.
    pub fn clear(&self) {
        self.inner.store(None);
    }

    /// Returns the file to which this module belongs.
    pub fn file(&self) -> File {
        self.file
    }

    /// Returns the Python version used to parse this module.
    pub fn python_version(&self) -> PythonVersion {
        self.python_version
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

/// Cheap cloneable wrapper around an instance of a module AST.
#[derive(Clone)]
pub struct ParsedModuleRef {
    module: ParsedModule,
    parsed: Arc<Parsed<ModModule>>,
}

impl ParsedModuleRef {
    /// Returns a reference to the [`ParsedModule`] that this instance was loaded from.
    pub fn module(&self) -> &ParsedModule {
        &self.module
    }

    /// Looks up a module row by its key.
    pub fn get_by_index(&self, index: NodeIndex) -> ruff_python_ast::flat::AnyNodeRef<'_> {
        index
            .as_u32()
            .and_then(|row| self.ast().row(row))
            .expect("row belongs to the module")
    }
}

impl std::ops::Deref for ParsedModuleRef {
    type Target = Parsed<ModModule>;

    fn deref(&self) -> &Self::Target {
        &self.parsed
    }
}

/// A parsed module reference that clears the cached AST on drop if the file was closed when loaded.
pub struct ParsedModuleGuard {
    module: ParsedModuleRef,
    clear_on_drop: bool,
}

impl std::ops::Deref for ParsedModuleGuard {
    type Target = ParsedModuleRef;

    fn deref(&self) -> &Self::Target {
        &self.module
    }
}

impl Drop for ParsedModuleGuard {
    fn drop(&mut self) {
        if self.clear_on_drop {
            self.module.module().clear();
        }
    }
}

/// Returns the heap-size of the currently stored `T` in the `ArcSwap`.
fn arc_swap_size<T>(arc_swap: &Arc<ArcSwapOption<T>>) -> usize
where
    T: GetSize,
{
    if let Some(value) = &*arc_swap.load() {
        T::get_heap_size(value)
    } else {
        0
    }
}

fn cache_module(mut parsed: Parsed<ModModule>) -> Arc<Parsed<ModModule>> {
    parsed.index_module();
    parsed.shrink_to_fit();
    Arc::new(parsed)
}

#[cfg(test)]
mod tests {
    use crate::Db;
    use crate::PythonFile;
    use crate::files::{system_path_to_file, vendored_path_to_file};
    use crate::parsed::parsed_module;
    use crate::system::{
        DbWithTestSystem, DbWithWritableSystem as _, SystemPath, SystemVirtualPath,
    };
    use crate::tests::TestDb;
    use crate::vendored::{VendoredFileSystemBuilder, VendoredPath};
    use ruff_python_ast::{NodeIndex, NodeIndexError, PythonVersion, sub_indices};
    use ruff_python_parser::parse_expression;
    use zip::CompressionMethod;

    #[test]
    fn nested_annotation_at_key_limit() {
        let (parent, _) = sub_indices(0).unwrap();
        let (first, last) = sub_indices(parent).unwrap();
        let mut annotation = parse_expression(r#"f"{x:0}{y:0}""#).unwrap();
        annotation
            .index_string_annotation(NodeIndex::from(parent))
            .unwrap();
        let mut keys: Vec<_> = annotation
            .root()
            .descendants()
            .skip(1)
            .map(|node| node.node_index().as_u32().unwrap())
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, (first..=last).collect::<Vec<_>>());

        let mut over_limit = parse_expression(r#"not f"{x:0}{y:0}""#).unwrap();
        assert!(matches!(
            over_limit.index_string_annotation(NodeIndex::from(parent)),
            Err(NodeIndexError::ExhaustedSubSubIndices)
        ));
    }

    #[test]
    fn python_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = "test.py";

        db.write_file(path, "x = 10")?;

        let file = system_path_to_file(&db, path).unwrap();

        let file = PythonFile::new(&db, file, PythonVersion::latest_ty());
        let parsed = parsed_module(&db, file).load(&db);

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn python_ipynb_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.ipynb");

        db.write_file(path, "%timeit a = b")?;

        let file = system_path_to_file(&db, path).unwrap();

        let file = PythonFile::new(&db, file, PythonVersion::latest_ty());
        let parsed = parsed_module(&db, file).load(&db);

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn virtual_python_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemVirtualPath::new("untitled:Untitled-1");

        db.write_virtual_file(path, "x = 10");

        let virtual_file = db.files().virtual_file(&db, path);

        let file = PythonFile::new(&db, virtual_file.file(), PythonVersion::latest_ty());
        let parsed = parsed_module(&db, file).load(&db);

        assert!(parsed.has_valid_syntax());

        Ok(())
    }

    #[test]
    fn virtual_ipynb_file() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemVirtualPath::new("untitled:Untitled-1.ipynb");

        db.write_virtual_file(path, "%timeit a = b");

        let virtual_file = db.files().virtual_file(&db, path);

        let file = PythonFile::new(&db, virtual_file.file(), PythonVersion::latest_ty());
        let parsed = parsed_module(&db, file).load(&db);

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

        let file = PythonFile::new(&db, file, PythonVersion::latest_ty());
        let parsed = parsed_module(&db, file).load(&db);

        assert!(parsed.has_valid_syntax());
    }

    #[test]
    fn same_file_at_different_python_versions() -> crate::system::Result<()> {
        let mut db = TestDb::new();
        db.write_file("test.py", "type Alias = int")?;
        let file = system_path_to_file(&db, "test.py").unwrap();

        let py311 = PythonFile::new(&db, file, PythonVersion::PY311);
        let py312 = PythonFile::new(&db, file, PythonVersion::PY312);
        let parsed_py311 = parsed_module(&db, py311);
        let parsed_py312 = parsed_module(&db, py312);

        for _ in 0..2 {
            assert!(
                !parsed_py311
                    .load(&db)
                    .unsupported_syntax_errors()
                    .is_empty()
            );
            assert!(
                parsed_py312
                    .load(&db)
                    .unsupported_syntax_errors()
                    .is_empty()
            );

            parsed_py311.clear();
            parsed_py312.clear();
        }

        Ok(())
    }
}
