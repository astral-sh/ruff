use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    self as ast, Expr, ExprCall, ExprName, Stmt, StmtImportFrom, StmtClassDef,
    visitor::{self, Visitor},
};
use rustc_hash::FxHashMap;

use crate::Violation;
use crate::checkers::ast::Checker;
use ruff_python_ast::Identifier;

static MODULES_WHITELIST: &'static [&'static str] = &[
    "typing",
    "six.moves",
    "collections.abc",
    "typing_extensions",
    "__future__",
];

/// ## What it does
/// Checks for types or function imports.
///
/// ## Why is this bad?
/// TODO
///
/// ## Example
/// ```python
/// from pathlib import Path
/// path = Path('/')
/// ```
///
/// Use instead:
/// ```python
/// import pathlib
/// path = pathlib.Path('/')
/// ```
///
/// ## References
/// - [Google Python Style Guide - Imports](https://google.github.io/styleguide/pyguide.html#22-imports)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.4.0")]
pub(crate) struct ImportType {
    module_name: String,
    type_name: String,
}

impl Violation for ImportType {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportType {
            module_name,
            type_name,
        } = self;
        format!("Importing type/function `{type_name}` from module `{module_name}`.")
    }
}

struct ImportTypeVisitor<'a, 'b> {
    checker: &'a Checker<'b>,
    seen_imports_from: FxHashMap<String, (&'a StmtImportFrom, &'a Identifier)>,
}

impl<'a, 'b> ImportTypeVisitor<'a, 'b> {
    fn new(checker: &'a Checker<'b>) -> Self {
        Self {
            checker,
            seen_imports_from: FxHashMap::default(),
        }
    }

    fn check_symbol(&mut self, name: &str) {
        if let Some((stmt_import_from, symbol)) = self.seen_imports_from.get(name) {
            let module_name = stmt_import_from
                        .module
                        .as_ref()
                        .map_or(".", |m| m.as_ref());
            if MODULES_WHITELIST.contains(&module_name) {
                return;
            }

            self.checker.report_diagnostic(
                ImportType {
                    module_name: stmt_import_from
                        .module
                        .as_ref()
                        .map_or(".", |m| m.as_ref())
                        .to_string(),
                    type_name: name.to_string(),
                },
                symbol.range,
            );
        }
    }

    fn check_nonmodule_expr(&mut self, expr: &'a Expr) {
            if let Expr::Name(ExprName { id: func_name, .. }) = expr {
                // from foo import bar
                // bar(...)
                // => bar is a type/function

                self.check_symbol(func_name);
        }
    }
}

impl<'a, 'b> Visitor<'a> for ImportTypeVisitor<'a, 'b> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        visitor::walk_stmt(self, stmt);

        if let Stmt::ImportFrom(stmt_import_from @ StmtImportFrom { names, .. }) = stmt {
            for name in names {
                let Identifier {
                    id: symbol_name, ..
                } = &name.name;

                let imported_symbol = if let Some(Identifier { id: alias_name, .. }) = &name.asname
                {
                    alias_name
                } else {
                    symbol_name
                };
                self.seen_imports_from
                    .insert(imported_symbol.to_string(), (stmt_import_from, &name.name));
            }
        }

        if let Stmt::ClassDef(StmtClassDef {
            arguments: Some(arguments), ..
        }) = stmt {
            for arg in &*arguments.args {
                self.check_nonmodule_expr(arg);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        visitor::walk_expr(self, expr);

        if let Expr::Call(ExprCall { func, .. }) = expr {
            // func(...) => not a module

            self.check_nonmodule_expr(&*func);
        }
    }

    fn visit_annotation(&mut self, expr: &'a Expr) {
        visitor::walk_annotation(self, expr);

        self.check_nonmodule_expr(expr);
    }
}

pub(crate) fn import_type(checker: &Checker, suite: &ast::Suite) {
    let mut visitor = ImportTypeVisitor::new(checker);
    for stmt in suite {
        visitor.visit_stmt(stmt);
    }
}
