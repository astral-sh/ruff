/// Utilities for sorting constant lists of string literals.
///
/// Examples where these are useful:
/// - Sorting `__all__` in the global scope,
/// - Sorting `__slots__` in a class scope
use std::borrow::Cow;
use std::cmp::Ordering;

use ruff_python_ast as ast;
use ruff_python_codegen::Stylist;
use ruff_python_parser::{TokenKind, Tokens};
use ruff_python_stdlib::str::is_cased_uppercase;
use ruff_python_trivia::{first_non_trivia_token, leading_indentation, SimpleTokenKind};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use is_macro;
use natord;

/// An enumeration of the different sorting styles
/// currently supported for displays of string literals
#[derive(Debug, Clone, Copy)]
pub(super) enum SortingStyle {
    /// Sort string-literal items according to a
    /// [natural sort](https://en.wikipedia.org/wiki/Natural_sort_order).
    Natural,
    /// Sort string-literal items "isort-style".
    ///
    /// An isort-style sort orders items first according to their casing:
    /// SCREAMING_SNAKE_CASE names (conventionally used for global constants)
    /// come first, followed by CamelCase names (conventionally used for
    /// classes), followed by anything else. Within each category,
    /// a [natural sort](https://en.wikipedia.org/wiki/Natural_sort_order)
    /// is used to order the elements.
    Isort,
}

impl SortingStyle {
    pub(super) fn compare(self, a: &str, b: &str) -> Ordering {
        match self {
            Self::Natural => natord::compare(a, b),
            Self::Isort => IsortSortKey::from(a).cmp(&IsortSortKey::from(b)),
        }
    }
}

/// A struct to implement logic necessary to achieve
/// an "isort-style sort".
///
/// An isort-style sort sorts items first according to their casing:
/// SCREAMING_SNAKE_CASE names (conventionally used for global constants)
/// come first, followed by CamelCase names (conventionally used for
/// classes), followed by anything else. Within each category,
/// a [natural sort](https://en.wikipedia.org/wiki/Natural_sort_order)
/// is used to order the elements.
struct IsortSortKey<'a> {
    category: InferredMemberType,
    value: &'a str,
}

impl Ord for IsortSortKey<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.category
            .cmp(&other.category)
            .then_with(|| natord::compare(self.value, other.value))
    }
}

impl PartialOrd for IsortSortKey<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for IsortSortKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for IsortSortKey<'_> {}

impl<'a> From<&'a str> for IsortSortKey<'a> {
    fn from(value: &'a str) -> Self {
        Self {
            category: InferredMemberType::of(value),
            value,
        }
    }
}

/// Classification for the casing of an element in a
/// sequence of literal strings.
///
/// This is necessary to achieve an "isort-style" sort,
/// where elements are sorted first by category,
/// then, within categories, are sorted according
/// to a natural sort.
///
/// You'll notice that a very similar enum exists
/// in ruff's reimplementation of isort.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone, Copy)]
enum InferredMemberType {
    Constant,
    Class,
    Other,
}

impl InferredMemberType {
    fn of(value: &str) -> Self {
        // E.g. `CONSTANT`
        if value.len() > 1 && is_cased_uppercase(value) {
            Self::Constant
        // E.g. `Class`
        } else if value.starts_with(char::is_uppercase) {
            Self::Class
        // E.g. `some_variable` or `some_function`
        } else {
            Self::Other
        }
    }
}

/// An enumeration of the various kinds of sequences for which Python has
/// [display literals](https://docs.python.org/3/reference/expressions.html#displays-for-lists-sets-and-dictionaries).
///
/// (I'm aware a set isn't actually a "sequence",
/// *but* for our purposes it's conceptually a sequence,
/// since in terms of the AST structure it's almost identical
/// to tuples/lists.)
///
/// Whereas list, dict and set literals are always parenthesized
/// (e.g. lists always start with `[` and end with `]`),
/// single-line tuple literals *can* be unparenthesized.
/// We keep the original AST node around for the
/// Tuple variant so that this can be queried later.
#[derive(Copy, Clone, Debug)]
pub(super) enum SequenceKind {
    List,
    Set,
    Tuple { parenthesized: bool },
}

