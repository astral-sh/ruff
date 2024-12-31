use std::collections::HashMap;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, visitor::Visitor, Expr, Stmt};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

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
pub(crate) struct ReturnInGenerator;

impl Violation for ReturnInGenerator {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Using `yield` and `return {value}` in a generator function can lead to confusing behavior"
            .to_string()
    }
}

/// B901
pub(crate) fn return_in_generator(checker: &mut Checker, function_def: &ast::StmtFunctionDef) {
    if function_def.name.id == "__await__" {
        return;
    }

    let mut visitor = ReturnInGeneratorVisitor::default();
    ast::statement_visitor::StatementVisitor::visit_body(&mut visitor, &function_def.body);

    if visitor.has_yield {
        if let Some(return_) = visitor.return_ {
            checker
                .diagnostics
                .push(Diagnostic::new(ReturnInGenerator, return_));
        }
    }
}

enum BindState {
    Stored,
    Reassigned,
}

#[derive(Default)]
struct ReturnInGeneratorVisitor {
    return_: Option<TextRange>,
    has_yield: bool,
    yield_expr_names: HashMap<String, BindState>,
    yield_on_last_visit: bool,
}

impl ast::statement_visitor::StatementVisitor<'_> for ReturnInGeneratorVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(ast::StmtExpr { value, .. }) => match **value {
                Expr::Yield(_) | Expr::YieldFrom(_) => {
                    self.has_yield = true;
                }
                _ => {
                    self.visit_expr(value);
                }
            },
            Stmt::FunctionDef(_) => {
                // Do not recurse into nested functions; they're evaluated separately.
            }
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                for target in targets {
                    self.discover_yield_assignments(target, value);
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value: Some(value),
                ..
            }) => {
                self.yield_on_last_visit = false;
                self.visit_expr(value);
                self.evaluate_target(target);
            }
            Stmt::Return(ast::StmtReturn {
                value: Some(value),
                range,
            }) => {
                if let Expr::Name(ast::ExprName { ref id, .. }) = **value {
                    if !matches!(
                        self.yield_expr_names.get(id.as_str()),
                        Some(BindState::Reassigned) | None
                    ) {
                        return;
                    }
                }
                self.return_ = Some(*range);
            }
            _ => ast::statement_visitor::walk_stmt(self, stmt),
        }
    }
}

impl Visitor<'_> for ReturnInGeneratorVisitor {
    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => {
                self.has_yield = true;
                self.yield_on_last_visit = true;
            }
            Expr::Lambda(_) | Expr::Call(_) => {}
            _ => ast::visitor::walk_expr(self, expr),
        }
    }
}

impl ReturnInGeneratorVisitor {
    /// Determine if a target is bound to a yield or a yield from expression and,
    /// if so, track that target
    fn evaluate_target(&mut self, target: &Expr) {
        if let Expr::Name(ast::ExprName { ref id, .. }) = *target {
            if self.yield_on_last_visit {
                match self.yield_expr_names.get(id.as_str()) {
                    Some(BindState::Reassigned) => {}
                    _ => {
                        self.yield_expr_names
                            .insert(id.to_string(), BindState::Stored);
                    }
                }
            } else {
                if let Some(BindState::Stored) = self.yield_expr_names.get(id.as_str()) {
                    self.yield_expr_names
                        .insert(id.to_string(), BindState::Reassigned);
                }
            }
        }
    }

    /// Given a target and a value, track any identifiers that are bound to
    /// yield or yield from expressions
    fn discover_yield_assignments(&mut self, target: &Expr, value: &Expr) {
        match target {
            Expr::Name(_) => {
                self.yield_on_last_visit = false;
                self.visit_expr(value);
                self.evaluate_target(target);
            }
            Expr::Tuple(ast::ExprTuple { elts: tar_elts, .. })
            | Expr::List(ast::ExprList { elts: tar_elts, .. }) => match value {
                Expr::Tuple(ast::ExprTuple { elts: val_elts, .. })
                | Expr::List(ast::ExprList { elts: val_elts, .. })
                | Expr::Set(ast::ExprSet { elts: val_elts, .. }) => {
                    self.discover_yield_container_assignments(tar_elts, val_elts);
                }
                Expr::Yield(_) | Expr::YieldFrom(_) => {
                    self.has_yield = true;
                    self.yield_on_last_visit = true;
                    self.evaluate_target(target);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn discover_yield_container_assignments(&mut self, targets: &[Expr], values: &[Expr]) {
        for (target, value) in targets.iter().zip(values) {
            match target {
                Expr::Tuple(ast::ExprTuple { elts: tar_elts, .. })
                | Expr::List(ast::ExprList { elts: tar_elts, .. })
                | Expr::Set(ast::ExprSet { elts: tar_elts, .. }) => {
                    match value {
                        Expr::Tuple(ast::ExprTuple { elts: val_elts, .. })
                        | Expr::List(ast::ExprList { elts: val_elts, .. })
                        | Expr::Set(ast::ExprSet { elts: val_elts, .. }) => {
                            self.discover_yield_container_assignments(tar_elts, val_elts);
                        }
                        Expr::Yield(_) | Expr::YieldFrom(_) => {
                            self.has_yield = true;
                            self.yield_on_last_visit = true;
                            self.evaluate_target(target);
                        }
                        _ => {}
                    };
                }
                Expr::Name(_) => {
                    self.yield_on_last_visit = false;
                    self.visit_expr(value);
                    self.evaluate_target(target);
                }
                _ => {}
            }
        }
    }
}
