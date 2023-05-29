use rustpython_parser::ast;
use rustpython_parser::ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::prelude::Expr;
use ruff_python_semantic::definition::{Definition, Member, MemberKind};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__iter__` methods in stubs with the wrong return type annotation.
///
/// ## Why is this bad?
/// `__iter__` should return an `Iterator`, not an `Iterable`.
///
/// ## Example
/// ```python
/// class Foo:
///     def __iter__(self) -> collections.abc.Iterable: ...
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def __iter__(self) -> collections.abc.Iterator: ...
/// ```
#[violation]
pub struct IterMethodReturnIterable;

impl Violation for IterMethodReturnIterable {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("__iter__ methods should never return ` Iterable[T]`, as they should always return some kind of `Iterator`.")
    }
}

/// PYI045
pub(crate) fn iter_method_return_iterable(checker: &mut Checker, definition: &Definition) {
    let Definition::Member(Member {
                               kind: MemberKind::Method,
                               stmt,
                               ..
                           }) = definition else {
        return;
    };

    let Stmt::FunctionDef(ast::StmtFunctionDef {
                              name,
                              returns,
                              ..
                          }) = stmt else {
        return;
    };

    if name != "__iter__" {
        return;
    }

    let Some(returns) = returns else {
        return;
    };

    let annotation = match returns.as_ref() {
        // e.g., Iterable[T]
        Expr::Subscript(ast::ExprSubscript { value, .. }) => value.as_ref(),
        // e.g., typing.Iterable, Iterable
        ann_expr @ (Expr::Name(_) | Expr::Attribute(_)) => ann_expr,
        _ => return,
    };

    if checker
        .semantic_model()
        .resolve_call_path(annotation)
        .map_or(false, |cp| {
            matches!(
                cp.as_slice(),
                &["typing", "Iterable"] | &["collections", "abc", "Iterable"]
            )
        })
    {
        checker
            .diagnostics
            .push(Diagnostic::new(IterMethodReturnIterable, returns.range()));
    }
}
