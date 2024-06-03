use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;

use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{ModModule, StringLiteral};
use ruff_python_parser::Parsed;

use crate::cache::KeyValueCache;
use crate::db::{LintDb, LintJar, QueryResult};
use crate::files::FileId;
use crate::module::ModuleName;
use crate::parse::parse;
use crate::source::{source_text, Source};
use crate::symbols::{
    resolve_global_symbol, symbol_table, Definition, GlobalSymbolId, SymbolId, SymbolTable,
};
use crate::types::{infer_definition_type, infer_symbol_public_type, Type};

#[tracing::instrument(level = "debug", skip(db))]
pub(crate) fn lint_syntax(db: &dyn LintDb, file_id: FileId) -> QueryResult<Diagnostics> {
    let lint_jar: &LintJar = db.jar()?;
    let storage = &lint_jar.lint_syntax;

    #[allow(clippy::print_stdout)]
    if std::env::var("RED_KNOT_SLOW_LINT").is_ok() {
        for i in 0..10 {
            db.cancelled()?;
            println!("RED_KNOT_SLOW_LINT is set, sleeping for {i}/10 seconds");
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    storage.get(&file_id, |file_id| {
        let mut diagnostics = Vec::new();

        let source = source_text(db.upcast(), *file_id)?;
        lint_lines(source.text(), &mut diagnostics);

        let parsed = parse(db.upcast(), *file_id)?;

        if parsed.errors().is_empty() {
            let ast = parsed.syntax();

            let mut visitor = SyntaxLintVisitor {
                diagnostics,
                source: source.text(),
            };
            visitor.visit_body(&ast.body);
            diagnostics = visitor.diagnostics;
        } else {
            diagnostics.extend(parsed.errors().iter().map(std::string::ToString::to_string));
        }

        Ok(Diagnostics::from(diagnostics))
    })
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

#[tracing::instrument(level = "debug", skip(db))]
pub(crate) fn lint_semantic(db: &dyn LintDb, file_id: FileId) -> QueryResult<Diagnostics> {
    let lint_jar: &LintJar = db.jar()?;
    let storage = &lint_jar.lint_semantic;

    storage.get(&file_id, |file_id| {
        let source = source_text(db.upcast(), *file_id)?;
        let parsed = parse(db.upcast(), *file_id)?;
        let symbols = symbol_table(db.upcast(), *file_id)?;

        let context = SemanticLintContext {
            file_id: *file_id,
            source,
            parsed: &*parsed,
            symbols,
            db,
            diagnostics: RefCell::new(Vec::new()),
        };

        lint_unresolved_imports(&context)?;
        lint_bad_overrides(&context)?;

        Ok(Diagnostics::from(context.diagnostics.take()))
    })
}

fn lint_unresolved_imports(context: &SemanticLintContext) -> QueryResult<()> {
    // TODO: Consider iterating over the dependencies (imports) only instead of all definitions.
    for (symbol, definition) in context.symbols().all_definitions() {
        match definition {
            Definition::Import(import) => {
                let ty = context.infer_symbol_public_type(symbol)?;

                if ty.is_unknown() {
                    context.push_diagnostic(format!("Unresolved module {}", import.module));
                }
            }
            Definition::ImportFrom(import) => {
                let ty = context.infer_symbol_public_type(symbol)?;

                if ty.is_unknown() {
                    let module_name = import.module().map(Deref::deref).unwrap_or_default();
                    let message = if import.level() > 0 {
                        format!(
                            "Unresolved relative import '{}' from {}{}",
                            import.name(),
                            ".".repeat(import.level() as usize),
                            module_name
                        )
                    } else {
                        format!(
                            "Unresolved import '{}' from '{}'",
                            import.name(),
                            module_name
                        )
                    };

                    context.push_diagnostic(message);
                }
            }
            _ => {}
        }
    }

    Ok(())
}

fn lint_bad_overrides(context: &SemanticLintContext) -> QueryResult<()> {
    // TODO we should have a special marker on the real typing module (from typeshed) so if you
    // have your own "typing" module in your project, we don't consider it THE typing module (and
    // same for other stdlib modules that our lint rules care about)
    let Some(typing_override) =
        resolve_global_symbol(context.db.upcast(), ModuleName::new("typing"), "override")?
    else {
        // TODO once we bundle typeshed, this should be unreachable!()
        return Ok(());
    };

    // TODO we should maybe index definitions by type instead of iterating all, or else iterate all
    // just once, match, and branch to all lint rules that care about a type of definition
    for (symbol, definition) in context.symbols().all_definitions() {
        if !matches!(definition, Definition::FunctionDef(_)) {
            continue;
        }
        let ty = infer_definition_type(
            context.db.upcast(),
            GlobalSymbolId {
                file_id: context.file_id,
                symbol_id: symbol,
            },
            definition.clone(),
        )?;
        let Type::Function(func) = ty else {
            unreachable!("type of a FunctionDef should always be a Function");
        };
        let Some(class) = func.get_containing_class(context.db.upcast())? else {
            // not a method of a class
            continue;
        };
        if func.has_decorator(context.db.upcast(), typing_override)? {
            let method_name = func.name(context.db.upcast())?;
            if class
                .get_super_class_member(context.db.upcast(), &method_name)?
                .is_none()
            {
                // TODO should have a qualname() method to support nested classes
                context.push_diagnostic(
                    format!(
                        "Method {}.{} is decorated with `typing.override` but does not override any base class method",
                        class.name(context.db.upcast())?,
                        method_name,
                    ));
            }
        }
    }
    Ok(())
}

pub struct SemanticLintContext<'a> {
    file_id: FileId,
    source: Source,
    parsed: &'a Parsed<ModModule>,
    symbols: Arc<SymbolTable>,
    db: &'a dyn LintDb,
    diagnostics: RefCell<Vec<String>>,
}

impl<'a> SemanticLintContext<'a> {
    pub fn source_text(&self) -> &str {
        self.source.text()
    }

    pub fn file_id(&self) -> FileId {
        self.file_id
    }

    pub fn ast(&self) -> &'a ModModule {
        self.parsed.syntax()
    }

    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    pub fn infer_symbol_public_type(&self, symbol_id: SymbolId) -> QueryResult<Type> {
        infer_symbol_public_type(
            self.db.upcast(),
            GlobalSymbolId {
                file_id: self.file_id,
                symbol_id,
            },
        )
    }

    pub fn push_diagnostic(&self, diagnostic: String) {
        self.diagnostics.borrow_mut().push(diagnostic);
    }

    pub fn extend_diagnostics(&mut self, diagnostics: impl IntoIterator<Item = String>) {
        self.diagnostics.get_mut().extend(diagnostics);
    }
}

#[derive(Debug)]
struct SyntaxLintVisitor<'a> {
    diagnostics: Vec<String>,
    source: &'a str,
}

impl Visitor<'_> for SyntaxLintVisitor<'_> {
    fn visit_string_literal(&mut self, string_literal: &'_ StringLiteral) {
        // A very naive implementation of use double quotes
        let text = &self.source[string_literal.range];

        if text.starts_with('\'') {
            self.diagnostics
                .push("Use double quotes for strings".to_string());
        }
    }
}

#[derive(Debug, Clone)]
pub enum Diagnostics {
    Empty,
    List(Arc<Vec<String>>),
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
            Diagnostics::List(Arc::new(value))
        }
    }
}

#[derive(Default, Debug)]
pub struct LintSyntaxStorage(KeyValueCache<FileId, Diagnostics>);

impl Deref for LintSyntaxStorage {
    type Target = KeyValueCache<FileId, Diagnostics>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LintSyntaxStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Default, Debug)]
pub struct LintSemanticStorage(KeyValueCache<FileId, Diagnostics>);

impl Deref for LintSemanticStorage {
    type Target = KeyValueCache<FileId, Diagnostics>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LintSemanticStorage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
