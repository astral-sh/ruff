use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{Arguments, Expr, StmtClassDef};
use ruff_python_semantic::SemanticModel;
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
    let bases = class_def.arguments.as_deref();
    let Some(bases) = bases else {
        return;
    };

    let semantic = checker.semantic();
    if !semantic.seen_typing() {
        return;
    }

    let generic_base_indices: Vec<usize> = bases
        .args
        .iter()
        .enumerate()
        .filter_map(|(base_index, base)| {
            if is_generic(base, semantic) {
                return Some(base_index);
            }
            None
        })
        .collect();

    if generic_base_indices.is_empty() {
        return;
    }

    let diagnostic =
        {
            if generic_base_indices.len() == 1 {
                let base_index = generic_base_indices[0];
                if base_index == bases.args.len() - 1 {
                    // Don't raise issue for the last base.
                    return;
                }

                Diagnostic::new(GenericNotLastBaseClass, class_def.identifier())
                    .with_fix(generate_fix(bases, base_index, checker.locator()))
            } else {
                // No fix if multiple generics are seen in the class bases.
                Diagnostic::new(GenericNotLastBaseClass, class_def.identifier())
            }
        };

    checker.diagnostics.push(diagnostic);
}

/// Return `true` if the given expression resolves to `typing.Generic[...]`.
fn is_generic(expr: &Expr, semantic: &SemanticModel) -> bool {
    semantic.match_typing_expr(map_subscript(expr), "Generic")
}

fn generate_fix(bases: &Arguments, generic_base_index: usize, locator: &Locator) -> Fix {
    let last_base = bases.args.last().expect("Last base should always exist");
    let generic_base = bases
        .args
        .get(generic_base_index)
        .expect("Generic base should always exist");
    let next_base = bases
        .args
        .get(generic_base_index + 1)
        .expect("Generic base should never be the last base during auto-fix");

    let deletion = Edit::deletion(generic_base.start(), next_base.start());
    let insertion = Edit::insertion(
        format!(", {}", locator.slice(generic_base.range())),
        last_base.end(),
    );
    Fix::safe_edits(insertion, [deletion])
}