impl SequenceKind {
    // N.B. We only need the source code for the Tuple variant here,
    // but if you already have a `Locator` instance handy,
    // getting the source code is very cheap.
    fn surrounding_brackets(self) -> (&'static str, &'static str) {
        match self {
            Self::List => ("[", "]"),
            Self::Set => ("{", "}"),
            Self::Tuple { parenthesized } => {
                if parenthesized {
                    ("(", ")")
                } else {
                    ("", "")
                }
            }
        }
    }

    const fn opening_token_for_multiline_definition(self) -> TokenKind {
        match self {
            Self::List => TokenKind::Lsqb,
            Self::Set => TokenKind::Lbrace,
            Self::Tuple { .. } => TokenKind::Lpar,
        }
    }

    const fn closing_token_for_multiline_definition(self) -> TokenKind {
        match self {
            Self::List => TokenKind::Rsqb,
            Self::Set => TokenKind::Rbrace,
            Self::Tuple { .. } => TokenKind::Rpar,
        }
    }
}

/// A newtype that zips together the string values of a display literal's elts,
/// together with the original AST nodes for that display literal's elts.
///
/// The main purpose of separating this out into a separate struct
/// is to enforce the invariants that:
///
/// 1. The two iterables that are zipped together have the same length; and,
/// 2. The length of both iterables is >= 2
struct SequenceElements<'a>(Vec<(&'a &'a str, &'a ast::Expr)>);

impl<'a> SequenceElements<'a> {
    fn new(elements: &'a [&str], elts: &'a [ast::Expr]) -> Self {
        assert_eq!(elements.len(), elts.len());
        assert!(
            elements.len() >= 2,
            "A sequence with < 2 elements cannot be unsorted"
        );
        Self(elements.iter().zip(elts).collect())
    }

    fn last_item_index(&self) -> usize {
        // Safe from underflow, as the constructor guarantees
        // that the underlying vector has length >= 2
        self.0.len() - 1
    }

    fn into_sorted_elts(
        mut self,
        sorting_style: SortingStyle,
    ) -> impl Iterator<Item = &'a ast::Expr> {
        self.0
            .sort_by(|(elem1, _), (elem2, _)| sorting_style.compare(elem1, elem2));
        self.0.into_iter().map(|(_, elt)| elt)
    }
}

/// Create a string representing a fixed-up single-line
/// definition of `__all__` or `__slots__` (etc.),
/// that can be inserted into the
/// source code as a `range_replacement` autofix.
pub(super) fn sort_single_line_elements_sequence(
    kind: SequenceKind,
    elts: &[ast::Expr],
    elements: &[&str],
    locator: &Locator,
    sorting_style: SortingStyle,
) -> String {
    let element_pairs = SequenceElements::new(elements, elts);
    let last_item_index = element_pairs.last_item_index();
    let (opening_paren, closing_paren) = kind.surrounding_brackets();
    let mut result = String::from(opening_paren);
    // We grab the original source-code ranges using `locator.slice()`
    // rather than using the expression generator, as this approach allows
    // us to easily preserve stylistic choices in the original source code
    // such as whether double or single quotes were used.
    for (i, elt) in element_pairs.into_sorted_elts(sorting_style).enumerate() {
        result.push_str(locator.slice(elt));
        if i < last_item_index {
            result.push_str(", ");
        }
    }
    result.push_str(closing_paren);
    result
}

/// An enumeration of the possible conclusions we could come to
/// regarding the ordering of the elements in a display of string literals
#[derive(Debug, is_macro::Is)]
pub(super) enum SortClassification<'a> {
    /// It's a display of string literals that is already sorted
    Sorted,
    /// It's an unsorted display of string literals,
    /// but we wouldn't be able to autofix it
    UnsortedButUnfixable,
    /// It's an unsorted display of string literals,
    /// and it's possible we could generate a fix for it;
    /// here's the values of the elts so we can use them to
    /// generate an autofix:
    UnsortedAndMaybeFixable { items: Vec<&'a str> },
    /// The display contains one or more items that are not string
    /// literals.
    NotAListOfStringLiterals,
}

