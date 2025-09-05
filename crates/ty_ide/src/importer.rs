#![allow(warnings)]

/*!
An abstraction for adding new imports to a single Python source file.

This importer is based on a similar abstraction in `ruff_linter::importer`.
Both of them use the lower level `ruff_python_importer::{Edit, Insertion}`
primitives. The main differences here are:

1. This work's with ty's semantic model instead of ruff's.
2. This owns the task of visiting AST to extract imports. This
   design was chosen because it's currently only used for inserting
   imports for unimported completion suggestions. If it needs to be
   used more broadly, it might make sense to roll construction of an
   `Impoter` into ty's `SemanticIndex`.
3. It doesn't have as many facilities as `ruff_linter`'s importer.
*/

use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_codegen::Stylist;
use ruff_python_importer::{Edit, Insertion};
use ruff_python_parser::{Parsed, Tokens};
use ruff_source_file::{LineIndex, Locator};
use ruff_text_size::{TextRange, TextSize};
use ty_project::Db;
use ty_python_semantic::ModuleName;

pub(crate) struct Importer<'a> {
    /// The ty Salsa database.
    db: &'a dyn Db,
    /// The file corresponding to the module that
    /// we want to insert an import statement into.
    file: File,
    /// The Python AST to which we are adding imports.
    ast: &'a [ast::Stmt],
    /// The tokens representing the Python AST.
    tokens: &'a Tokens,
    /// The [`Locator`] for the Python AST.
    locator: Locator<'a>,
    /// The [`Stylist`] for the Python AST.
    stylist: &'a Stylist<'a>,
    /// The list of visited, top-level runtime imports in the Python AST.
    imports: Vec<AstImport<'a>>,
}

impl<'a> Importer<'a> {
    /// Create a new importer.
    ///
    /// The `Stylist` dictates the code formatting options of any code
    /// edit (if any) produced by this importer.
    ///
    /// The `file` given should correspond to the module that we want
    /// to insert an import statement into.
    ///
    /// The `Locator` is used to get access to the original source
    /// text for `file`, which is used to help produce code edits (if
    /// any). It's also used to re-parse code using `libcst` in order
    /// to produce code edits that retain "trivia" in the source code
    /// (i.e., whitespace and comments).
    ///
    /// The AST given (corresponding to the contents of `file`) is
    /// traversed and top-level imports are extracted from it. This
    /// permits adding imports in a way that is harmonious with
    /// existing imports.
    pub(crate) fn new(
        db: &'a dyn Db,
        stylist: &'a Stylist<'a>,
        file: File,
        source: &'a str,
        line_index: LineIndex,
        parsed: &'a Parsed<ast::ModModule>,
    ) -> Self {
        let locator = Locator::with_index(source, line_index);
        let imports = TopLevelImports::find(parsed);
        Self {
            db,
            file,
            ast: parsed.suite(),
            tokens: parsed.tokens(),
            locator,
            stylist,
            imports,
        }
    }

