use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, Arguments, Expr, StmtClassDef};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for classes inheriting from `typing.Generic[]`, but `Generic[]` is
/// not the last base class in the bases list.
///
/// ## Why is this bad?
/// `Generic[]` not being the final class in the bases tuple can cause
/// unexpected behaviour at runtime (See [this CPython issue][1] for example).
/// In a stub file, however, this rule is enforced purely for stylistic
/// consistency.
///
/// For example:
/// ```python
/// class LinkedList(Generic[T], Sized):
///     def push(self, item: T) -> None:
///         self._items.append(item)
///
/// class MyMapping(
///     Generic[K, V],
///     Iterable[Tuple[K, V]],
///     Container[Tuple[K, V]],
/// ):
///     ...
/// ```
///
/// ```python
/// class LinkedList(Sized, Generic[T]):
///     def push(self, item: T) -> None:
///         self._items.append(item)
///
/// class MyMapping(
///     Iterable[Tuple[K, V]],
///     Container[Tuple[K, V]],
///     Generic[K, V],
/// ):
///     ...
/// ```
/// ## References
/// - [`typing.Generic` documentation](https://docs.python.org/3/library/typing.html#typing.Generic)
///
/// [1]: https://github.com/python/cpython/issues/106102
#[violation]
pub struct GenericNotLastBaseClass;

impl Violation for GenericNotLastBaseClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`Generic[]` should always be the last base class")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Move `Generic[]` to be the last base class".to_string())
    }
}

/// PYI059
pub(crate) fn generic_not_last_base_class(
    checker: &mut Checker,
    class_def: &StmtClassDef,
    arguments: Option<&Arguments>,
) {
    let semantic = checker.semantic();

    if arguments.is_some_and(|arguments| {
        arguments.args.iter().enumerate().any(|(index, base)| {
            if index == arguments.args.len() - 1 {
                return false;
            }
            is_generic(base, semantic)
        })
    }) {
        checker.diagnostics.push(Diagnostic::new(
            GenericNotLastBaseClass,
            class_def.identifier(),
        ));
    }
}

/// Return `true` if the given expression resolves to `typing.Generic[...]`.
fn is_generic(expr: &Expr, semantic: &SemanticModel) -> bool {
    if !semantic.seen_typing() {
        return false;
    }

    let Expr::Subscript(ast::ExprSubscript { value, .. }) = expr else {
        return false;
    };

    let qualified_name = semantic.resolve_qualified_name(value);
    qualified_name.as_ref().is_some_and(|qualified_name| {
        semantic.match_typing_qualified_name(qualified_name, "Generic")
    })
}