impl<'a> SortClassification<'a> {
    pub(super) fn of_elements(elements: &'a [ast::Expr], sorting_style: SortingStyle) -> Self {
        // If it's of length less than 2, it has to be sorted already
        let Some((first, rest @ [_, ..])) = elements.split_first() else {
            return Self::Sorted;
        };

        // If any elt we encounter is not an ExprStringLiteral AST node,
        // that indicates at least one item in the sequence is not a string literal,
        // which means the sequence is out of scope for RUF022/RUF023/etc.
        let Some(string_node) = first.as_string_literal_expr() else {
            return Self::NotAListOfStringLiterals;
        };
        let mut current = string_node.value.to_str();

        for expr in rest {
            let Some(string_node) = expr.as_string_literal_expr() else {
                return Self::NotAListOfStringLiterals;
            };
            let next = string_node.value.to_str();
            if sorting_style.compare(next, current).is_lt() {
                // Looks like the sequence was not in fact already sorted!
                //
                // Now we need to gather the necessary information we'd need
                // to create an autofix. We need to know three things for this:
                //
                // 1. Are all items in the sequence string literals?
                //    (If not, we won't even be emitting the violation, let alone
                //    trying to fix it.)
                // 2. Are any items in the sequence implicitly concatenated?
                //    (If so, we might be *emitting* the violation, but we definitely
                //    won't be trying to fix it.)
                // 3. What is the value of each elt in the sequence?
                let mut items = Vec::with_capacity(elements.len());
                let mut any_implicit_concatenation = false;
                for expr in elements {
                    let Some(string_node) = expr.as_string_literal_expr() else {
                        return Self::NotAListOfStringLiterals;
                    };
                    any_implicit_concatenation |= string_node.value.is_implicit_concatenated();
                    items.push(string_node.value.to_str());
                }
                if any_implicit_concatenation {
                    return Self::UnsortedButUnfixable;
                }
                return Self::UnsortedAndMaybeFixable { items };
            }
            current = next;
        }
        // Looks like the sequence was already sorted -- hooray!
        // We won't be emitting a violation this time.
        Self::Sorted
    }
}

// An instance of this struct encapsulates an analysis
/// of a multiline Python tuple/list that represents an
/// `__all__`/`__slots__`/etc. definition or augmentation.
pub(super) struct MultilineStringSequenceValue<'a> {
    items: Vec<StringSequenceItem<'a>>,
    range: TextRange,
    ends_with_trailing_comma: bool,
}

impl<'a> MultilineStringSequenceValue<'a> {
    pub(super) fn len(&self) -> usize {
        self.items.len()
    }

    /// Analyse the source range for a multiline Python tuple/list that
    /// represents an `__all__`/`__slots__`/etc. definition or augmentation.
    /// Return `None` if the analysis fails for whatever reason.
    pub(super) fn from_source_range(
        range: TextRange,
        kind: SequenceKind,
        locator: &Locator,
        tokens: &Tokens,
        string_items: &[&'a str],
    ) -> Option<MultilineStringSequenceValue<'a>> {
        // Parse the multiline string sequence using the raw tokens.
        // See the docs for `collect_string_sequence_lines()` for why we have to
        // use the raw tokens, rather than just the AST, to do this parsing.
        //
        // Step (1). Start by collecting information on each line individually:
        let (lines, ends_with_trailing_comma) =
            collect_string_sequence_lines(range, kind, tokens, string_items)?;

        // Step (2). Group lines together into sortable "items":
        //   - Any "item" contains a single element of the list/tuple
        //   - Assume that any comments on their own line are meant to be grouped
        //     with the element immediately below them: if the element moves,
        //     the comments above the element move with it.
        //   - The same goes for any comments on the same line as an element:
        //     if the element moves, the comment moves with it.
        let items = collect_string_sequence_items(lines, range, locator);

        Some(MultilineStringSequenceValue {
            items,
            range,
            ends_with_trailing_comma,
        })
    }

