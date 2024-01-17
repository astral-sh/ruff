use std::fmt::Display;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::ScopeKind;
use ruff_source_file::Locator;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::sorting_helpers::{
    sort_single_line_elements_dict, sort_single_line_elements_sequence, DisplayKind, SequenceKind,
    SortClassification,
};

use natord;

/// ## What it does
/// Checks for `__slots__` and `__match_args__`
/// definitions that are not ordered according to a
/// [natural sort](https://en.wikipedia.org/wiki/Natural_sort_order).
///
/// ## Why is this bad?
/// Consistency is good. Use a common convention for
/// these special variables to make your code more
/// readable and idiomatic.
///
/// ## Example
/// ```python
/// class Dog:
///     __slots__ = "name", "breed"
/// ```
///
/// Use instead:
/// ```python
/// class Dog:
///     __slots__ = "breed", "name"
/// ```
#[violation]
pub struct UnsortedDunderSlots {
    class_name: String,
    class_variable: SpecialClassDunder,
}

impl Violation for UnsortedDunderSlots {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnsortedDunderSlots {
            class_name,
            class_variable,
        } = self;
        format!("`{class_name}.{class_variable}` is not sorted")
    }

    fn fix_title(&self) -> Option<String> {
        let UnsortedDunderSlots {
            class_name,
            class_variable,
        } = self;
        Some(format!(
            "Apply a natural sort to `{class_name}.{class_variable}`"
        ))
    }
}

/// Enumeration of the two special class dunders
/// that we're interested in for this rule: `__match_args__` and `__slots__`
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
enum SpecialClassDunder {
    Slots,
    MatchArgs,
}

impl Display for SpecialClassDunder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let string = match self {
            Self::MatchArgs => "__match_args__",
            Self::Slots => "__slots__",
        };
        write!(f, "{string}")
    }
}

/// Sort a `__slots__`/`__match_args__` definition
/// represented by a `StmtAssign` AST node.
/// For example: `__slots__ = ["b", "c", "a"]`.
pub(crate) fn sort_dunder_slots_assign(
    checker: &mut Checker,
    ast::StmtAssign { value, targets, .. }: &ast::StmtAssign,
) {
    if let [expr] = targets.as_slice() {
        sort_dunder_slots(checker, expr, value);
    }
}

/// Sort a `__slots__`/`__match_args__` definition
/// represented by a `StmtAnnAssign` AST node.
/// For example: `__slots__: list[str] = ["b", "c", "a"]`.
pub(crate) fn sort_dunder_slots_ann_assign(checker: &mut Checker, node: &ast::StmtAnnAssign) {
    if let Some(value) = &node.value {
        sort_dunder_slots(checker, &node.target, value);
    }
}

/// Sort a tuple, list, dict or set that defines `__slots__`
/// or `__match_args__` in a class scope.
///
/// This routine checks whether the display is sorted, and emits a
/// violation if it is not sorted. If the tuple/list/set was not sorted,
/// it attempts to set a `Fix` on the violation.
fn sort_dunder_slots(checker: &mut Checker, target: &ast::Expr, node: &ast::Expr) {
    let ast::Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    let dunder_kind = match id.as_str() {
        "__slots__" => SpecialClassDunder::Slots,
        "__match_args__" => SpecialClassDunder::MatchArgs,
        _ => return,
    };

    // We're only interested in `__slots__`/`__match_args__` in the class scope
    let ScopeKind::Class(ast::StmtClassDef {
        name: class_name, ..
    }) = checker.semantic().current_scope().kind
    else {
        return;
    };

    let Some((elts, range, display_kind)) = extract_elts(dunder_kind, node) else {
        return;
    };

    let elts_analysis = SortClassification::from_elements(&elts, natord::compare);
    if elts_analysis.is_not_a_list_of_string_literals() || elts_analysis.is_sorted() {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnsortedDunderSlots {
            class_name: class_name.to_string(),
            class_variable: dunder_kind,
        },
        range,
    );

    if let SortClassification::UnsortedAndMaybeFixable { items } = elts_analysis {
        let locator = checker.locator();
        if !locator.contains_line_break(range) {
            diagnostic.set_fix(create_fix(display_kind, &elts, &items, range, locator));
        }
    }

    checker.diagnostics.push(diagnostic);
}

fn extract_elts(
    dunder_kind: SpecialClassDunder,
    node: &ast::Expr,
) -> Option<(Vec<&ast::Expr>, TextRange, DisplayKind<'_>)> {
    let result = match (dunder_kind, node) {
        (_, ast::Expr::List(ast::ExprList { elts, range, .. })) => (
            elts.iter().collect(),
            *range,
            DisplayKind::Sequence(SequenceKind::List),
        ),
        (_, ast::Expr::Tuple(tuple_node @ ast::ExprTuple { elts, range, .. })) => {
            let display_kind = DisplayKind::Sequence(SequenceKind::Tuple(tuple_node));
            (elts.iter().collect(), *range, display_kind)
        }
        (SpecialClassDunder::Slots, ast::Expr::Set(ast::ExprSet { elts, range })) => (
            elts.iter().collect(),
            *range,
            DisplayKind::Sequence(SequenceKind::Set),
        ),
        (
            SpecialClassDunder::Slots,
            ast::Expr::Dict(ast::ExprDict {
                keys,
                values,
                range,
            }),
        ) => {
            let mut narrowed_keys = Vec::with_capacity(keys.len());
            for key in keys {
                if let Some(key) = key {
                    narrowed_keys.push(key);
                } else {
                    return None;
                }
            }
            // If `None` was present in the keys, it indicates a "** splat", .e.g
            // `__slots__ = {"foo": "bar", **other_dict}`
            // If `None` wasn't present in the keys,
            // the length of the keys should always equal the length of the values
            assert_eq!(narrowed_keys.len(), values.len());
            (narrowed_keys, *range, DisplayKind::Dict { values })
        }
        _ => return None,
    };
    Some(result)
}

fn create_fix(
    display_kind: DisplayKind<'_>,
    elts: &[&ast::Expr],
    items: &[&str],
    range: TextRange,
    locator: &Locator,
) -> Fix {
    let new_var = match display_kind {
        DisplayKind::Dict { values } => {
            sort_single_line_elements_dict(elts, items, values, locator, natord::compare)
        }
        DisplayKind::Sequence(sequence_kind) => sort_single_line_elements_sequence(
            &sequence_kind,
            elts,
            items,
            locator,
            natord::compare,
        ),
    };
    Fix::safe_edit(Edit::range_replacement(new_var, range))
}
