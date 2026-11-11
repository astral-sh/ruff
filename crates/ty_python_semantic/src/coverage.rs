//! Line-level type coverage, as reported by `ty check --html-report`.
//!
//! This follows mypy's line-precision reports (`mypy --html-report`). Every expression, parameter,
//! and import is classified by its inferred type, and each line takes the least precise
//! classification of the nodes that start on it. Lines on which no node starts, such as blank lines,
//! comments, docstrings, and the continuation lines of a multi-line expression, are [`Empty`].
//!
//! [`Empty`]: LineCoverage::Empty

use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::line_index;
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::visitor::source_order::{self, SourceOrderVisitor};
use ruff_python_ast::{self as ast};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextSize};

use crate::types::visitor::any_over_type;
use crate::types::{ProgramEnvironment, Type};
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

/// Returns the coverage classification of each line in `file`, indexed by zero-based line number.
pub fn line_coverage(db: &dyn Db, file: File) -> Vec<LineCoverage> {
    let line_index = line_index(db, file);
    let file = db.program_file(file);
    let model = SemanticModel::new(db, file);

    let mut visitor = CoverageVisitor {
        lines: vec![LineCoverage::Empty; line_index.line_count()],
        env: model.program_environment(),
        model,
        line_index,
    };

    let ast = parsed_module(db, file.python_file(db)).load(db);
    visitor.visit_body(ast.suite());
    visitor.lines
}

struct CoverageVisitor<'db> {
    model: SemanticModel<'db>,
    env: ProgramEnvironment<'db>,
    line_index: LineIndex,
    lines: Vec<LineCoverage>,
}

impl<'db> CoverageVisitor<'db> {
    /// Records the type of a node that starts at `offset`.
    ///
    /// Nodes without a type, such as expressions in unreachable code, are not recorded.
    fn record(&mut self, ty: Option<Type<'db>>, offset: TextSize) {
        let Some(ty) = ty else {
            return;
        };

        let coverage = classify(self.model.db(), &self.env, ty);
        let line = self.line_index.line_index(offset).to_zero_indexed();

        if let Some(line_coverage) = self.lines.get_mut(line) {
            *line_coverage = (*line_coverage).max(coverage);
        }
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

    if any_over_type(db, env, ty, true, |ty| ty.is_dynamic()) {
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
                self.record(function.inferred_type(&self.model), function.name.start());
            }
            ast::Stmt::ClassDef(class) => {
                self.record(class.inferred_type(&self.model), class.name.start());
            }
            _ => {}
        }

        source_order::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        self.record(expr.inferred_type(&self.model), expr.start());
        source_order::walk_expr(self, expr);
    }

    fn visit_parameter(&mut self, parameter: &ast::Parameter) {
        self.record(parameter.inferred_type(&self.model), parameter.name.start());
        source_order::walk_parameter(self, parameter);
    }

    fn visit_alias(&mut self, alias: &ast::Alias) {
        self.record(alias.inferred_type(&self.model), alias.start());
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
        let mut db = setup_db();
        let source = dedent(source);
        db.write_file("test.py", source.as_ref()).unwrap();
        let file = system_path_to_file(&db, "test.py").unwrap();

        let lines = line_coverage(&db, file);
        let mut output = String::new();
        for (line, coverage) in source.lines().zip(lines) {
            let marker = match coverage {
                LineCoverage::Empty => ' ',
                LineCoverage::Precise => 'P',
                LineCoverage::Imprecise => 'I',
                LineCoverage::Dynamic => 'D',
            };
            writeln!(output, "{marker} | {line}").unwrap();
        }
        output
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
        D | import does_not_exist
        P | from typing import Any
          | 
        P | def annotated(x: int) -> list[int]:
        P |     return [x]
          | 
        D | def unannotated(y):
        D |     return y
          | 
        D | def explicit_any(z: Any) -> list[Any]:
        D |     return [z]
          | 
        P | a = annotated(1)
        D | b = unannotated(1)
        I | c = explicit_any(1)
        P | d = annotated(
        P |     1,
          | )
        "#);
    }
}