    /// Sort a multiline sequence of literal strings
    /// that is known to be unsorted.
    ///
    /// This function panics if it is called and `self.items`
    /// has length < 2. It's redundant to call this method in this case,
    /// since lists with < 2 items cannot be unsorted,
    /// so this is a logic error.
    pub(super) fn into_sorted_source_code(
        mut self,
        sorting_style: SortingStyle,
        locator: &Locator,
        stylist: &Stylist,
    ) -> String {
        let (first_item_start, last_item_end) = match self.items.as_slice() {
            [first_item, .., last_item] => (first_item.start(), last_item.end()),
            _ => panic!(
                "We shouldn't be attempting an autofix if a sequence has < 2 elements;
                a sequence with 1 or 0 elements cannot be unsorted."
            ),
        };

        // As well as the "items" in a multiline string sequence,
        // there is also a "prelude" and a "postlude":
        //  - Prelude == the region of source code from the opening parenthesis,
        //    up to the start of the first item in `__all__`/`__slots__`/etc.
        //  - Postlude == the region of source code from the end of the last
        //    item in `__all__`/`__slots__`/etc. up to and including the closing
        //    parenthesis.
        //
        // For example:
        //
        // ```python
        // __all__ = [  # comment0
        //   # comment1
        //   "first item",
        //   "last item"  # comment2
        //   # comment3
        // ]  # comment4
        // ```
        //
        // - The prelude in the above example is the source code region
        //   starting just before the opening `[` and ending just after `# comment0`.
        //   `comment0` here counts as part of the prelude because it is on
        //   the same line as the opening paren, and because we haven't encountered
        //   any elements of `__all__` yet, but `comment1` counts as part of the first item,
        //   as it's on its own line, and all comments on their own line are grouped
        //   with the next element below them to make "items",
        //   (an "item" being a region of source code that all moves as one unit
        //   when `__all__` is sorted).
        // - The postlude in the above example is the source code region starting
        //   just after `# comment2` and ending just after the closing paren.
        //   `# comment2` is part of the last item, as it's an inline comment on the
        //   same line as an element, but `# comment3` becomes part of the postlude
        //   because there are no items below it. `# comment4` is not part of the
        //   postlude: it's outside of the source-code range considered by this rule,
        //   and should therefore be untouched.
        //
        let newline = stylist.line_ending().as_str();
        let start_offset = self.start();
        let leading_indent = leading_indentation(locator.full_line(start_offset));
        let item_indent = format!("{}{}", leading_indent, stylist.indentation().as_str());

        let prelude =
            multiline_string_sequence_prelude(first_item_start, newline, start_offset, locator);
        let postlude = multiline_string_sequence_postlude(
            last_item_end,
            newline,
            leading_indent,
            &item_indent,
            self.end(),
            locator,
        );

        // We only add a trailing comma to the last item in the sequence
        // as part of `join_multiline_string_sequence_items()`
        // if both the following are true:
        //
        // (1) The last item in the original sequence had a trailing comma; AND,
        // (2) The first "semantically significant" token in the postlude is not a comma
        //     (if the first semantically significant token *is* a comma, and we add another comma,
        //     we'll end up with two commas after the final item, which would be invalid syntax)
        let needs_trailing_comma = self.ends_with_trailing_comma
            && first_non_trivia_token(TextSize::new(0), &postlude)
                .map_or(true, |tok| tok.kind() != SimpleTokenKind::Comma);

        self.items
            .sort_by(|a, b| sorting_style.compare(a.value, b.value));
        let joined_items = join_multiline_string_sequence_items(
            &self.items,
            locator,
            &item_indent,
            newline,
            needs_trailing_comma,
        );

        format!("{prelude}{joined_items}{postlude}")
    }
}