    /// Imports a symbol into this importer's module.
    ///
    /// The given request is assumed to be valid. That is, the module
    /// is assumed to be importable and the member is assumed to be a
    /// valid thing to import from the given module.
    ///
    /// When possible (particularly when there is no existing import
    /// statement to satisfy the given request), the import style on
    /// the request is respected. When there is an existing import,
    /// then the existing style is always respected instead.
    ///
    /// The "import action" returned includes an edit for inserting
    /// the actual import (if necessary) along with the symbol text
    /// that should be used to refer to the imported symbol. While
    /// the symbol text may be expected to just be equivalent to the
    /// request's `member`, it can be different. For example, if there
    /// is an alias or if the corresponding module is already imported
    /// in a qualified way.
    pub(crate) fn import(&self, request: ImportRequest<'_>) -> ImportAction {
        let mut symbol_text: Box<str> = request.member.into();
        let Some(response) = self.find(&request) else {
            let import = Insertion::start_of_file(self.ast, &self.locator, self.stylist)
                .into_edit(&request.to_string());
            if matches!(request.style, ImportStyle::Import) {
                symbol_text = format!("{}.{}", request.module, request.member).into();
            }
            return ImportAction {
                import: Some(import),
                symbol_text,
            };
        };
        match response.kind {
            ImportResponseKind::Unqualified { ast, alias } => {
                let member = alias.asname.as_ref().unwrap_or(&alias.name).as_str();
                // As long as it's not a glob, we use whatever name
                // the member is imported as when inserting the symbol.
                if member != "*" {
                    symbol_text = member.into();
                }
                ImportAction {
                    import: None,
                    symbol_text,
                }
            }
            ImportResponseKind::Qualified { ast, alias } => {
                let module = alias.asname.as_ref().unwrap_or(&alias.name).as_str();
                ImportAction {
                    import: None,
                    symbol_text: format!("{module}.{symbol_text}").into(),
                }
            }
            ImportResponseKind::Partial(ast) => {
                let import =
                    Insertion::end_of_statement(response.import.stmt, &self.locator, self.stylist)
                        .into_edit(&format!(
                            "from {} import {}",
                            request.module, request.member
                        ));
                ImportAction {
                    import: Some(import),
                    symbol_text,
                }
            }
        }
    }

    /// Look for an import already in this importer's module that
    /// satisfies the given request. If found, the corresponding
    /// import is returned along with the way in which the import
    /// satisfies the request.
    fn find<'importer>(
        &'importer self,
        request: &ImportRequest<'_>,
    ) -> Option<ImportResponse<'importer, 'a>> {
        // BREADCRUMBS: We need to implement prioritization here.
        // That is, we prefer unqualified, then qualified and finally
        // partial.
        for import in &self.imports {
            if let Some(response) = import.satisfies(self.db, self.file, request) {
                return Some(response);
            }
        }
        None
    }
}

/// The edits needed to insert the import statement.
///
/// While this is usually just an edit to add an import statement (or
/// modify an existing one), it can also sometimes just be a change
/// to the text that should be inserted for a particular symbol. For
/// example, if one were to ask for `search` from the `re` module, and
/// `re` was already imported, then we'd return no edits for import
/// statements and the text `re.search` to use for the symbol.
#[derive(Debug)]
pub(crate) struct ImportAction {
    import: Option<Edit>,
    symbol_text: Box<str>,
}

impl ImportAction {
    /// Returns an edit to insert an import statement.
    pub(crate) fn import(&self) -> Option<&Edit> {
        self.import.as_ref()
    }

    /// Returns the symbol text that should be used.
    ///
    /// Usually this is identical to the symbol text given to the corresponding
    /// `ImportRequest`, but this may sometimes be fully qualified based on
    /// existing imports or import preferences.
    pub(crate) fn symbol_text(&self) -> &str {
        &*self.symbol_text
    }
}

/// A borrowed AST of a Python import statement.
#[derive(Debug)]
struct AstImport<'ast> {
    /// The original AST statement containing the import.
    stmt: &'ast ast::Stmt,
    /// The specific type of import.
    ///
    /// Storing this means we can do exhaustive case analysis
    /// on the type of the import without needing to constantly
    /// unwrap it from a more general `Stmt`. Still, we keep the
    /// `Stmt` around because some APIs want that.
    kind: AstImportKind<'ast>,
}

