use std::borrow::Cow;

use itertools::izip;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_semantic::Binding;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::sequence_sorting::{
    sort_single_line_elements_sequence, CommentComplexity, MultilineStringSequenceValue,
    SequenceKind, SortClassification, SortingStyle,
};
use crate::Locator;

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
///
/// ## Fix safety
/// This rule's fix is marked as unsafe in three situations.
///
/// Firstly, the fix is unsafe if there are any comments that take up
/// a whole line by themselves inside the `__slots__` definition, for example:
/// ```py
/// class Foo:
///     __slots__ = [
///         # eggy things
///         "duck_eggs",
///         "chicken_eggs",
///         # hammy things
///         "country_ham",
///         "parma_ham",
///     ]
/// ```
///
/// This is a common pattern used to delimit categories within a class's slots,
/// but it would be out of the scope of this rule to attempt to maintain these
/// categories when applying a natural sort to the items of `__slots__`.
///
/// Secondly, the fix is also marked as unsafe if there are more than two
/// `__slots__` items on a single line and that line also has a trailing
/// comment, since here it is impossible to accurately gauge which item the
/// comment should be moved with when sorting `__slots__`:
/// ```py
/// class Bar:
///     __slots__ = [
///         "a", "c", "e",  # a comment
///         "b", "d", "f",  # a second  comment
///     ]
/// ```
///
/// Lastly, this rule's fix is marked as unsafe whenever Ruff can detect that
/// code elsewhere in the same file reads the `__slots__` variable in some way
/// and the `__slots__` variable is not assigned to a set. This is because the
/// order of the items in `__slots__` may have semantic significance if the
/// `__slots__` of a class is being iterated over, or being assigned to another
/// value.
///
/// In the vast majority of other cases, this rule's fix is unlikely to
/// cause breakage; as such, Ruff will otherwise mark this rule's fix as
/// safe. However, note that (although it's rare) the value of `__slots__`
/// could still be read by code outside of the module in which the
/// `__slots__` definition occurs, in which case this rule's fix could
/// theoretically cause breakage.
#[derive(ViolationMetadata)]
pub(crate) struct UnsortedDunderSlots {
    class_name: ast::name::Name,
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

const SORTING_STYLE: SortingStyle = SortingStyle::Natural;

/// Sort a tuple, list, dict or set that defines `__slots__` in a class scope.
///
/// This routine checks whether the display is sorted, and emits a
/// violation if it is not sorted. If the tuple/list/set was not sorted,
/// it attempts to set a `Fix` on the violation.
pub(crate) fn sort_dunder_slots(checker: &Checker, binding: &Binding) -> Option<Diagnostic> {
    let semantic = checker.semantic();

    let (target, value) = match binding.statement(semantic)? {
        ast::Stmt::Assign(ast::StmtAssign { targets, value, .. }) => match targets.as_slice() {
            [target] => (target, &**value),
            _ => return None,
        },
        ast::Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. }) => {
            (&**target, value.as_deref()?)
        }
        _ => return None,
    };

    let ast::ExprName { id, .. } = target.as_name_expr()?;

    if id != "__slots__" {
        return None;
    }

    // We're only interested in `__slots__` in the class scope
    let enclosing_class = semantic.scopes[binding.scope].kind.as_class()?;

    // and it has to be an assignment to a "display literal" (a literal dict/set/tuple/list)
    let display = StringLiteralDisplay::new(value)?;

    let sort_classification = SortClassification::of_elements(&display.elts, SORTING_STYLE);
    if sort_classification.is_not_a_list_of_string_literals() || sort_classification.is_sorted() {
        return None;
    }

    let mut diagnostic = Diagnostic::new(
        UnsortedDunderSlots {
            class_name: enclosing_class.name.id.clone(),
        },
        display.range,
    );

    if let SortClassification::UnsortedAndMaybeFixable { items } = sort_classification {
        if let Some((sorted_source_code, comment_complexity)) =
            display.generate_sorted_source_code(&items, checker)
        {
            let edit = Edit::range_replacement(sorted_source_code, display.range());
            let applicability = if comment_complexity.is_complex()
                || (binding.is_used() && !display.kind.is_set_literal())
            {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };
            diagnostic.set_fix(Fix::applicable_edit(edit, applicability));
        }
    }

    Some(diagnostic)
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
    kind: DisplayKind<'a>,
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
                let kind = DisplayKind::Sequence(SequenceKind::List);
                Self {
                    elts: Cow::Borrowed(elts),
                    range: *range,
                    kind,
                }
            }
            ast::Expr::Tuple(ast::ExprTuple {
                elts,
                range,
                parenthesized,
                ..
            }) => {
                let kind = DisplayKind::Sequence(SequenceKind::Tuple {
                    parenthesized: *parenthesized,
                });
                Self {
                    elts: Cow::Borrowed(elts),
                    range: *range,
                    kind,
                }
            }
            ast::Expr::Set(ast::ExprSet { elts, range }) => {
                let kind = DisplayKind::Sequence(SequenceKind::Set);
                Self {
                    elts: Cow::Borrowed(elts),
                    range: *range,
                    kind,
                }
            }
            ast::Expr::Dict(dict) => {
                let mut narrowed_keys = Vec::with_capacity(dict.len());
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
                assert_eq!(narrowed_keys.len(), dict.len());
                Self {
                    elts: Cow::Owned(narrowed_keys),
                    range: dict.range(),
                    kind: DisplayKind::Dict { items: &dict.items },
                }
            }
            _ => return None,
        };
        Some(result)
    }

    fn generate_sorted_source_code(
        &self,
        elements: &[&str],
        checker: &Checker,
    ) -> Option<(String, CommentComplexity)> {
        let locator = checker.locator();

        let multiline_classification = if locator.contains_line_break(self.range()) {
            MultilineClassification::Multiline
        } else {
            MultilineClassification::SingleLine
        };

        match (&self.kind, multiline_classification) {
            (DisplayKind::Sequence(sequence_kind), MultilineClassification::Multiline) => {
                let analyzed_sequence = MultilineStringSequenceValue::from_source_range(
                    self.range(),
                    *sequence_kind,
                    locator,
                    checker.tokens(),
                    elements,
                )?;
                assert_eq!(analyzed_sequence.len(), self.elts.len());
                let comment_complexity = analyzed_sequence.comment_complexity();
                let sorted_code = analyzed_sequence.into_sorted_source_code(
                    SORTING_STYLE,
                    locator,
                    checker.stylist(),
                );
                Some((sorted_code, comment_complexity))
            }
            // Sorting multiline dicts is unsupported
            (DisplayKind::Dict { .. }, MultilineClassification::Multiline) => None,
            (DisplayKind::Sequence(sequence_kind), MultilineClassification::SingleLine) => {
                let sorted_code = sort_single_line_elements_sequence(
                    *sequence_kind,
                    &self.elts,
                    elements,
                    locator,
                    SORTING_STYLE,
                );
                Some((sorted_code, CommentComplexity::Simple))
            }
            (DisplayKind::Dict { items }, MultilineClassification::SingleLine) => {
                let sorted_code =
                    sort_single_line_elements_dict(&self.elts, elements, items, locator);
                Some((sorted_code, CommentComplexity::Simple))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MultilineClassification {
    SingleLine,
    Multiline,
}

/// An enumeration of the various kinds of
/// [display literals](https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries)
/// Python provides for builtin containers.
#[derive(Debug, Copy, Clone)]
enum DisplayKind<'a> {
    Sequence(SequenceKind),
    Dict { items: &'a [ast::DictItem] },
}

impl DisplayKind<'_> {
    const fn is_set_literal(self) -> bool {
        matches!(self, Self::Sequence(SequenceKind::Set))
    }
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
