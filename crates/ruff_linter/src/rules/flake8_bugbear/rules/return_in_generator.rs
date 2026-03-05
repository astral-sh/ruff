use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::{Visitor, walk_expr, walk_stmt};
use ruff_python_ast::{self as ast, Expr, Stmt, StmtFunctionDef};
use ruff_text_size::TextRange;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::flake8_pytest_style::helpers::is_pytest_hookimpl_wrapper;

/// ## What it does
/// Checks for `return {value}` statements in functions that also contain `yield`
/// or `yield from` statements.
///
/// ## Why is this bad?
/// Using `return {value}` in a generator function was syntactically invalid in
/// Python 2.
///
/// In Python 3 [PEP 380](https://peps.python.org/pep-0380/) added the use of
/// `return {value}` in a generator as part of implementing delegation to
/// subgenerators to allow for left-hand-sides in generator delegation:
///
/// ```python
/// def genA():
///     yield 'a'
///     yield 'b'
///     return 2
///
/// def genB():
///     count = yield from genA()
///     # count == 2
/// ```
///
/// The `return {value}` statement is equivalent to `raise StopIteration({value})`
/// but results in clearer (sub)generator code.
///
/// However, this can lead to valid code where `return` (rather than `yield from`)
/// is accidentally used for delegation.  For example given:
///
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
/// There is a bug in the `file_types=None` case which will yield no values and
/// return the `dir_path.glob('*')` iterator as the value of `StopIteration`.
/// Although this value can be captured by other generator functions, consumers
/// of `Iterators` will discard it:
///
/// ```shell
/// >>> list(get_file_paths(file_types=["cfg", "toml"]))
/// [PosixPath('setup.cfg'), PosixPath('pyproject.toml')]
/// >>> list(get_file_paths())
/// []
/// ```
///
/// Consider suppressing this diagnostic unless you are sure that
/// you will never need this language feature.
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
///         _lhs = yield from dir_path.glob("*")
///     else:
///         for file_type in file_types:
///             _lhs = yield from dir_path.glob(f"*.{file_type}")
///
///     # if you need to also forward the return value of the subgenerators
///     # return _lhs
/// ```
///
/// This examples make use of `yield from` language feature that `return` in
/// generators is part of.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.4.8")]
pub(crate) struct ReturnInGenerator;

impl Violation for ReturnInGenerator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Using `return {value}` in a generator function can mask logic errors.".to_string()
    }
}

/// B901
pub(crate) fn return_in_generator(checker: &Checker, function_def: &StmtFunctionDef) {
    if function_def.name.id == "__await__" {
        return;
    }

    // Async functions are flagged by the `ReturnInGenerator` semantic syntax error.
    if function_def.is_async {
        return;
    }

    if function_def
        .decorator_list
        .iter()
        .any(|decorator| is_pytest_hookimpl_wrapper(decorator, checker.semantic()))
    {
        return;
    }

    let mut visitor = ReturnInGeneratorVisitor::default();
    visitor.visit_body(&function_def.body);

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
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) => {
                // Do not recurse into nested functions; they're evaluated separately.
            }
            Stmt::Return(ast::StmtReturn {
                value: Some(_),
                range,
                node_index: _,
            }) => {
                self.return_ = Some(*range);
                walk_stmt(self, stmt);
            }
            _ => walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Lambda(_) => {}
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                self.has_yield = true;
            }
            _ => walk_expr(self, expr),
        }
    }
}
