use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat;
use ruff_python_ast::flat::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};
use ruff_text_size::Ranged;
use ruff_text_size::TextRange;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_pytest_style::helpers::is_pytest_hookimpl_wrapper;

/// ## What it does
/// Checks for `return {value}` statements in functions that also contain `yield`
/// or `yield from` statements.
///
/// ## Why is this bad?
/// Using `return {value}` in a generator function was syntactically invalid in
/// Python 2. In Python 3 `return {value}` _can_ be used in a generator; however,
/// the combination of `yield` and `return` can lead to confusing behavior, as
/// the `return` statement will cause the generator to raise `StopIteration`
/// with the value provided, rather than returning the value to the caller.
///
/// For example, given:
/// ```python
/// from collections.abc import Iterable
/// from pathlib import Path
///
///
/// def get_file_paths(file_types: Iterable[str] | None = None) -> Iterable[Path]:
///     dir_path = Path(".")
///     if file_types is None:
///         return dir_path.glob("*")
///
///     for file_type in file_types:
///         yield from dir_path.glob(f"*.{file_type}")
/// ```
///
/// Readers might assume that `get_file_paths()` would return an iterable of
/// `Path` objects in the directory; in reality, though, `list(get_file_paths())`
/// evaluates to `[]`, since the `return` statement causes the generator to raise
/// `StopIteration` with the value `dir_path.glob("*")`:
///
/// ```shell
/// >>> list(get_file_paths(file_types=["cfg", "toml"]))
/// [PosixPath('setup.cfg'), PosixPath('pyproject.toml')]
/// >>> list(get_file_paths())
/// []
/// ```
///
/// For intentional uses of `return` in a generator, consider suppressing this
/// diagnostic.
///
/// ## Example
/// ```python
/// from collections.abc import Iterable
/// from pathlib import Path
///
///
/// def get_file_paths(file_types: Iterable[str] | None = None) -> Iterable[Path]:
///     dir_path = Path(".")
///     if file_types is None:
///         return dir_path.glob("*")
///
///     for file_type in file_types:
///         yield from dir_path.glob(f"*.{file_type}")
/// ```
///
/// Use instead:
///
/// ```python
/// from collections.abc import Iterable
/// from pathlib import Path
///
///
/// def get_file_paths(file_types: Iterable[str] | None = None) -> Iterable[Path]:
///     dir_path = Path(".")
///     if file_types is None:
///         yield from dir_path.glob("*")
///     else:
///         for file_type in file_types:
///             yield from dir_path.glob(f"*.{file_type}")
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.4.8", category = Category::Suspicious)]
pub(crate) struct ReturnInGenerator;

impl Violation for ReturnInGenerator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Using `yield` and `return {value}` in a generator function can lead to confusing behavior"
            .to_string()
    }
}

/// B901
pub(crate) fn return_in_generator(
    checker: &Checker,
    function_def: flat::View<'_, StmtFunctionDef>,
) {
    if !function_def.body_may_contain_yield() {
        return;
    }

    if function_def.name().id() == "__await__" {
        return;
    }

    // Async functions are flagged by the `ReturnInGenerator` semantic syntax error.
    if function_def.is_async() {
        return;
    }

    if function_def
        .decorator_list()
        .iter()
        .any(|decorator| is_pytest_hookimpl_wrapper(decorator, checker.semantic()))
    {
        return;
    }

    let mut visitor = ReturnInGeneratorVisitor::default();
    visitor.visit_body(function_def.body());

    if visitor.has_yield {
        if let Some(return_) = visitor.return_ {
            checker.report_diagnostic(ReturnInGenerator, return_);
        }
    }
}

#[derive(Default)]
struct ReturnInGeneratorVisitor {
    return_: Option<TextRange>,
    has_yield: bool,
}

impl Visitor<'_> for ReturnInGeneratorVisitor {
    fn visit_stmt(&mut self, stmt: flat::View<'_, Stmt>) {
        match stmt.classify() {
            flat::StmtKind::FunctionDef(_) => {
                // Do not recurse into nested functions; they're evaluated separately.
            }
            flat::StmtKind::Return(stmt_return) if stmt_return.value().is_some() => {
                let range = stmt_return.range();

                self.return_ = Some(range);
                walk_stmt(self, stmt);
            }
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: flat::View<'_, Expr>) {
        match expr.classify() {
            flat::ExprKind::Lambda(_) => {}
            flat::ExprKind::Yield(_) | flat::ExprKind::YieldFrom(_) => {
                self.has_yield = true;
            }
            _ => walk_expr(self, expr),
        }
    }
}
