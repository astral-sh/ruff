//! Add and modify import statements to make module members available during fix execution.

use std::error::Error;

use anyhow::Result;
use libcst_native::{Codegen, CodegenState, ImportAlias, Name, NameOrAttribute};
use ruff_text_size::TextSize;
use rustpython_parser::ast::{self, Ranged, Stmt, Suite};

use ruff_diagnostics::Edit;
use ruff_python_ast::imports::{AnyImport, Import, ImportFrom};
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_semantic::model::SemanticModel;

use crate::cst::matchers::{match_aliases, match_import_from, match_statement};
use crate::importer::insertion::Insertion;

mod insertion;

pub(crate) struct Importer<'a> {
    /// The Python AST to which we are adding imports.
    python_ast: &'a Suite,
    /// The [`Locator`] for the Python AST.
    locator: &'a Locator<'a>,
    /// The [`Stylist`] for the Python AST.
    stylist: &'a Stylist<'a>,
    /// The list of visited, top-level runtime imports in the Python AST.
    runtime_imports: Vec<&'a Stmt>,
    /// The list of visited, top-level `if TYPE_CHECKING:` blocks in the Python AST.
    type_checking_blocks: Vec<&'a Stmt>,
}

impl<'a> Importer<'a> {
    pub(crate) fn new(
        python_ast: &'a Suite,
        locator: &'a Locator<'a>,
        stylist: &'a Stylist<'a>,
    ) -> Self {
        Self {
            python_ast,
            locator,
            stylist,
            runtime_imports: Vec::default(),
            type_checking_blocks: Vec::default(),
        }
    }

    /// Visit a top-level import statement.
    pub(crate) fn visit_import(&mut self, import: &'a Stmt) {
        self.runtime_imports.push(import);
    }

    /// Visit a top-level type-checking block.
    pub(crate) fn visit_type_checking_block(&mut self, type_checking_block: &'a Stmt) {
        self.type_checking_blocks.push(type_checking_block);
    }

    /// Add an import statement to import the given module.
    ///
    /// If there are no existing imports, the new import will be added at the top
    /// of the file. Otherwise, it will be added after the most recent top-level
    /// import statement.
    pub(crate) fn add_import(&self, import: &AnyImport, at: TextSize) -> Edit {
        let required_import = import.to_string();
        if let Some(stmt) = self.preceding_import(at) {
            // Insert after the last top-level import.
            Insertion::end_of_statement(stmt, self.locator, self.stylist)
                .into_edit(&required_import)
        } else {
            // Insert at the top of the file.
            Insertion::top_of_file(self.python_ast, self.locator, self.stylist)
                .into_edit(&required_import)
        }
    }

    /// Generate an [`Edit`] to reference the given symbol. Returns the [`Edit`] necessary to make
    /// the symbol available in the current scope along with the bound name of the symbol.
    ///
    /// Attempts to reuse existing imports when possible.
    pub(crate) fn get_or_import_symbol(
        &self,
        symbol: &ImportRequest,
        at: TextSize,
        semantic_model: &SemanticModel,
    ) -> Result<(Edit, String), ResolutionError> {
        match self.get_symbol(symbol, at, semantic_model) {
            Some(result) => result,
            None => self.import_symbol(symbol, at, semantic_model),
        }
    }

