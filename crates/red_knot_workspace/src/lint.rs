use std::cell::RefCell;
use std::ops::Deref;
use std::time::Duration;

use tracing::debug_span;

use red_knot_python_semantic::types::Type;
use red_knot_python_semantic::{HasTy, ModuleName, SemanticModel};
use ruff_db::files::File;
use ruff_db::parsed::{parsed_module, ParsedModule};
use ruff_db::source::{line_index, source_text, SourceText};
use ruff_python_ast as ast;
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};
use ruff_text_size::{Ranged, TextSize};

use crate::db::Db;

/// Workaround query to test for if the computation should be cancelled.
/// Ideally, push for Salsa to expose an API for testing if cancellation was requested.
#[salsa::tracked]
#[allow(unused_variables)]
pub(crate) fn unwind_if_cancelled(db: &dyn Db) {}

#[salsa::tracked(return_ref)]
pub(crate) fn lint_syntax(db: &dyn Db, file_id: File) -> Diagnostics {
    #[allow(clippy::print_stdout)]
    if std::env::var("RED_KNOT_SLOW_LINT").is_ok() {
        for i in 0..10 {
            unwind_if_cancelled(db);

            println!("RED_KNOT_SLOW_LINT is set, sleeping for {i}/10 seconds");
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    let mut diagnostics = Vec::new();

    let source = source_text(db.upcast(), file_id);
    lint_lines(&source, &mut diagnostics);

    let parsed = parsed_module(db.upcast(), file_id);

    if parsed.errors().is_empty() {
        let ast = parsed.syntax();

        let mut visitor = SyntaxLintVisitor {
            diagnostics,
            source: &source,
        };
        visitor.visit_body(&ast.body);
        diagnostics = visitor.diagnostics;
    } else {
        let path = file_id.path(db);
        let line_index = line_index(db.upcast(), file_id);
        diagnostics.extend(parsed.errors().iter().map(|err| {
            let source_location = line_index.source_location(err.location.start(), source.as_str());
            format!(
                "{}:{}:{}: {}",
                path.as_str(),
                source_location.row,
                source_location.column,
                err,
            )
        }));
    }

    Diagnostics::from(diagnostics)
}

fn lint_lines(source: &str, diagnostics: &mut Vec<String>) {
    for (line_number, line) in source.lines().enumerate() {
        if line.len() < 88 {
            continue;
        }

        let char_count = line.chars().count();
        if char_count > 88 {
            diagnostics.push(format!(
                "Line {} is too long ({} characters)",
                line_number + 1,
                char_count
            ));
        }
    }
}

#[allow(unreachable_pub)]
#[salsa::tracked(return_ref)]
pub fn lint_semantic(db: &dyn Db, file_id: File) -> Diagnostics {
    let _span = debug_span!("lint_semantic", file=%file_id.path(db)).entered();

    let source = source_text(db.upcast(), file_id);
    let parsed = parsed_module(db.upcast(), file_id);
    let semantic = SemanticModel::new(db.upcast(), file_id);

    if !parsed.is_valid() {
        return Diagnostics::Empty;
    }

    let context = SemanticLintContext {
        source,
        parsed,
        semantic,
        diagnostics: RefCell::new(Vec::new()),
    };

    SemanticVisitor { context: &context }.visit_body(parsed.suite());

    Diagnostics::from(context.diagnostics.take())
}

fn format_diagnostic(context: &SemanticLintContext, message: &str, start: TextSize) -> String {
    let source_location = context
        .semantic
        .line_index()
        .source_location(start, context.source_text());
    format!(
        "{}:{}:{}: {}",
        context.semantic.file_path().as_str(),
        source_location.row,
        source_location.column,
        message,
    )
}

fn lint_unresolved_imports(context: &SemanticLintContext, import: AnyImportRef) {
    match import {
        AnyImportRef::Import(import) => {
            for alias in &import.names {
                let ty = alias.ty(&context.semantic);

                if ty.is_unbound() {
                    context.push_diagnostic(format_diagnostic(
                        context,
                        &format!("Unresolved import '{}'", &alias.name),
                        alias.start(),
                    ));
                }
            }
        }
        AnyImportRef::ImportFrom(import) => {
            let ast::StmtImportFrom::MemberList(import) = import else {
                // TODO support `*` imports
                return;
            };
            for alias in &import.names {
                let ty = alias.ty(&context.semantic);

                if ty.is_unbound() {
                    context.push_diagnostic(format_diagnostic(
                        context,
                        &format!("Unresolved import '{}'", &alias.name),
                        alias.start(),
                    ));
                }
            }
        }
    }
}

fn lint_maybe_undefined(context: &SemanticLintContext, name: &ast::ExprName) {
    if !matches!(name.ctx, ast::ExprContext::Load) {
        return;
    }
    let semantic = &context.semantic;
    match name.ty(semantic) {
        Type::Unbound => {
            context.push_diagnostic(format_diagnostic(
                context,
                &format!("Name '{}' used when not defined.", &name.id),
                name.start(),
            ));
        }
        Type::Union(union) if union.contains(semantic.db(), Type::Unbound) => {
            context.push_diagnostic(format_diagnostic(
                context,
                &format!("Name '{}' used when possibly not defined.", &name.id),
                name.start(),
            ));
        }
        _ => {}
    }
}

fn lint_bad_override(context: &SemanticLintContext, class: &ast::StmtClassDef) {
    let semantic = &context.semantic;

    // TODO we should have a special marker on the real typing module (from typeshed) so if you
    //   have your own "typing" module in your project, we don't consider it THE typing module (and
    //   same for other stdlib modules that our lint rules care about)
    let Some(typing) = semantic.resolve_module(ModuleName::new("typing").unwrap()) else {
        return;
    };

    let override_ty = semantic.global_symbol_ty(&typing, "override");

    let Type::Class(class_ty) = class.ty(semantic) else {
        return;
    };

    for function in class
        .body
        .iter()
        .filter_map(|stmt| stmt.as_function_def_stmt())
    {
        let Type::Function(ty) = function.ty(semantic) else {
            return;
        };

        // TODO this shouldn't make direct use of the Db; see comment on SemanticModel::db
        let db = semantic.db();

        if ty.has_decorator(db, override_ty) {
            let method_name = ty.name(db);
            if class_ty
                .inherited_class_member(db, &method_name)
                .is_unbound()
            {
                // TODO should have a qualname() method to support nested classes
                context.push_diagnostic(
                    format!(
                        "Method {}.{} is decorated with `typing.override` but does not override any base class method",
                        class_ty.name(db),
                        method_name,
                    ));
            }
        }
    }
}

pub(crate) struct SemanticLintContext<'a> {
    source: SourceText,
    parsed: &'a ParsedModule,
    semantic: SemanticModel<'a>,
    diagnostics: RefCell<Vec<String>>,
}

impl<'db> SemanticLintContext<'db> {
    #[allow(unused)]
    pub(crate) fn source_text(&self) -> &str {
        self.source.as_str()
    }

    #[allow(unused)]
    pub(crate) fn ast(&self) -> &'db ast::ModModule {
        self.parsed.syntax()
    }

    pub(crate) fn push_diagnostic(&self, diagnostic: String) {
        self.diagnostics.borrow_mut().push(diagnostic);
    }

    #[allow(unused)]
    pub(crate) fn extend_diagnostics(&mut self, diagnostics: impl IntoIterator<Item = String>) {
        self.diagnostics.get_mut().extend(diagnostics);
    }
}

