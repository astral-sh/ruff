//! Line-level type coverage, as reported by `ty check --html-report`.
//!
//! This follows mypy's line-precision reports (`mypy --html-report`). Every expression, parameter,
//! and import is classified by its inferred type, and each line takes the least precise
//! classification of the nodes that start on it. Lines on which no node starts, such as blank lines,
//! comments, docstrings, and the continuation lines of a multi-line expression, are [`Empty`].
//!
//! [`Empty`]: LineCoverage::Empty

use std::fmt::Write as _;

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::{SourceText, line_index, source_text};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{self as ast};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

use crate::types::visitor::any_over_type_through_aliases;
use crate::types::{KnownClass, ProgramEnvironment, Type};
use crate::{Db, HasType, SemanticModel};

/// The coverage classification of a single line.
///
/// Variants are ordered from most to least precise (after [`Empty`](Self::Empty)), so that the
/// classification of a line is the maximum over the nodes that start on it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LineCoverage {
    /// No node starts on this line.
    #[default]
    Empty,
    /// Every type on this line is fully known.
    Precise,
    /// A type on this line contains a dynamic type, for example `list[Unknown]`.
    Imprecise,
    /// A type on this line is itself dynamic, for example `Unknown` or `Any`.
    Dynamic,
}

/// The coverage of a single line.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Line {
    pub coverage: LineCoverage,
    /// A note for each imprecise or dynamic node that starts on this line, naming the node and its
    /// type, for example ``"`y.foo`: Unknown"``.
    pub notes: Vec<String>,
}

/// Returns the coverage of each line in `file`, indexed by zero-based line number.
pub fn line_coverage(db: &dyn Db, file: File) -> Vec<Line> {
    let line_index = line_index(db, file);
    let source = source_text(db, file);
    let file = db.program_file(file);
    let model = SemanticModel::new(db, file);

    let mut visitor = CoverageVisitor {
        lines: vec![Line::default(); line_index.line_count()],
        env: model.program_environment(),
        model,
        line_index,
        source,
    };

    let ast = parsed_module(db, file.python_file(db)).load(db);
    visitor.visit_body(ast.suite());
    visitor.lines
}

struct CoverageVisitor<'db> {
    model: SemanticModel<'db>,
    env: ProgramEnvironment<'db>,
    line_index: LineIndex,
    source: SourceText,
    lines: Vec<Line>,
}

impl<'db> CoverageVisitor<'db> {
    /// Returns the line on which the node at `range` starts.
    fn line_mut(&mut self, range: TextRange) -> Option<&mut Line> {
        let line_number = self.line_index.line_index(range.start()).to_zero_indexed();
        self.lines.get_mut(line_number)
    }

    /// Records the type of the node at `range` on the line where the node starts.
    ///
    /// Nodes without a type, such as expressions in unreachable code, are not recorded.
    fn record(&mut self, ty: Option<Type<'db>>, range: TextRange) {
        if let Some(ty) = ty {
            let coverage = classify(self.model.db(), &self.env, ty);
            self.record_coverage(coverage, ty, range);
        }
    }

    /// Records `coverage` for the node at `range`, whose type is `ty`.
    fn record_coverage(&mut self, coverage: LineCoverage, ty: Type<'db>, range: TextRange) {
        let note = (coverage > LineCoverage::Precise).then(|| self.note(ty, range));
        let Some(line) = self.line_mut(range) else {
            return;
        };

        line.coverage = line.coverage.max(coverage);
        line.notes.extend(note);
    }

    /// Describes the node at `range` and its type for the line's hover text.
    fn note(&self, ty: Type<'db>, range: TextRange) -> String {
        // Show only the first line of a multi-line node, and shorten long nodes.
        let text = self.source.as_str()[range]
            .lines()
            .next()
            .unwrap_or_default();
        let mut note = String::from("`");
        if text.chars().count() > 40 {
            note.extend(text.chars().take(39));
            note.push('…');
        } else {
            note.push_str(text);
        }
        let _ = write!(note, "`: {}", ty.display(self.model.db(), &self.env));
        note
    }
}

