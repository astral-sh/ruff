use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{Expr, StmtClassDef};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for classes inheriting from `typing.Generic[]` where `Generic[]` is
/// not the last base class in the bases list.
///
/// ## Why is this bad?
/// `Generic[]` not being the final class in the bases tuple can cause
/// unexpected behaviour at runtime (See [this CPython issue][1] for example).
/// The rule is also applied to stub files, but, unlike at runtime,
/// in stubs it is purely enforced for stylistic consistency.
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
/// Use instead:
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`Generic[]` should always be the last base class")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Move `Generic[]` to the end".to_string())
    }
}

/// PYI059
pub(crate) fn generic_not_last_base_class(checker: &mut Checker, class_def: &StmtClassDef) {
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

    // If Generic[] exists, but is the last base, don't raise issue.
    if generic_base.range() == last_base.range() {
        return;
    }

    let mut diagnostic = Diagnostic::new(GenericNotLastBaseClass, bases.range());

    // No fix if multiple generics are seen in the class bases.
    if generic_base_iter.next().is_none() {
        diagnostic.set_fix(generate_fix(
            generic_base, last_base, checker.locator(),
        ));
    }

    checker.diagnostics.push(diagnostic);
}

fn generate_fix(generic_base: &Expr, last_base: &Expr, locator: &Locator) -> Fix {
    let comma_after_generic_base = generic_base.end().to_usize()
        + locator
            .after(generic_base.end())
            .find(',')
            .expect("Comma must always exist after generic base");

    let last_whitespace = (comma_after_generic_base + 1)
        + locator.contents()[comma_after_generic_base + 1..]
            .bytes()
            .position(|b| !b.is_ascii_whitespace())
            .expect("Non whitespace character must always exist after Generic[]");

    let comma_after_generic_base: u32 = comma_after_generic_base.try_into().unwrap();
    let last_whitespace: u32 = last_whitespace.try_into().unwrap();

    let base_deletion = Edit::deletion(generic_base.start(), generic_base.end());
    let base_comma_deletion =
        Edit::deletion(comma_after_generic_base.into(), last_whitespace.into());
    let insertion = Edit::insertion(
        format!(", {}", locator.slice(generic_base.range())),
        last_base.end(),
    );
    Fix::safe_edits(insertion, [base_deletion, base_comma_deletion])
}
