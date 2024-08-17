use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::map_subscript;
use ruff_text_size::Ranged;

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
/// In Python, an `Iterable` is an object that has an `__iter__` method; an
/// `Iterator` is an object that has `__iter__` and `__next__` methods. All
/// `__iter__` methods are expected to return `Iterator`s. Type checkers may
/// not always recognize an object as being iterable if its `__iter__` method
/// does not return an `Iterator`.
///
/// Every `Iterator` is an `Iterable`, but not every `Iterable` is an `Iterator`.
/// For example, `list` is an `Iterable`, but not an `Iterator`; you can obtain
/// an iterator over a list's elements by passing the list to `iter()`:
///
/// ```pycon
/// >>> import collections.abc
/// >>> x = [42]
/// >>> isinstance(x, collections.abc.Iterable)
/// True
/// >>> isinstance(x, collections.abc.Iterator)
/// False
/// >>> next(x)
/// Traceback (most recent call last):
///  File "<stdin>", line 1, in <module>
/// TypeError: 'list' object is not an iterator
/// >>> y = iter(x)
/// >>> isinstance(y, collections.abc.Iterable)
/// True
/// >>> isinstance(y, collections.abc.Iterator)
/// True
/// >>> next(y)
/// 42
/// ```
///
/// Using `Iterable` rather than `Iterator` as a return type for an `__iter__`
/// methods would imply that you would not necessarily be able to call `next()`
/// on the returned object, violating the expectations of the interface.
///
/// ## Example
///
/// ```python
/// import collections.abc
///
///
/// class Klass:
///     def __iter__(self) -> collections.abc.Iterable[str]: ...
/// ```
///
/// Use instead:
///
/// ```python
/// import collections.abc
///
///
/// class Klass:
///     def __iter__(self) -> collections.abc.Iterator[str]: ...
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
        kind: MemberKind::Method(function),
        ..
    }) = definition
    else {
        return;
    };

    let Some(returns) = function.returns.as_ref() else {
        return;
    };

    let is_async = match function.name.as_str() {
        "__iter__" => false,
        "__aiter__" => true,
        _ => return,
    };

    // Support both `Iterable` and `Iterable[T]`.
    let annotation = map_subscript(returns);

    if checker
        .semantic()
        .resolve_qualified_name(map_subscript(annotation))
        .is_some_and(|qualified_name| {
            if is_async {
                matches!(
                    qualified_name.segments(),
                    ["typing", "AsyncIterable"] | ["collections", "abc", "AsyncIterable"]
                )
            } else {
                matches!(
                    qualified_name.segments(),
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