fn classify<'db>(db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> LineCoverage {
    if ty.is_dynamic() {
        return LineCoverage::Dynamic;
    }

    // Like mypy, we don't consider a reference to a function or class imprecise just because its
    // signature mentions a dynamic type. For example, in `f(x)`, where `f` is `def f(a, b: int)`,
    // the imprecision is reported on the line that defines `a`, and on any line that uses the
    // `Unknown` return type of `f(x)`, but not on every line that mentions `f`.
    if matches!(
        ty,
        Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::Callable(_) | Type::ClassLiteral(_)
    ) {
        return LineCoverage::Precise;
    }

    // ty infers `CoroutineType[Any, Any, T]` for a call to an `async def` function. Since ty adds
    // those `Any`s itself, we classify the coroutine by its result type `T`, much as mypy treats
    // `Any`s it creates for special forms as precise.
    if let Some(specialization) = ty.known_specialization(db, env, KnownClass::CoroutineType)
        && let [_, _, result] = specialization.types(db)
    {
        return classify(db, env, *result).min(LineCoverage::Imprecise);
    }

    if any_over_type_through_aliases(db, env, ty, |ty| ty.is_dynamic()) {
        LineCoverage::Imprecise
    } else {
        LineCoverage::Precise
    }
}

impl SourceOrderVisitor<'_> for CoverageVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            // Docstrings carry no type information, so their lines stay empty.
            stmt if is_docstring_stmt(stmt) => return,
            ast::Stmt::FunctionDef(function) => {
                self.record(function.inferred_type(&self.model), function.name.range());
            }
            ast::Stmt::ClassDef(class) => {
                self.record(class.inferred_type(&self.model), class.name.range());
            }
            // These statements have no expressions, but like mypy we count their lines as precise
            // rather than empty.
            ast::Stmt::Pass(_) | ast::Stmt::Break(_) | ast::Stmt::Continue(_) => {
                if let Some(line) = self.line_mut(stmt.range()) {
                    line.coverage = line.coverage.max(LineCoverage::Precise);
                }
            }
            // The value of a `type` statement is a type expression, which we classify through the
            // alias's name rather than expression by expression (see `visit_annotation`).
            ast::Stmt::TypeAlias(type_alias) => {
                self.visit_expr(&type_alias.name);
                return;
            }
            _ => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        self.record(expr.inferred_type(&self.model), expr.range());
        source_order::walk_expr(self, expr);
    }

    /// Records the type that an annotation declares, without visiting the expressions inside it.
    ///
    /// Like mypy, we classify `def f(x: list[Any])` by the declared type `list[Any]`, which is
    /// imprecise. Visiting the annotation's sub-expressions would classify the `Any` inside it as
    /// dynamic, even though no value on the line has a dynamic type.
    fn visit_annotation(&mut self, expr: &ast::Expr) {
        self.record(expr.inferred_type(&self.model), expr.range());
    }

    fn visit_parameter(&mut self, parameter: &ast::Parameter) {
        self.record(parameter.inferred_type(&self.model), parameter.name.range());
        source_order::walk_parameter(self, parameter);
    }

    /// Like mypy, we classify an import by whether it resolves, not by the type of what it imports.
    /// For example, importing a type alias `Json = dict[str, Any]` is precise.
    fn visit_alias(&mut self, alias: &ast::Alias) {
        if let Some(ty) = alias.inferred_type(&self.model) {
            let coverage = if ty.is_dynamic() {
                LineCoverage::Dynamic
            } else {
                LineCoverage::Precise
            };
            self.record_coverage(coverage, ty, alias.range());
        }
        source_order::walk_alias(self, alias);
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_python_trivia::textwrap::dedent;

    use super::{LineCoverage, line_coverage};
    use crate::db::tests::setup_db;

    /// Renders each source line prefixed with a marker for its classification.
    fn annotated(source: &str) -> String {
        annotated_with_modules(&[], source)
    }

    /// Like [`annotated`], with other modules that `source` can import.
    fn annotated_with_modules(modules: &[(&str, &str)], source: &str) -> String {
        let mut db = setup_db();
        for (path, module_source) in modules {
            db.write_file(path, dedent(module_source).as_ref()).unwrap();
        }
        let source = dedent(source);
        db.write_file("test.py", source.as_ref()).unwrap();
        let file = system_path_to_file(&db, "test.py").unwrap();

        let lines = line_coverage(&db, file);
        let mut output = String::new();
        for (line, coverage) in source.lines().zip(lines) {
            let marker = match coverage.coverage {
                LineCoverage::Empty => ' ',
                LineCoverage::Precise => 'P',
                LineCoverage::Imprecise => 'I',
                LineCoverage::Dynamic => 'D',
            };
            write!(output, "{marker} | {line}").unwrap();
            if !coverage.notes.is_empty() {
                write!(output, "  # {}", coverage.notes.join(", ")).unwrap();
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn async_protocols_and_aliases() {
        insta::assert_snapshot!(annotated(
            r#"
            from typing import Any, Iterable

            async def fetch() -> int:
                return 1

            async def main() -> None:
                coroutine = fetch()
                result = await coroutine

            def chunks() -> Iterable[bytes]:
                return [b""]

            type Json = dict[str, Any]

            def parse() -> Json:
                return {}

            data = parse()
            "#
        ), @r#"
          | 
        P | from typing import Any, Iterable
          | 
        P | async def fetch() -> int:
        P |     return 1
          | 
        P | async def main() -> None:
        P |     coroutine = fetch()
        P |     result = await coroutine
          | 
        P | def chunks() -> Iterable[bytes]:
        P |     return [b""]
          | 
        I | type Json = dict[str, Any]  # `Json`: TypeAliasType
          | 
        I | def parse() -> Json:  # `Json`: Json
        I |     return {}  # `{}`: dict[str, Any]
          | 
        I | data = parse()  # `data`: Json, `parse()`: Json
        "#);
    }

    #[test]
    fn annotations_and_imports() {
        insta::assert_snapshot!(annotated_with_modules(
            &[("/src/aliases.py", "from typing import Any\n\nJson = dict[str, Any]\n")],
            r#"
            from typing import Any
            from aliases import Json
            from does_not_exist import Missing

            def send(message: dict[str, Any]) -> None:
                pass

            def load(raw: str) -> Json:
                return {}

            def first(values: list[int]) -> list[int]:
                return values[1:]
            "#
        ), @"
          | 
        P | from typing import Any
        P | from aliases import Json
        D | from does_not_exist import Missing  # `Missing`: Unknown
          | 
        I | def send(message: dict[str, Any]) -> None:  # `message`: dict[str, Any], `dict[str, Any]`: dict[str, Any]
        P |     pass
          | 
        I | def load(raw: str) -> Json:  # `Json`: dict[str, Any]
        I |     return {}  # `{}`: dict[str, Any]
          | 
        P | def first(values: list[int]) -> list[int]:
        P |     return values[1:]
        ");
    }

    #[test]
    fn classifies_lines() {
        insta::assert_snapshot!(annotated(
            r#"
            """Docstring."""
            import does_not_exist
            from typing import Any

            def annotated(x: int) -> list[int]:
                return [x]

            def unannotated(y):
                return y

            def explicit_any(z: Any) -> list[Any]:
                return [z]

            a = annotated(1)
            b = unannotated(1)
            c = explicit_any(1)
            d = annotated(
                1,
            )
            "#
        ), @r#"
          | 
          | """Docstring."""
        D | import does_not_exist  # `does_not_exist`: Unknown
        P | from typing import Any
          | 
        P | def annotated(x: int) -> list[int]:
        P |     return [x]
          | 
        D | def unannotated(y):  # `y`: Unknown
        D |     return y  # `y`: Unknown
          | 
        D | def explicit_any(z: Any) -> list[Any]:  # `z`: Any, `Any`: Any, `list[Any]`: list[Any]
        D |     return [z]  # `[z]`: list[Any], `z`: Any
          | 
        P | a = annotated(1)
        D | b = unannotated(1)  # `b`: Unknown, `unannotated(1)`: Unknown
        I | c = explicit_any(1)  # `c`: list[Any], `explicit_any(1)`: list[Any]
        P | d = annotated(
        P |     1,
          | )
        "#);
    }
}