impl Ranged for MultilineStringSequenceValue<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Collect data on each line of a multiline string sequence.
/// Return `None` if the sequence appears to be invalid,
/// or if it's an edge case we don't support.
///
/// Why do we need to do this using the raw tokens,
/// when we already have the AST? The AST strips out
/// crucial information that we need to track here for
/// a multiline string sequence, such as:
/// - The value of comments
/// - The amount of whitespace between the end of a line
///   and an inline comment
/// - Whether or not the final item in the tuple/list has a
///   trailing comma
///
/// All of this information is necessary to have at a later
/// stage if we're to sort items without doing unnecessary
/// brutality to the comments and pre-existing style choices
/// in the original source code.
fn collect_string_sequence_lines<'a>(
    range: TextRange,
    kind: SequenceKind,
    tokens: &Tokens,
    string_items: &[&'a str],
) -> Option<(Vec<StringSequenceLine<'a>>, bool)> {
    // These first two variables are used for keeping track of state
    // regarding the entirety of the string sequence...
    let mut ends_with_trailing_comma = false;
    let mut lines = vec![];
    // ... all state regarding a single line of a string sequence
    // is encapsulated in this variable
    let mut line_state = LineState::default();
    // An iterator over the string values in the sequence.
    let mut string_items_iter = string_items.iter();

    let mut token_iter = tokens.in_range(range).iter();
    let first_token = token_iter.next()?;
    if first_token.kind() != kind.opening_token_for_multiline_definition() {
        return None;
    }
    let expected_final_token = kind.closing_token_for_multiline_definition();

    for token in token_iter {
        match token.kind() {
            TokenKind::NonLogicalNewline => {
                lines.push(line_state.into_string_sequence_line());
                line_state = LineState::default();
            }
            TokenKind::Comment => {
                line_state.visit_comment_token(token.range());
            }
            TokenKind::String => {
                let Some(string_value) = string_items_iter.next() else {
                    unreachable!("Expected the number of string tokens to be equal to the number of string items in the sequence");
                };
                line_state.visit_string_token(string_value, token.range());
                ends_with_trailing_comma = false;
            }
            TokenKind::Comma => {
                line_state.visit_comma_token(token.range());
                ends_with_trailing_comma = true;
            }
            kind if kind == expected_final_token => {
                lines.push(line_state.into_string_sequence_line());
                break;
            }
            _ => return None,
        }
    }
    Some((lines, ends_with_trailing_comma))
}

/// This struct is for keeping track of state
/// regarding a single line in a multiline string sequence
/// It is purely internal to `collect_string_sequence_lines()`,
/// and should not be used outside that function.
///
/// There are three possible kinds of line in a multiline
/// string sequence, and we don't know what kind of a line
/// we're in until all tokens in that line have been processed:
///
/// - A line with just a comment
///   (`StringSequenceLine::JustAComment)`)
/// - A line with one or more string items in it
///   (`StringSequenceLine::OneOrMoreItems`)
/// - An empty line (`StringSequenceLine::Empty`)
///
/// As we process the tokens in a single line,
/// this struct accumulates the necessary state for us
/// to be able to determine what kind of a line we're in.
/// Once the entire line has been processed,
/// `into_string_sequence_line()` is called, which consumes
/// `self` and produces the classification for the line.
#[derive(Debug, Default)]
struct LineState<'a> {
    first_item_in_line: Option<(&'a str, TextRange)>,
    following_items_in_line: Vec<(&'a str, TextRange)>,
    comment_range_start: Option<TextSize>,
    comment_in_line: Option<TextRange>,
}

impl<'a> LineState<'a> {
    fn visit_string_token(&mut self, token_value: &'a str, token_range: TextRange) {
        if self.first_item_in_line.is_none() {
            self.first_item_in_line = Some((token_value, token_range));
        } else {
            self.following_items_in_line
                .push((token_value, token_range));
        }
        self.comment_range_start = Some(token_range.end());
    }

    fn visit_comma_token(&mut self, token_range: TextRange) {
        self.comment_range_start = Some(token_range.end());
    }

    /// If this is a comment on its own line,
    /// record the range of that comment.
    ///
    /// *If*, however, we've already seen a comma
    /// or a string in this line, that means that we're
    /// in a line with items. In that case, we want to
    /// record the range of the comment, *plus* the whitespace
    /// (if any) preceding the comment. This is so that we don't
    /// unnecessarily apply opinionated formatting changes
    /// where they might not be welcome.
    fn visit_comment_token(&mut self, token_range: TextRange) {
        self.comment_in_line = {
            if let Some(comment_range_start) = self.comment_range_start {
                Some(TextRange::new(comment_range_start, token_range.end()))
            } else {
                Some(token_range)
            }
        }
    }

    fn into_string_sequence_line(self) -> StringSequenceLine<'a> {
        if let Some(first_item) = self.first_item_in_line {
            StringSequenceLine::OneOrMoreItems(LineWithItems {
                first_item,
                following_items: self.following_items_in_line,
                trailing_comment_range: self.comment_in_line,
            })
        } else {
            self.comment_in_line
                .map_or(StringSequenceLine::Empty, |comment_range| {
                    StringSequenceLine::JustAComment(LineWithJustAComment(comment_range))
                })
        }
    }
}