impl<'ast> AstImport<'ast> {
    /// Returns whether this import satisfies the given request.
    ///
    /// If it does, then this returns *how* the import satisfies
    /// the request.
    fn satisfies<'importer>(
        &'importer self,
        db: &'_ dyn Db,
        importing_file: File,
        request: &ImportRequest<'_>,
    ) -> Option<ImportResponse<'importer, 'ast>> {
        match self.kind {
            AstImportKind::Import(ast) => {
                let alias = ast
                    .names
                    .iter()
                    .find(|alias| alias.name.as_str() == request.module)?;
                let kind = ImportResponseKind::Qualified { ast, alias };
                Some(ImportResponse { import: self, kind })
            }
            AstImportKind::ImportFrom(ast) => {
                let module = ModuleName::from_import_statement(db, importing_file, ast).ok()?;
                if module.as_str() != request.module {
                    return None;
                }
                let kind = ast
                    .names
                    .iter()
                    .find(|alias| {
                        alias.name.as_str() == "*" || alias.name.as_str() == request.member
                    })
                    .map(|alias| ImportResponseKind::Unqualified { ast, alias })
                    .unwrap_or_else(|| ImportResponseKind::Partial(ast));
                Some(ImportResponse { import: self, kind })
            }
        }
    }
}

/// The specific kind of import.
#[derive(Debug)]
enum AstImportKind<'ast> {
    Import(&'ast ast::StmtImport),
    ImportFrom(&'ast ast::StmtImportFrom),
}

/// A request to import a module into the global scope of a Python module.
#[derive(Debug)]
pub(crate) struct ImportRequest<'a> {
    /// The module from which the symbol should be imported (e.g.,
    /// `foo`, in `from foo import bar`).
    module: &'a str,
    /// The member to import (e.g., `bar`, in `from foo import bar`).
    member: &'a str,
    /// The preferred style to use when importing the symbol (e.g.,
    /// `import foo` or `from foo import bar`).
    ///
    /// This style isn't respected if the `module` already has
    /// an import statement. In that case, the existing style is
    /// respected.
    style: ImportStyle,
}

impl<'a> ImportRequest<'a> {
    /// Create a new `ImportRequest` from a `module` and `member`.
    ///
    /// If `module` has no existing imports, the symbol should be
    /// imported using the `import` statement.
    pub(crate) fn import(module: &'a str, member: &'a str) -> Self {
        Self {
            module,
            member,
            style: ImportStyle::Import,
        }
    }

    /// Create a new `ImportRequest` from a module and member.
    ///
    /// If `module` has no existing imports, the symbol should be
    /// imported using the `import from` statement.
    pub(crate) fn import_from(module: &'a str, member: &'a str) -> Self {
        Self {
            module,
            member,
            style: ImportStyle::ImportFrom,
        }
    }
}

impl std::fmt::Display for ImportRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.style {
            ImportStyle::Import => write!(f, "import {}", self.module),
            ImportStyle::ImportFrom => write!(f, "from {} import {}", self.module, self.member),
        }
    }
}

/// The response to an import request.
#[derive(Debug)]
struct ImportResponse<'importer, 'ast> {
    import: &'importer AstImport<'ast>,
    kind: ImportResponseKind<'ast>,
}

/// The kind of response to an import request.
///
/// This encodes the answer to the question: how does a given import
/// statement satisfy an `ImportRequest`? This encodes the different
/// degrees to the request is satisfied.
#[derive(Debug)]
enum ImportResponseKind<'ast> {
    /// The import satisfies the request as-is. The symbol is already
    /// imported directly and may be used unqualified.
    ///
    /// This always corresponds to a `from <...> import <...>`
    /// statement. Note that `<...>` may be a glob import!
    Unqualified {
        /// The AST of the import that satisfied the request.
        ast: &'ast ast::StmtImportFrom,
        /// The specific alias in the `from <...> import <...>`
        /// statement that satisfied the request's `member`.
        alias: &'ast ast::Alias,
    },
    /// The necessary module is imported, but the symbol itself is not
    /// in scope. The symbol can be used via `module.symbol`.
    ///
    /// This always corresponds to a `import <...>` statement.
    Qualified {
        /// The AST of the import that satisfied the request.
        ast: &'ast ast::StmtImport,
        /// The specific alias in the import statement that
        /// satisfied the request's `module`.
        alias: &'ast ast::Alias,
    },
    /// The necessary module is imported via `from module import ...`,
    /// but the desired symbol is not listed in `...`.
    ///
    /// This always corresponds to a `from <...> import <...>`
    /// statement.
    Partial(&'ast ast::StmtImportFrom),
}