    /// Return an [`Edit`] to reference an existing symbol, if it's present in the given [`SemanticModel`].
    fn get_symbol(
        &self,
        symbol: &ImportRequest,
        at: TextSize,
        semantic_model: &SemanticModel,
    ) -> Option<Result<(Edit, String), ResolutionError>> {
        // If the symbol is already available in the current scope, use it.
        let imported_name =
            semantic_model.resolve_qualified_import_name(symbol.module, symbol.member)?;

        // If the symbol source (i.e., the import statement) comes after the current location,
        // abort. For example, we could be generating an edit within a function, and the import
        // could be defined in the module scope, but after the function definition. In this case,
        // it's unclear whether we can use the symbol (the function could be called between the
        // import and the current location, and thus the symbol would not be available). It's also
        // unclear whether should add an import statement at the top of the file, since it could
        // be shadowed between the import and the current location.
        if imported_name.range().start() > at {
            return Some(Err(ResolutionError::ImportAfterUsage));
        }

        // If the symbol source (i.e., the import statement) is in a typing-only context, but we're
        // in a runtime context, abort.
        if imported_name.context().is_typing() && semantic_model.execution_context().is_runtime() {
            return Some(Err(ResolutionError::IncompatibleContext));
        }

        // We also add a no-op edit to force conflicts with any other fixes that might try to
        // remove the import. Consider:
        //
        // ```py
        // import sys
        //
        // quit()
        // ```
        //
        // Assume you omit this no-op edit. If you run Ruff with `unused-imports` and
        // `sys-exit-alias` over this snippet, it will generate two fixes: (1) remove the unused
        // `sys` import; and (2) replace `quit()` with `sys.exit()`, under the assumption that `sys`
        // is already imported and available.
        //
        // By adding this no-op edit, we force the `unused-imports` fix to conflict with the
        // `sys-exit-alias` fix, and thus will avoid applying both fixes in the same pass.
        let import_edit = Edit::range_replacement(
            self.locator.slice(imported_name.range()).to_string(),
            imported_name.range(),
        );
        Some(Ok((import_edit, imported_name.into_name())))
    }

    /// Generate an [`Edit`] to reference the given symbol. Returns the [`Edit`] necessary to make
    /// the symbol available in the current scope along with the bound name of the symbol.
    ///
    /// For example, assuming `module` is `"functools"` and `member` is `"lru_cache"`, this function
    /// could return an [`Edit`] to add `import functools` to the top of the file, alongside with
    /// the name on which the `lru_cache` symbol would be made available (`"functools.lru_cache"`).
    fn import_symbol(
        &self,
        symbol: &ImportRequest,
        at: TextSize,
        semantic_model: &SemanticModel,
    ) -> Result<(Edit, String), ResolutionError> {
        if let Some(stmt) = self.find_import_from(symbol.module, at) {
            // Case 1: `from functools import lru_cache` is in scope, and we're trying to reference
            // `functools.cache`; thus, we add `cache` to the import, and return `"cache"` as the
            // bound name.
            if semantic_model.is_unbound(symbol.member) {
                let Ok(import_edit) = self.add_member(stmt, symbol.member) else {
                    return Err(ResolutionError::InvalidEdit);
                };
                Ok((import_edit, symbol.member.to_string()))
            } else {
                Err(ResolutionError::ConflictingName(symbol.member.to_string()))
            }
        } else {
            match symbol.style {
                ImportStyle::Import => {
                    // Case 2a: No `functools` import is in scope; thus, we add `import functools`,
                    // and return `"functools.cache"` as the bound name.
                    if semantic_model.is_unbound(symbol.module) {
                        let import_edit =
                            self.add_import(&AnyImport::Import(Import::module(symbol.module)), at);
                        Ok((
                            import_edit,
                            format!(
                                "{module}.{member}",
                                module = symbol.module,
                                member = symbol.member
                            ),
                        ))
                    } else {
                        Err(ResolutionError::ConflictingName(symbol.module.to_string()))
                    }
                }
                ImportStyle::ImportFrom => {
                    // Case 2b: No `functools` import is in scope; thus, we add
                    // `from functools import cache`, and return `"cache"` as the bound name.
                    if semantic_model.is_unbound(symbol.member) {
                        let import_edit = self.add_import(
                            &AnyImport::ImportFrom(ImportFrom::member(
                                symbol.module,
                                symbol.member,
                            )),
                            at,
                        );
                        Ok((import_edit, symbol.member.to_string()))
                    } else {
                        Err(ResolutionError::ConflictingName(symbol.member.to_string()))
                    }
                }
            }
        }
    }