/// Instances of this struct represent source-code lines in the middle
/// of multiline tuples/lists/sets where the line contains
/// 0 elements of the sequence, but the line does have a comment in it.
#[derive(Debug)]
struct LineWithJustAComment(TextRange);

/// Instances of this struct represent source-code lines in
/// multiline tuples/lists/sets where the line contains at least
/// 1 element of the sequence. The line may contain > 1 element of the
/// sequence, and may also have a trailing comment after the element(s).
#[derive(Debug)]
struct LineWithItems<'a> {
    // For elements in the list, we keep track of the value of the
    // value of the element as well as the source-code range of the element.
    // (We need to know the actual value so that we can sort the items.)
    first_item: (&'a str, TextRange),
    following_items: Vec<(&'a str, TextRange)>,
    // For comments, we only need to keep track of the source-code range.
    trailing_comment_range: Option<TextRange>,
}

impl LineWithItems<'_> {
    fn num_items(&self) -> usize {
        self.following_items.len() + 1
    }
}

/// An enumeration of the possible kinds of source-code lines
/// that can exist in a multiline string sequence:
///
/// - A line that has no string elements, but does have a comment.
/// - A line that has one or more string elements,
///   and may also have a trailing comment.
/// - An entirely empty line.
#[derive(Debug)]
enum StringSequenceLine<'a> {
    JustAComment(LineWithJustAComment),
    OneOrMoreItems(LineWithItems<'a>),
    Empty,
}

/// Given data on each line in a multiline string sequence,
/// group lines together into "items".
///
/// Each item contains exactly one string element,
/// but might contain multiple comments attached to that element
/// that must move with the element when the string sequence is sorted.
///
/// Note that any comments following the last item are discarded here,
/// but that doesn't matter: we add them back in `into_sorted_source_code()`
/// as part of the `postlude` (see comments in that function)
fn collect_string_sequence_items<'a>(
    lines: Vec<StringSequenceLine<'a>>,
    dunder_all_range: TextRange,
    locator: &Locator,
) -> Vec<StringSequenceItem<'a>> {
    let mut all_items = Vec::with_capacity(match lines.as_slice() {
        [StringSequenceLine::OneOrMoreItems(single)] => single.num_items(),
        _ => lines.len(),
    });
    let mut first_item_encountered = false;
    let mut preceding_comment_ranges = vec![];
    for line in lines {
        match line {
            StringSequenceLine::JustAComment(LineWithJustAComment(comment_range)) => {
                // Comments on the same line as the opening paren and before any elements
                // count as part of the "prelude"; these are not grouped into any item...
                if first_item_encountered
                    || locator.line_start(comment_range.start())
                        != locator.line_start(dunder_all_range.start())
                {
                    // ...but for all other comments that precede an element,
                    // group the comment with the element following that comment
                    // into an "item", so that the comment moves as one with the element
                    // when the list/tuple/set is sorted
                    preceding_comment_ranges.push(comment_range);
                }
            }
            StringSequenceLine::OneOrMoreItems(LineWithItems {
                first_item: (first_val, first_range),
                following_items,
                trailing_comment_range: comment_range,
            }) => {
                first_item_encountered = true;
                all_items.push(StringSequenceItem::new(
                    first_val,
                    std::mem::take(&mut preceding_comment_ranges),
                    first_range,
                    comment_range,
                ));
                for (value, range) in following_items {
                    all_items.push(StringSequenceItem::with_no_comments(value, range));
                }
            }
            StringSequenceLine::Empty => continue, // discard empty lines
        }
    }
    all_items
}

