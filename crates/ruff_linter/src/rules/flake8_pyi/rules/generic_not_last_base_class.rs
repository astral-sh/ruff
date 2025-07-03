use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, helpers::map_subscript};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{Parentheses, add_argument, remove_argument};
use crate::{Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for classes inheriting from `typing.Generic[]` where `Generic[]` is
/// not the last base class in the bases tuple.
///
/// ## Why is this bad?
/// If `Generic[]` is not the final class in the bases tuple, unexpected
/// behaviour can occur at runtime (See [this CPython issue][1] for an example).
///
/// The rule is also applied to stub files, where it won't cause issues at
/// runtime. This is because type checkers may not be able to infer an
/// accurate [MRO] for the class, which could lead to unexpected or
/// inaccurate results when they analyze your code.
///
/// For example:
/// ```python
/// from collections.abc import Container, Iterable, Sized
/// from typing import Generic, TypeVar
///
///
/// T = TypeVar("T")
/// K = TypeVar("K")
/// V = TypeVar("V")
///
///
/// class LinkedList(Generic[T], Sized):
///     def push(self, item: T) -> None:
///         self._items.append(item)
///
///
/// class MyMapping(
///     Generic[K, V],
///     Iterable[tuple[K, V]],
///     Container[tuple[K, V]],
/// ):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// from collections.abc import Container, Iterable, Sized
/// from typing import Generic, TypeVar
///
///
/// T = TypeVar("T")
/// K = TypeVar("K")
/// V = TypeVar("V")
///
///
/// class LinkedList(Sized, Generic[T]):
///     def push(self, item: T) -> None:
///         self._items.append(item)
///
///
/// class MyMapping(
///     Iterable[tuple[K, V]],
///     Container[tuple[K, V]],
///     Generic[K, V],
/// ):
///     ...
/// ```
///
/// ## Fix safety
///
/// This rule's fix is always unsafe because reordering base classes can change
/// the behavior of the code by modifying the class's MRO. The fix will also
/// delete trailing comments after the `Generic` base class in multi-line base
/// class lists, if any are present.
///
/// ## Fix availability
///
/// This rule's fix is only available when there are no `*args` present in the base class list.
///
/// ## References
/// - [`typing.Generic` documentation](https://docs.python.org/3/library/typing.html#typing.Generic)
///
/// [1]: https://github.com/python/cpython/issues/106102
/// [MRO]: https://docs.python.org/3/glossary.html#term-method-resolution-order
#[derive(ViolationMetadata)]
pub(crate) struct GenericNotLastBaseClass;

impl Violation for GenericNotLastBaseClass {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`Generic[]` should always be the last base class".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Move `Generic[]` to the end".to_string())
    }
}

/// PYI059
pub(crate) fn generic_not_last_base_class(checker: &Checker, class_def: &ast::StmtClassDef) {
    let Some(bases) = class_def.arguments.as_deref() else {
        return;
    };

    let semantic = checker.semantic();
    if !semantic.seen_typing() {
        return;
    }

    let Some(last_base) = bases.args.last() else {
        return;
    };

    let mut generic_base_iter = bases
        .args
        .iter()
        .filter(|base| semantic.match_typing_expr(map_subscript(base), "Generic"));

    let Some(generic_base) = generic_base_iter.next() else {
        return;
    };

    // If `Generic[]` exists, but is the last base, don't emit a diagnostic.
    if generic_base.range() == last_base.range() {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(GenericNotLastBaseClass, bases.range());

    // Avoid suggesting a fix if any of the arguments is starred. This avoids tricky syntax errors
    // in cases like
    //
    // ```python
    // class C3(Generic[T], metaclass=type, *[str]): ...
    // ```
    //
    // where we would naively try to put `Generic[T]` after `*[str]`, which is also after a keyword
    // argument, causing the error.
    if bases
        .arguments_source_order()
        .any(|arg| arg.value().is_starred_expr())
    {
        return;
    }

    // No fix if multiple `Generic[]`s are seen in the class bases.
    if generic_base_iter.next().is_none() {
        diagnostic.try_set_fix(|| generate_fix(generic_base, bases, checker));
    }
}

fn generate_fix(
    generic_base: &ast::Expr,
    arguments: &ast::Arguments,
    checker: &Checker,
) -> anyhow::Result<Fix> {
    let locator = checker.locator();
    let source = locator.contents();

    let deletion = remove_argument(
        generic_base,
        arguments,
        Parentheses::Preserve,
        source,
        checker.comment_ranges(),
    )?;
    let insertion = add_argument(
        locator.slice(generic_base),
        arguments,
        checker.comment_ranges(),
        source,
    );

    Ok(Fix::unsafe_edits(deletion, [insertion]))
}
