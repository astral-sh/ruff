use std::borrow::Cow;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_semantic::ScopeKind;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::sequence_sorting::{
    sort_single_line_elements_sequence, MultilineStringSequenceValue, SequenceKind,
    SortClassification, SortingStyle,
};

use itertools::izip;

/// ## What it does
/// Checks for `__slots__` definitions that are not ordered according to a
/// [natural sort](https://en.wikipedia.org/wiki/Natural_sort_order).
///
/// ## Why is this bad?
/// Consistency is good. Use a common convention for this special variable
/// to make your code more readable and idiomatic.
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
}

impl Violation for UnsortedDunderSlots {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`{}.__slots__` is not sorted", self.class_name)
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Apply a natural sort to `{}.__slots__`",
            self.class_name
        ))
    }
}

/// Sort a `__slots__` definition
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

/// Sort a `__slots__` definition
/// represented by a `StmtAnnAssign` AST node.
/// For example: `__slots__: list[str] = ["b", "c", "a"]`.
pub(crate) fn sort_dunder_slots_ann_assign(checker: &mut Checker, node: &ast::StmtAnnAssign) {
    if let Some(value) = &node.value {
        sort_dunder_slots(checker, &node.target, value);
    }
}

const SORTING_STYLE: SortingStyle = SortingStyle::Natural;

/// Sort a tuple, list, dict or set that defines `__slots__` in a class scope.
///
/// This routine checks whether the display is sorted, and emits a
/// violation if it is not sorted. If the tuple/list/set was not sorted,
/// it attempts to set a `Fix` on the violation.
fn sort_dunder_slots(checker: &mut Checker, target: &ast::Expr, node: &ast::Expr) {
    let ast::Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    if id != "__slots__" {
        return;
    }

    // We're only interested in `__slots__` in the class scope
    let ScopeKind::Class(ast::StmtClassDef {
        name: class_name, ..
    }) = checker.semantic().current_scope().kind
    else {
        return;
    };

    let Some(display) = StringLiteralDisplay::new(node) else {
        return;
    };

    let sort_classification = SortClassification::of_elements(&display.elts, SORTING_STYLE);
    if sort_classification.is_not_a_list_of_string_literals() || sort_classification.is_sorted() {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnsortedDunderSlots {
            class_name: class_name.to_string(),
        },
        display.range,
    );

    if let SortClassification::UnsortedAndMaybeFixable { items } = sort_classification {
        if let Some(fix) = display.generate_fix(&items, checker) {
            diagnostic.set_fix(fix);
        }
    }

    checker.diagnostics.push(diagnostic);
}

/// Struct representing a [display](https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries)
/// of string literals.
#[derive(Debug)]
struct StringLiteralDisplay<'a> {
    /// The elts from the original AST node representing the display.
    /// Each elt is the AST representation of a single string literal
    /// element in the display
    elts: Cow<'a, Vec<ast::Expr>>,
    /// The source-code range of the display as a whole
    range: TextRange,
    /// What kind of a display is it? A dict, set, list or tuple?
    display_kind: DisplayKind<'a>,
}

impl Ranged for StringLiteralDisplay<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl<'a> StringLiteralDisplay<'a> {
    fn new(node: &'a ast::Expr) -> Option<Self> {
        let result = match node {
            ast::Expr::List(ast::ExprList { elts, range, .. }) => {
                let display_kind = DisplayKind::Sequence(SequenceKind::List);
                Self {
                    elts: Cow::Borrowed(elts),
                    range: *range,
                    display_kind,
                }
            }
            ast::Expr::Tuple(tuple_node @ ast::ExprTuple { elts, range, .. }) => {
                let display_kind = DisplayKind::Sequence(SequenceKind::Tuple {
                    parenthesized: tuple_node.parenthesized,
                });
                Self {
                    elts: Cow::Borrowed(elts),
                    range: *range,
                    display_kind,
                }
            }
            ast::Expr::Set(ast::ExprSet { elts, range }) => {
                let display_kind = DisplayKind::Sequence(SequenceKind::Set);
                Self {
                    elts: Cow::Borrowed(elts),
                    range: *range,
                    display_kind,
                }
            }
            ast::Expr::Dict(dict @ ast::ExprDict { items, range }) => {
                let mut narrowed_keys = Vec::with_capacity(items.len());
                for key in dict.iter_keys() {
                    if let Some(key) = key {
                        // This is somewhat unfortunate,
                        // *but* using a dict for __slots__ is very rare
                        narrowed_keys.push(key.to_owned());
                    } else {
                        return None;
                    }
                }
                // If `None` was present in the keys, it indicates a "** splat", .e.g
                // `__slots__ = {"foo": "bar", **other_dict}`
                // If `None` wasn't present in the keys,
                // the length of the keys should always equal the length of the values
                assert_eq!(narrowed_keys.len(), items.len());
                Self {
                    elts: Cow::Owned(narrowed_keys),
                    range: *range,
                    display_kind: DisplayKind::Dict { items },
                }
            }
            _ => return None,
        };
        Some(result)
    }

