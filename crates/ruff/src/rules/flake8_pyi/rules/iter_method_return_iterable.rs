use ruff_python_ast as ast;
use ruff_python_ast::{Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_python_semantic::{Definition, Member, MemberKind};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `__iter__` methods in stubs that return `Iterable[T]` instead
/// of an `Iterator[T]`.
///
/// ## Why is this bad?
/// `__iter__` methods should always should return an `Iterator` of some kind,
/// not an `Iterable`.
///
/// In Python, an `Iterator` is an object that has a `__next__` method, which
/// provides a consistent interface for sequentially processing elements from
/// a sequence or other iterable object. Meanwhile, an `Iterable` is an object
/// with an `__iter__` method, which itself returns an `Iterator`.
///
/// Every `Iterator` is an `Iterable`, but not every `Iterable` is an `Iterator`.
/// By returning an `Iterable` from `__iter__`, you may end up returning an
/// object that doesn't implement `__next__`, which will cause a `TypeError`
/// at runtime. For example, returning a `list` from `__iter__` will cause
/// a `TypeError` when you call `__next__` on it, as a `list` is an `Iterable`,
/// but not an `Iterator`.
///
/// ## Example
/// ```python
/// import collections.abc
///
///
/// class Class:
///     def __iter__(self) -> collections.abc.Iterable[str]:
///         ...
/// ```
///
/// Use instead:
/// ```python
/// import collections.abc
///
///
/// class Class:
///     def __iter__(self) -> collections.abc.Iterator[str]:
///         ...
/// ```
#[violation]
pub struct IterMethodReturnIterable {
    is_async: bool,
}

impl Violation for IterMethodReturnIterable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let IterMethodReturnIterable { is_async } = self;
        if *is_async {
            format!("`__aiter__` methods should return an `AsyncIterator`, not an `AsyncIterable`")
        } else {
            format!("`__iter__` methods should return an `Iterator`, not an `Iterable`")
        }
    }
}

/// PYI045
pub(crate) fn iter_method_return_iterable(checker: &mut Checker, definition: &Definition) {
    let Definition::Member(Member {
        kind: MemberKind::Method,
        stmt,
        ..
    }) = definition
    else {
        return;
    };

    let Stmt::FunctionDef(ast::StmtFunctionDef { name, returns, .. }) = stmt else {
        return;
    };

    let Some(returns) = returns else {
        return;
    };

    let annotation = if let Expr::Subscript(ast::ExprSubscript { value, .. }) = returns.as_ref() {
        // Ex) `Iterable[T]`
        value
    } else {
        // Ex) `Iterable`, `typing.Iterable`
        returns
    };

    let is_async = match name.as_str() {
        "__iter__" => false,
        "__aiter__" => true,
        _ => return,
    };

    if checker
        .semantic()
        .resolve_call_path(annotation)
        .is_some_and(|call_path| {
            if is_async {
                matches!(
                    call_path.as_slice(),
                    ["typing", "AsyncIterable"] | ["collections", "abc", "AsyncIterable"]
                )
            } else {
                matches!(
                    call_path.as_slice(),
                    ["typing", "Iterable"] | ["collections", "abc", "Iterable"]
                )
            }
        })
    {
        checker.diagnostics.push(Diagnostic::new(
            IterMethodReturnIterable { is_async },
            returns.range(),
        ));
    }
}