    /// Return the import statement that precedes the given position, if any.
    fn preceding_import(&self, at: TextSize) -> Option<&Stmt> {
        self.runtime_imports
            .partition_point(|stmt| stmt.start() < at)
            .checked_sub(1)
            .map(|idx| self.runtime_imports[idx])
    }

    /// Return the top-level [`Stmt`] that imports the given module using `Stmt::ImportFrom`
    /// preceding the given position, if any.
    fn find_import_from(&self, module: &str, at: TextSize) -> Option<&Stmt> {
        let mut import_from = None;
        for stmt in &self.runtime_imports {
            if stmt.start() >= at {
                break;
            }
            if let Stmt::ImportFrom(ast::StmtImportFrom {
                module: name,
                level,
                ..
            }) = stmt
            {
                if level.map_or(true, |level| level.to_u32() == 0)
                    && name.as_ref().map_or(false, |name| name == module)
                {
                    import_from = Some(*stmt);
                }
            }
        }
        import_from
    }

    /// Add the given member to an existing `Stmt::ImportFrom` statement.
    fn add_member(&self, stmt: &Stmt, member: &str) -> Result<Edit> {
        let mut statement = match_statement(self.locator.slice(stmt.range()))?;
        let import_from = match_import_from(&mut statement)?;
        let aliases = match_aliases(import_from)?;
        aliases.push(ImportAlias {
            name: NameOrAttribute::N(Box::new(Name {
                value: member,
                lpar: vec![],
                rpar: vec![],
            })),
            asname: None,
            comma: aliases.last().and_then(|alias| alias.comma.clone()),
        });
        let mut state = CodegenState {
            default_newline: &self.stylist.line_ending(),
            default_indent: self.stylist.indentation(),
            ..CodegenState::default()
        };
        statement.codegen(&mut state);
        Ok(Edit::range_replacement(state.to_string(), stmt.range()))
    }
}

#[derive(Debug)]
enum ImportStyle {
    /// Import the symbol using the `import` statement (e.g. `import foo; foo.bar`).
    Import,
    /// Import the symbol using the `from` statement (e.g. `from foo import bar; bar`).
    ImportFrom,
}

#[derive(Debug)]
pub(crate) struct ImportRequest<'a> {
    /// The module from which the symbol can be imported (e.g., `foo`, in `from foo import bar`).
    module: &'a str,
    /// The member to import (e.g., `bar`, in `from foo import bar`).
    member: &'a str,
    /// The preferred style to use when importing the symbol (e.g., `import foo` or
    /// `from foo import bar`), if it's not already in scope.
    style: ImportStyle,
}

impl<'a> ImportRequest<'a> {
    /// Create a new `ImportRequest` from a module and member. If not present in the scope,
    /// the symbol should be imported using the "import" statement.
    pub(crate) fn import(module: &'a str, member: &'a str) -> Self {
        Self {
            module,
            member,
            style: ImportStyle::Import,
        }
    }

    /// Create a new `ImportRequest` from a module and member. If not present in the scope,
    /// the symbol should be imported using the "import from" statement.
    pub(crate) fn import_from(module: &'a str, member: &'a str) -> Self {
        Self {
            module,
            member,
            style: ImportStyle::ImportFrom,
        }
    }
}

/// The result of an [`Importer::get_or_import_symbol`] call.
#[derive(Debug)]
pub(crate) enum ResolutionError {
    /// The symbol is imported, but the import came after the current location.
    ImportAfterUsage,
    /// The symbol is imported, but in an incompatible context (e.g., in typing-only context, while
    /// we're in a runtime context).
    IncompatibleContext,
    /// The symbol can't be imported, because another symbol is bound to the same name.
    ConflictingName(String),
    /// The symbol can't be imported due to an error in editing an existing import statement.
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

impl Error for ResolutionError {}