    fn generate_fix(&self, elements: &[&str], checker: &Checker) -> Option<Fix> {
        let locator = checker.locator();
        let is_multiline = locator.contains_line_break(self.range());
        let sorted_source_code = match (&self.display_kind, is_multiline) {
            (DisplayKind::Sequence(sequence_kind), true) => {
                let analyzed_sequence = MultilineStringSequenceValue::from_source_range(
                    self.range(),
                    *sequence_kind,
                    locator,
                    checker.tokens(),
                    elements,
                )?;
                assert_eq!(analyzed_sequence.len(), self.elts.len());
                analyzed_sequence.into_sorted_source_code(SORTING_STYLE, locator, checker.stylist())
            }
            // Sorting multiline dicts is unsupported
            (DisplayKind::Dict { .. }, true) => return None,
            (DisplayKind::Sequence(sequence_kind), false) => sort_single_line_elements_sequence(
                *sequence_kind,
                &self.elts,
                elements,
                locator,
                SORTING_STYLE,
            ),
            (DisplayKind::Dict { items }, false) => {
                sort_single_line_elements_dict(&self.elts, elements, items, locator)
            }
        };
        Some(Fix::safe_edit(Edit::range_replacement(
            sorted_source_code,
            self.range,
        )))
    }
}

/// An enumeration of the various kinds of
/// [display literals](https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries)
/// Python provides for builtin containers.
#[derive(Debug)]
enum DisplayKind<'a> {
    Sequence(SequenceKind),
    Dict { items: &'a [ast::DictItem] },
}

/// A newtype that zips together three iterables:
///
/// 1. The string values of a dict literal's keys;
/// 2. The original AST nodes for the dict literal's keys; and,
/// 3. The original AST nodes for the dict literal's values
///
/// The main purpose of separating this out into a separate struct
/// is to enforce the invariants that:
///
/// 1. The three iterables that are zipped together have the same length; and,
/// 2. The length of all three iterables is >= 2
struct DictElements<'a>(Vec<(&'a &'a str, &'a ast::Expr, &'a ast::Expr)>);

impl<'a> DictElements<'a> {
    fn new(elements: &'a [&str], key_elts: &'a [ast::Expr], items: &'a [ast::DictItem]) -> Self {
        assert_eq!(key_elts.len(), elements.len());
        assert_eq!(elements.len(), items.len());
        assert!(
            elements.len() >= 2,
            "A sequence with < 2 elements cannot be unsorted"
        );
        Self(izip!(elements, key_elts, items.iter().map(|item| &item.value)).collect())
    }

    fn last_item_index(&self) -> usize {
        // Safe from underflow, as the constructor guarantees
        // that the underlying vector has length >= 2
        self.0.len() - 1
    }

    fn into_sorted_elts(mut self) -> impl Iterator<Item = (&'a ast::Expr, &'a ast::Expr)> {
        self.0
            .sort_by(|(elem1, _, _), (elem2, _, _)| SORTING_STYLE.compare(elem1, elem2));
        self.0.into_iter().map(|(_, key, value)| (key, value))
    }
}

/// Create a string representing a fixed-up single-line
/// definition of a `__slots__` dictionary that can be
/// inserted into the source code as a `range_replacement`
/// autofix.
///
/// N.B. This function could potentially be moved into
/// `sequence_sorting.rs` if any other modules need it,
/// but stays here for now, since this is currently the
/// only module that needs it
fn sort_single_line_elements_dict<'a>(
    key_elts: &'a [ast::Expr],
    elements: &'a [&str],
    original_items: &'a [ast::DictItem],
    locator: &Locator,
) -> String {
    let element_trios = DictElements::new(elements, key_elts, original_items);
    let last_item_index = element_trios.last_item_index();
    let mut result = String::from('{');
    // We grab the original source-code ranges using `locator.slice()`
    // rather than using the expression generator, as this approach allows
    // us to easily preserve stylistic choices in the original source code
    // such as whether double or single quotes were used.
    for (i, (key, value)) in element_trios.into_sorted_elts().enumerate() {
        result.push_str(locator.slice(key));
        result.push_str(": ");
        result.push_str(locator.slice(value));
        if i < last_item_index {
            result.push_str(", ");
        }
    }
    result.push('}');
    result
}
