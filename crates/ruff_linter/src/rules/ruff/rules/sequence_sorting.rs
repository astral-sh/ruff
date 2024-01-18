/// Utilities for sorting constant lists of string literals.
///
/// Examples where these are useful:
/// - Sorting `__all__` in the global scope,
/// - Sorting `__slots__` or `__match_args__` in a class scope
use std::cmp::Ordering;

use ruff_python_ast as ast;
use ruff_python_parser::Tok;
use ruff_source_file::Locator;

use is_macro;
use itertools::{izip, Itertools};

/// An enumeration of the various kinds of sequences for which Python has
/// [display literals](https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries).
///
/// (I'm aware a set isn't actually a "sequence",
/// *but* for our purposes it's conceptually a sequence,
/// since in terms of the AST structure it's almost identical
/// to tuples/lists.)
///
/// Whereas lists, dicts and sets are always parenthesized
/// (e.g. lists always start with `[` and end with `]`),
/// single-line tuples *can* be unparenthesized.
/// We keep the original AST node around for the
/// Tuple variant so that this can be queried later.
#[derive(Debug)]
pub(super) enum SequenceKind<'a> {
    List,
    Set,
    Tuple(&'a ast::ExprTuple),
}

impl SequenceKind<'_> {
    fn surrounding_parens(&self, source: &str) -> (&str, &str) {
        match self {
            Self::List => ("[", "]"),
            Self::Set => ("{", "}"),
            Self::Tuple(ast_node) => {
                if ast_node.is_parenthesized(source) {
                    ("(", ")")
                } else {
                    ("", "")
                }
            }
        }
    }

    pub(super) fn opening_token_for_multiline_definition(&self) -> Tok {
        match self {
            Self::List => Tok::Lsqb,
            Self::Set => Tok::Lbrace,
            Self::Tuple(_) => Tok::Lpar,
        }
    }

    pub(super) fn closing_token_for_multiline_definition(&self) -> Tok {
        match self {
            Self::List => Tok::Rsqb,
            Self::Set => Tok::Rbrace,
            Self::Tuple(_) => Tok::Rpar,
        }
    }
}

/// An enumeration of the various kinds of
/// [display literals](https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries)
/// Python provides for builtin containers.
#[derive(Debug, is_macro::Is)]
pub(super) enum DisplayKind<'a> {
    Sequence(SequenceKind<'a>),
    Dict { values: &'a Vec<ast::Expr> },
}

/// Create a string representing a fixed-up single-line
/// definition of `__all__` or `__slots__` (etc.),
/// that can be inserted into the
/// source code as a `range_replacement` autofix.
pub(super) fn sort_single_line_elements_sequence<F>(
    kind: &SequenceKind,
    elts: &[ast::Expr],
    elements: &[&str],
    locator: &Locator,
    mut cmp_fn: F,
) -> String
where
    F: FnMut(&str, &str) -> Ordering,
{
    assert_eq!(elts.len(), elements.len());
    let (opening_paren, closing_paren) = kind.surrounding_parens(locator.contents());
    let last_item_index = elements.len().saturating_sub(1);
    let mut result = String::from(opening_paren);

    let mut element_pairs = elements.iter().zip(elts).collect_vec();
    element_pairs.sort_by(|(elem1, _), (elem2, _)| cmp_fn(elem1, elem2));
    // We grab the original source-code ranges using `locator.slice()`
    // rather than using the expression generator, as this approach allows
    // us to easily preserve stylistic choices in the original source code
    // such as whether double or single quotes were used.
    for (i, (_, elt)) in element_pairs.iter().enumerate() {
        result.push_str(locator.slice(elt));
        if i < last_item_index {
            result.push_str(", ");
        }
    }

    result.push_str(closing_paren);
    result
}

/// Create a string representing a fixed-up single-line
/// definition of `__all__` or `__slots__` (etc.),
/// that can be inserted into the
/// source code as a `range_replacement` autofix.
pub(super) fn sort_single_line_elements_dict<F>(
    key_elts: &[ast::Expr],
    elements: &[&str],
    value_elts: &[ast::Expr],
    locator: &Locator,
    mut cmp_fn: F,
) -> String
where
    F: FnMut(&str, &str) -> Ordering,
{
    assert!(key_elts.len() == elements.len() && elements.len() == value_elts.len());
    let last_item_index = elements.len().saturating_sub(1);
    let mut result = String::from('{');

    let mut element_trios = izip!(elements, key_elts, value_elts).collect_vec();
    element_trios.sort_by(|(elem1, _, _), (elem2, _, _)| cmp_fn(elem1, elem2));
    // We grab the original source-code ranges using `locator.slice()`
    // rather than using the expression generator, as this approach allows
    // us to easily preserve stylistic choices in the original source code
    // such as whether double or single quotes were used.
    for (i, (_, key, value)) in element_trios.iter().enumerate() {
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

/// An enumeration of the possible conclusions we could come to
/// regarding the ordering of the elements in a display of string literals:
///
/// 1. It's a display of string literals that is already sorted
/// 2. It's an unsorted display of string literals,
///    but we wouldn't be able to autofix it
/// 3. It's an unsorted display of string literals,
///    and it's possible we could generate a fix for it
/// 4. The display contains one or more items that are not string
///    literals.
#[derive(Debug, is_macro::Is)]
pub(super) enum SortClassification<'a> {
    Sorted,
    UnsortedButUnfixable,
    UnsortedAndMaybeFixable { items: Vec<&'a str> },
    NotAListOfStringLiterals,
}

impl<'a> SortClassification<'a> {
    pub(super) fn from_elements<F>(elements: &'a [ast::Expr], mut cmp_fn: F) -> Self
    where
        F: FnMut(&str, &str) -> Ordering,
    {
        let Some((first, rest @ [_, ..])) = elements.split_first() else {
            return Self::Sorted;
        };
        let Some(string_node) = first.as_string_literal_expr() else {
            return Self::NotAListOfStringLiterals;
        };
        let mut this = string_node.value.to_str();

        for expr in rest {
            let Some(string_node) = expr.as_string_literal_expr() else {
                return Self::NotAListOfStringLiterals;
            };
            let next = string_node.value.to_str();
            if cmp_fn(next, this).is_lt() {
                let mut items = Vec::with_capacity(elements.len());
                for expr in elements {
                    let Some(string_node) = expr.as_string_literal_expr() else {
                        return Self::NotAListOfStringLiterals;
                    };
                    if string_node.value.is_implicit_concatenated() {
                        return Self::UnsortedButUnfixable;
                    }
                    items.push(string_node.value.to_str());
                }
                return Self::UnsortedAndMaybeFixable { items };
            }
            this = next;
        }
        Self::Sorted
    }
}