/// The style of a Python import statement.
#[derive(Debug)]
enum ImportStyle {
    /// Import the symbol using the `import` statement (e.g. `import
    /// foo; foo.bar`).
    Import,
    /// Import the symbol using the `from` statement (e.g. `from foo
    /// import bar; bar`).
    ImportFrom,
}

/// An error that can occur when trying to add an import.
#[derive(Debug)]
pub(crate) enum ResolutionError {
    /// The symbol is imported, but the import came after the current location.
    ImportAfterUsage,
    /// The symbol is imported, but in an incompatible context (e.g., in
    /// typing-only context, while we're in a runtime context).
    IncompatibleContext,
    /// The symbol can't be imported, because another symbol is bound to the
    /// same name.
    ConflictingName(String),
    /// The symbol can't be imported due to an error in editing an existing
    /// import statement.
    InvalidEdit,
}

impl std::fmt::Display for ResolutionError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionError::ImportAfterUsage => {
                fmt.write_str("Unable to use existing symbol due to late binding")
            }
            ResolutionError::IncompatibleContext => {
                fmt.write_str("Unable to use existing symbol due to incompatible context")
            }
            ResolutionError::ConflictingName(binding) => std::write!(
                fmt,
                "Unable to insert `{binding}` into scope due to name conflict"
            ),
            ResolutionError::InvalidEdit => {
                fmt.write_str("Unable to modify existing import statement")
            }
        }
    }
}

impl std::error::Error for ResolutionError {}

/// An AST visitor for extracting top-level imports.
#[derive(Debug, Default)]
struct TopLevelImports<'ast> {
    level: u64,
    imports: Vec<AstImport<'ast>>,
}