/// An instance of this struct represents a single element
/// from a multiline string sequence, *and* any comments that
/// are "attached" to it. The comments "attached" to the element
/// will move with the element when the tuple/list/set is sorted.
///
/// Comments on their own line immediately preceding the element will
/// always form a contiguous range with the range of the element itself;
/// however, inline comments won't necessary form a contiguous range.
/// Consider the following scenario, where both `# comment0` and `# comment1`
/// will move with the "a" element when the list is sorted:
///
/// ```python
/// __all__ = [
///     "b",
///     # comment0
///     "a", "c",  # comment1
/// ]
/// ```
///
/// The desired outcome here is:
///
/// ```python
/// __all__ = [
///     # comment0
///     "a",  # comment1
///     "b",
///     "c",
/// ]
/// ```
///
/// To achieve this, both `# comment0` and `# comment1`
/// are grouped into the `StringSequenceItem` instance
/// where the value is `"a"`, even though the source-code range
/// of `# comment1` does not form a contiguous range with the
/// source-code range of `"a"`.
#[derive(Debug)]
struct StringSequenceItem<'a> {
    value: &'a str,
    preceding_comment_ranges: Vec<TextRange>,
    element_range: TextRange,
    // total_range incorporates the ranges of preceding comments
    // (which must be contiguous with the element),
    // but doesn't incorporate any trailing comments
    // (which might be contiguous, but also might not be)
    total_range: TextRange,
    end_of_line_comments: Option<TextRange>,
}

impl<'a> StringSequenceItem<'a> {
    fn new(
        value: &'a str,
        preceding_comment_ranges: Vec<TextRange>,
        element_range: TextRange,
        end_of_line_comments: Option<TextRange>,
    ) -> Self {
        let total_range = {
            if let Some(first_comment_range) = preceding_comment_ranges.first() {
                TextRange::new(first_comment_range.start(), element_range.end())
            } else {
                element_range
            }
        };
        Self {
            value,
            preceding_comment_ranges,
            element_range,
            total_range,
            end_of_line_comments,
        }
    }

    fn with_no_comments(value: &'a str, element_range: TextRange) -> Self {
        Self::new(value, vec![], element_range, None)
    }
}

impl Ranged for StringSequenceItem<'_> {
    fn range(&self) -> TextRange {
        self.total_range
    }
}

/// Return a string representing the "prelude" for a
/// multiline string sequence.
///
/// See inline comments in
/// `MultilineStringSequenceValue::into_sorted_source_code()`
/// for a definition of the term "prelude" in this context.
fn multiline_string_sequence_prelude<'a>(
    first_item_start_offset: TextSize,
    newline: &str,
    dunder_all_offset: TextSize,
    locator: &'a Locator,
) -> Cow<'a, str> {
    let prelude_end = {
        let first_item_line_offset = locator.line_start(first_item_start_offset);
        if first_item_line_offset == locator.line_start(dunder_all_offset) {
            first_item_start_offset
        } else {
            first_item_line_offset
        }
    };
    let prelude = locator.slice(TextRange::new(dunder_all_offset, prelude_end));
    if prelude.ends_with(['\r', '\n']) {
        Cow::Borrowed(prelude)
    } else {
        Cow::Owned(format!("{}{}", prelude.trim_end(), newline))
    }
}