#[derive(Debug)]
struct SyntaxLintVisitor<'a> {
    diagnostics: Vec<String>,
    source: &'a str,
}

impl Visitor<'_> for SyntaxLintVisitor<'_> {
    fn visit_string_literal(&mut self, string_literal: &'_ ast::StringLiteral) {
        // A very naive implementation of use double quotes
        let text = &self.source[string_literal.range];

        if text.starts_with('\'') {
            self.diagnostics
                .push("Use double quotes for strings".to_string());
        }
    }
}

struct SemanticVisitor<'a> {
    context: &'a SemanticLintContext<'a>,
}

impl Visitor<'_> for SemanticVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::ClassDef(class) => {
                lint_bad_override(self.context, class);
            }
            ast::Stmt::Import(import) => {
                lint_unresolved_imports(self.context, AnyImportRef::Import(import));
            }
            ast::Stmt::ImportFrom(import) => {
                lint_unresolved_imports(self.context, AnyImportRef::ImportFrom(import));
            }
            _ => {}
        }

        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        match expr {
            ast::Expr::Name(name) if matches!(name.ctx, ast::ExprContext::Load) => {
                lint_maybe_undefined(self.context, name);
            }
            _ => {}
        }

        walk_expr(self, expr);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diagnostics {
    Empty,
    List(Vec<String>),
}

impl Diagnostics {
    pub fn as_slice(&self) -> &[String] {
        match self {
            Diagnostics::Empty => &[],
            Diagnostics::List(list) => list.as_slice(),
        }
    }
}

impl Deref for Diagnostics {
    type Target = [String];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl From<Vec<String>> for Diagnostics {
    fn from(value: Vec<String>) -> Self {
        if value.is_empty() {
            Diagnostics::Empty
        } else {
            Diagnostics::List(value)
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum AnyImportRef<'a> {
    Import(&'a ast::StmtImport),
    ImportFrom(&'a ast::StmtImportFrom),
}

impl Ranged for AnyImportRef<'_> {
    fn range(&self) -> ruff_text_size::TextRange {
        match self {
            AnyImportRef::Import(import) => import.range(),
            AnyImportRef::ImportFrom(import) => import.range(),
        }
    }
}

#[cfg(test)]
mod tests {
    use red_knot_python_semantic::{Program, ProgramSettings, PythonVersion, SearchPathSettings};
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};

    use crate::db::tests::TestDb;

    use super::{lint_semantic, Diagnostics};

    fn setup_db() -> TestDb {
        setup_db_with_root(SystemPathBuf::from("/src"))
    }

    fn setup_db_with_root(src_root: SystemPathBuf) -> TestDb {
        let db = TestDb::new();

        db.memory_file_system()
            .create_directory_all(&src_root)
            .unwrap();

        Program::from_settings(
            &db,
            ProgramSettings {
                target_version: PythonVersion::default(),
                search_paths: SearchPathSettings {
                    extra_paths: Vec::new(),
                    src_root,
                    site_packages: vec![],
                    custom_typeshed: None,
                },
            },
        )
        .expect("Valid program settings");

        db
    }

    #[test]
    fn undefined_variable() {
        let mut db = setup_db();

        db.write_dedented(
            "/src/a.py",
            "
            x = int
            if flag:
                y = x
            y
            ",
        )
        .unwrap();

        let file = system_path_to_file(&db, "/src/a.py").expect("file to exist");
        let Diagnostics::List(messages) = lint_semantic(&db, file) else {
            panic!("expected some diagnostics");
        };

        assert_eq!(
            *messages,
            if cfg!(windows) {
                vec![
                    "\\src\\a.py:3:4: Name 'flag' used when not defined.",
                    "\\src\\a.py:5:1: Name 'y' used when possibly not defined.",
                ]
            } else {
                vec![
                    "/src/a.py:3:4: Name 'flag' used when not defined.",
                    "/src/a.py:5:1: Name 'y' used when possibly not defined.",
                ]
            }
        );
    }
}
