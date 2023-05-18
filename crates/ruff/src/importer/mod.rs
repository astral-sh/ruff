//! Add and modify import statements to make module members available during fix execution.

use anyhow::Result;
use libcst_native::{Codegen, CodegenState, ImportAlias, Name, NameOrAttribute};
use ruff_text_size::TextSize;
use rustpython_parser::ast::{self, Ranged, Stmt, Suite};

use ruff_diagnostics::Edit;
use ruff_python_ast::imports::AnyImport;
use ruff_python_ast::source_code::{Locator, Stylist};

use crate::cst::matchers::{match_aliases, match_import_from, match_module};
use crate::importer::insertion::Insertion;

mod insertion;

pub struct Importer<'a> {
    python_ast: &'a Suite,
    locator: &'a Locator<'a>,
    stylist: &'a Stylist<'a>,
    ordered_imports: Vec<&'a Stmt>,
}

impl<'a> Importer<'a> {
    pub fn new(python_ast: &'a Suite, locator: &'a Locator<'a>, stylist: &'a Stylist<'a>) -> Self {
        Self {
            python_ast,
            locator,
            stylist,
            ordered_imports: Vec::default(),
        }
    }

    /// Visit a top-level import statement.
    pub fn visit_import(&mut self, import: &'a Stmt) {
        self.ordered_imports.push(import);
    }

    /// Return the import statement that precedes the given position, if any.
    fn preceding_import(&self, at: TextSize) -> Option<&Stmt> {
        self.ordered_imports
            .partition_point(|stmt| stmt.start() < at)
            .checked_sub(1)
            .map(|idx| self.ordered_imports[idx])
    }

    /// Add an import statement to import the given module.
    ///
    /// If there are no existing imports, the new import will be added at the top
    /// of the file. Otherwise, it will be added after the most recent top-level
    /// import statement.
    pub fn add_import(&self, import: &AnyImport, at: TextSize) -> Edit {
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

    /// Return the top-level [`Stmt`] that imports the given module using `Stmt::ImportFrom`
    /// preceding the given position, if any.
    pub fn find_import_from(&self, module: &str, at: TextSize) -> Option<&Stmt> {
        let mut import_from = None;
        for stmt in &self.ordered_imports {
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
    pub fn add_member(&self, stmt: &Stmt, member: &str) -> Result<Edit> {
        let mut tree = match_module(self.locator.slice(stmt.range()))?;
        let import_from = match_import_from(&mut tree)?;
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
        tree.codegen(&mut state);
        Ok(Edit::range_replacement(state.to_string(), stmt.range()))
    }
}