/// Join the elements and comments of a multiline string sequence
/// definition into a single string.
///
/// The resulting string does not include the "prelude" or
/// "postlude" of the tuple/set/list.
/// (See inline comments in
/// `MultilineStringSequence::into_sorted_source_code()` for
/// definitions of the terms "prelude" and "postlude" in this
/// context.)
fn join_multiline_string_sequence_items(
    sorted_items: &[StringSequenceItem],
    locator: &Locator,
    item_indent: &str,
    newline: &str,
    needs_trailing_comma: bool,
) -> String {
    assert!(
        sorted_items.len() >= 2,
        "A sequence with < 2 items cannot be unsorted"
    );
    let last_item_index = sorted_items.len() - 1;

    let mut new_dunder_all = String::new();
    for (i, item) in sorted_items.iter().enumerate() {
        let is_final_item = i == last_item_index;
        for comment_range in &item.preceding_comment_ranges {
            new_dunder_all.push_str(item_indent);
            new_dunder_all.push_str(locator.slice(comment_range));
            new_dunder_all.push_str(newline);
        }
        new_dunder_all.push_str(item_indent);
        new_dunder_all.push_str(locator.slice(item.element_range));
        if !is_final_item || needs_trailing_comma {
            new_dunder_all.push(',');
        }
        if let Some(trailing_comments) = item.end_of_line_comments {
            new_dunder_all.push_str(locator.slice(trailing_comments));
        }
        if !is_final_item {
            new_dunder_all.push_str(newline);
        }
    }
    new_dunder_all
}

/// Return a string representing the "postlude" for a
/// multiline string sequence.
///
/// See inline comments in
/// `MultilineStringSequence::into_sorted_source_code()`
/// for a definition of the term "postlude" in this context.
fn multiline_string_sequence_postlude<'a>(
    last_item_end_offset: TextSize,
    newline: &str,
    leading_indent: &str,
    item_indent: &str,
    dunder_all_range_end: TextSize,
    locator: &'a Locator,
) -> Cow<'a, str> {
    let postlude_start = {
        let last_item_line_offset = locator.line_end(last_item_end_offset);
        if last_item_line_offset == locator.line_end(dunder_all_range_end) {
            last_item_end_offset
        } else {
            last_item_line_offset
        }
    };
    let postlude = locator.slice(TextRange::new(postlude_start, dunder_all_range_end));

    // If the postlude consists solely of a closing parenthesis
    // (not preceded by any whitespace/newlines),
    // plus possibly a single trailing comma prior to the parenthesis,
    // fixup the postlude so that the parenthesis appears on its own line,
    // and so that the final item has a trailing comma.
    // This produces formatting more similar
    // to that which the formatter would produce.
    if postlude.len() <= 2 {
        let mut reversed_postlude_chars = postlude.chars().rev();
        if let Some(closing_paren @ (')' | '}' | ']')) = reversed_postlude_chars.next() {
            if reversed_postlude_chars.next().map_or(true, |c| c == ',') {
                return Cow::Owned(format!(",{newline}{leading_indent}{closing_paren}"));
            }
        }
    }

    let newline_chars = ['\r', '\n'];
    if !postlude.starts_with(newline_chars) {
        return Cow::Borrowed(postlude);
    }

    // The rest of this function uses heuristics to
    // avoid very long indents for the closing paren
    // that don't match the style for the rest of the
    // fixed-up multiline string sequence.
    //
    // For example, we want to avoid something like this
    // (not uncommon in code that hasn't been
    // autoformatted)...
    //
    // ```python
    // __all__ = ["xxxxxx", "yyyyyy",
    //            "aaaaaa", "bbbbbb",
    //            ]
    // ```
    //
    // ...getting autofixed to this:
    //
    // ```python
    // __all__ = [
    //     "a",
    //     "b",
    //     "x",
    //     "y",
    //            ]
    // ```
    if TextSize::of(leading_indentation(
        postlude.trim_start_matches(newline_chars),
    )) <= TextSize::of(item_indent)
    {
        return Cow::Borrowed(postlude);
    }
    let trimmed_postlude = postlude.trim_start();
    if trimmed_postlude.starts_with([']', ')', '}']) {
        return Cow::Owned(format!("{newline}{leading_indent}{trimmed_postlude}"));
    }
    Cow::Borrowed(postlude)
}