impl<'ast> TopLevelImports<'ast> {
    /// Find all top-level imports from the given AST of a Python module.
    fn find(parsed: &'ast Parsed<ast::ModModule>) -> Vec<AstImport<'ast>> {
        let mut visitor = TopLevelImports::default();
        visitor.visit_body(parsed.suite());
        visitor.imports
    }
}

impl<'ast> StatementVisitor<'ast> for TopLevelImports<'ast> {
    fn visit_stmt(&mut self, stmt: &'ast ast::Stmt) {
        match *stmt {
            ast::Stmt::Import(ref node) => {
                if self.level == 0 {
                    let kind = AstImportKind::Import(node);
                    self.imports.push(AstImport { stmt, kind });
                }
            }
            ast::Stmt::ImportFrom(ref node) => {
                if self.level == 0 {
                    let kind = AstImportKind::ImportFrom(node);
                    self.imports.push(AstImport { stmt, kind });
                }
            }
            _ => {
                // OK because it's not practical for the source code
                // depth of a Python to exceed a u64.
                //
                // Also, it is perhaps a bit too eager to increment
                // this for every non-import statement, particularly
                // compared to the more refined scope tracking in the
                // semantic index builder. However, I don't think
                // we need anything more refined here. We only care
                // about top-level imports. So as soon as we get into
                // something nested, we can bail out.
                //
                // Although, this does mean, e.g.,
                //
                //     if predicate:
                //         import whatever
                //
                // at the module scope Is not caught here. If we
                // need those imports, I think we'll just want some
                // more case analysis with more careful `level`
                // incrementing.
                self.level = self.level.checked_add(1).unwrap();
                walk_stmt(self, stmt);
                // Always OK because we can only be here after
                // a successful +1 from above.
                self.level = self.level.checked_sub(1).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8Component;
    use insta::assert_snapshot;
    use insta::internals::SettingsBindDropGuard;

    use crate::tests::{CursorTest, cursor_test};
    use ruff_db::diagnostic::{Diagnostic, DiagnosticFormat, DisplayDiagnosticConfig};
    use ruff_db::files::{File, FileRootKind, system_path_to_file};
    use ruff_db::parsed::parsed_module;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithWritableSystem, SystemPath, SystemPathBuf};
    use ruff_db::{Db, system};
    use ruff_python_codegen::Stylist;
    use ruff_python_trivia::textwrap::dedent;
    use ruff_source_file::Locator;
    use ruff_text_size::TextSize;
    use ty_project::ProjectMetadata;
    use ty_python_semantic::{
        Program, ProgramSettings, PythonPlatform, PythonVersionWithSource, SearchPathSettings,
    };

    use super::*;

    impl CursorTest {
        fn import(&self, module: &str, member: &str) -> String {
            self.add(ImportRequest::import(module, member))
        }

        fn import_from(&self, module: &str, member: &str) -> String {
            self.add(ImportRequest::import_from(module, member))
        }

        fn add(&self, request: ImportRequest<'_>) -> String {
            let resp = self.importer().import(request);

            // We attempt to emulate what an LSP client would
            // do here and "insert" the import into the original
            // source document. I'm not 100% sure this models
            // reality correctly, but in particular, we are
            // careful to insert the symbol name first since
            // it *should* come after the import.
            let mut source = self.cursor.source.to_string();
            source.insert_str(self.cursor.offset.to_usize(), &resp.symbol_text);
            if let Some(edit) = resp.import() {
                assert!(
                    edit.range().start() <= self.cursor.offset,
                    "import edit must come at or before <CURSOR>, \
                     but <CURSOR> starts at {} and the import \
                     edit is at {}..{}",
                    self.cursor.offset.to_usize(),
                    edit.range().start().to_usize(),
                    edit.range().end().to_usize(),
                );
                source.replace_range(edit.range().to_std_range(), edit.content().unwrap_or(""));
            }
            source
        }

        fn importer(&self) -> Importer<'_> {
            Importer::new(
                &self.db,
                &self.cursor.stylist,
                self.cursor.file,
                self.cursor.source.as_str(),
                self.cursor.line_index.clone(),
                &*self.cursor.parsed,
            )
        }
    }

    #[test]
    fn empty_source_qualified() {
        let test = cursor_test("<CURSOR>");
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        collections.defaultdict
        ");
    }

    #[test]
    fn empty_source_unqualified() {
        let test = cursor_test("<CURSOR>");
        assert_snapshot!(
            test.import_from("collections", "defaultdict"), @r"
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_exists_qualified() {
        let test = cursor_test(
            "\
import collections
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections
        collections.defaultdict
        ");
    }

    #[test]
    fn import_exists_unqualified() {
        let test = cursor_test(
            "\
from collections import defaultdict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict
        defaultdict
        ");
    }

    #[test]
    fn import_exists_glob() {
        let test = cursor_test(
            "\
from collections import *
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import *
        defaultdict
        ");
    }

    #[test]
    fn import_exists_qualified_aliased() {
        let test = cursor_test(
            "\
import collections as c
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        import collections as c
        c.defaultdict
        ");
    }

    #[test]
    fn import_exists_unqualified_aliased() {
        let test = cursor_test(
            "\
from collections import defaultdict as ddict
<CURSOR>
        ",
        );
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import defaultdict as ddict
        ddict
        ");
    }

    #[test]
    fn import_partially_exists() {
        let test = cursor_test(
            "\
from collections import Counter
<CURSOR>
        ",
        );
        // BREADCRUMBS: This should try to add to the
        // existing `from collections` import instead
        // of adding a new line. This might prove a
        // little tricky, since `Insertion::end_of_statement`
        // isn't quite what we want. I suppose there might
        // be some cases where we want to insert a new line?
        // But maybe not, and we should let a formatter handle
        // that. But the statement might have continuations
        // or semi-colons. Anyway, get this test working then
        // add a bunch of other tests handling this case.
        assert_snapshot!(
            test.import("collections", "defaultdict"), @r"
        from collections import Counter
        from collections import defaultdict
        defaultdict
        ");
    }
}
