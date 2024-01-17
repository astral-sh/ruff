use std::borrow::Cow;
use std::cmp::Ordering;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_codegen::Stylist;
use ruff_python_parser::{lexer, Mode, Tok};
use ruff_python_stdlib::str::is_cased_uppercase;
use ruff_python_trivia::leading_indentation;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::sorting_helpers::{
    sort_single_line_elements_sequence, SequenceKind, SortClassification,
};

use itertools::Itertools;
use natord;

/// ## What it does
/// Checks for `__all__` definitions that are not ordered
/// according to an "isort-style" sort.
///
/// An isort-style sort sorts items first according to their casing:
/// SCREAMING_SNAKE_CASE names (conventionally used for global constants)
/// come first, followed by CamelCase names (conventionally used for
/// classes), followed by anything else. Within each category,
/// a [natural sort](https://en.wikipedia.org/wiki/Natural_sort_order)
/// is used to order the elements.
///
/// ## Why is this bad?
/// Consistency is good. Use a common convention for `__all__` to make your
/// code more readable and idiomatic.
///
/// ## Example
/// ```python
/// import sys
///
/// __all__ = [
///     "b",
///     "c",
///     "a",
/// ]
///
/// if sys.platform == "win32":
///     __all__ += ["z", "y"]
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// __all__ = [
///     "a",
///     "b",
///     "c",
/// ]
///
/// if sys.platform == "win32":
///     __all__ += ["y", "z"]
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as always being safe, in that
/// it should never alter the semantics of any Python code.
/// However, note that for multiline `__all__` definitions
/// that include comments on their own line, it can be hard
/// to tell where the comments should be moved to when sorting
/// the contents of `__all__`. While this rule's fix will
/// never delete a comment, it might *sometimes* move a
/// comment to an unexpected location.
#[violation]
pub struct UnsortedDunderAll;

impl Violation for UnsortedDunderAll {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__all__` is not sorted")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Apply an isort-style sorting to `__all__`".to_string())
    }
}

/// Sort an `__all__` definition represented by a `StmtAssign` AST node.
/// For example: `__all__ = ["b", "c", "a"]`.
pub(crate) fn sort_dunder_all_assign(
    checker: &mut Checker,
    ast::StmtAssign { value, targets, .. }: &ast::StmtAssign,
) {
    if let [expr] = targets.as_slice() {
        sort_dunder_all(checker, expr, value);
    }
}

/// Sort an `__all__` mutation represented by a `StmtAugAssign` AST node.
/// For example: `__all__ += ["b", "c", "a"]`.
pub(crate) fn sort_dunder_all_aug_assign(checker: &mut Checker, node: &ast::StmtAugAssign) {
    if node.op.is_add() {
        sort_dunder_all(checker, &node.target, &node.value);
    }
}

/// Sort a tuple or list passed to `__all__.extend()`.
pub(crate) fn sort_dunder_all_extend_call(
    checker: &mut Checker,
    ast::ExprCall {
        func,
        arguments: ast::Arguments { args, keywords, .. },
        ..
    }: &ast::ExprCall,
) {
    let ([value_passed], []) = (args.as_slice(), keywords.as_slice()) else {
        return;
    };
    let ast::Expr::Attribute(ast::ExprAttribute {
        ref value,
        ref attr,
        ..
    }) = **func
    else {
        return;
    };
    if attr == "extend" {
        sort_dunder_all(checker, value, value_passed);
    }
}

/// Sort an `__all__` definition represented by a `StmtAnnAssign` AST node.
/// For example: `__all__: list[str] = ["b", "c", "a"]`.
pub(crate) fn sort_dunder_all_ann_assign(checker: &mut Checker, node: &ast::StmtAnnAssign) {
    if let Some(value) = &node.value {
        sort_dunder_all(checker, &node.target, value);
    }
}

/// Sort a tuple or list that defines or mutates the global variable `__all__`.
///
/// This routine checks whether the tuple or list is sorted, and emits a
/// violation if it is not sorted. If the tuple/list was not sorted,
/// it attempts to set a `Fix` on the violation.
fn sort_dunder_all(checker: &mut Checker, target: &ast::Expr, node: &ast::Expr) {
    let ast::Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    if id != "__all__" {
        return;
    }

    // We're only interested in `__all__` in the global scope
    if !checker.semantic().current_scope().kind.is_module() {
        return;
    }

    let (elts, range, kind) = match node {
        ast::Expr::List(ast::ExprList { elts, range, .. }) => (elts, *range, SequenceKind::List),
        ast::Expr::Tuple(tuple_node @ ast::ExprTuple { elts, range, .. }) => {
            (elts, *range, SequenceKind::Tuple(tuple_node))
        }
        _ => return,
    };
    let elts = elts.iter().collect_vec();

    let elts_analysis = SortClassification::from_elements(&elts, |a, b| {
        AllItemSortKey::from(a).cmp(&AllItemSortKey::from(b))
    });
    if elts_analysis.is_not_a_list_of_string_literals() || elts_analysis.is_sorted() {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnsortedDunderAll, range);

    if let SortClassification::UnsortedAndMaybeFixable { items } = elts_analysis {
        if let Some(fix) = create_fix(range, &elts, &items, &kind, checker) {
            diagnostic.set_fix(fix);
        }
    }

    checker.diagnostics.push(diagnostic);
}

/// A struct to implement logic necessary to achieve
/// an "isort-style sort".
///
/// See the docs for this module as a whole for the
/// definition we use here of an "isort-style sort".
struct AllItemSortKey<'a> {
    category: InferredMemberType,
    value: &'a str,
}

impl Ord for AllItemSortKey<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.category
            .cmp(&other.category)
            .then_with(|| natord::compare(self.value, other.value))
    }
}

impl PartialOrd for AllItemSortKey<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for AllItemSortKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for AllItemSortKey<'_> {}

impl<'a> From<&'a str> for AllItemSortKey<'a> {
    fn from(value: &'a str) -> Self {
        Self {
            category: InferredMemberType::of(value),
            value,
        }
    }
}

impl<'a> From<&'a DunderAllItem> for AllItemSortKey<'a> {
    fn from(item: &'a DunderAllItem) -> Self {
        Self::from(item.value.as_str())
    }
}

/// Classification for an element in `__all__`.
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

/// Attempt to return `Some(fix)`, where `fix` is a `Fix`
/// that can be set on the diagnostic to sort the user's
/// `__all__` definition
///
/// Return `None` if it's a multiline `__all__` definition
/// and the token-based analysis in
/// `MultilineDunderAllValue::from_source_range()` encounters
/// something it doesn't expect, meaning the violation
/// is unfixable in this instance.
fn create_fix(
    range: TextRange,
    elts: &[&ast::Expr],
    string_items: &[&str],
    kind: &SequenceKind,
    checker: &Checker,
) -> Option<Fix> {
    let locator = checker.locator();
    let is_multiline = locator.contains_line_break(range);

    let sorted_source_code = {
        // The machinery in the `MultilineDunderAllValue` is actually
        // sophisticated enough that it would work just as well for
        // single-line `__all__` definitions, and we could reduce
        // the number of lines of code in this file by doing that.
        // Unfortunately, however, `MultilineDunderAllValue::from_source_range()`
        // must process every token in an `__all__` definition as
        // part of its analysis, and this is quite slow. For
        // single-line `__all__` definitions, it's also unnecessary,
        // as it's impossible to have comments in between the
        // `__all__` elements if the `__all__` definition is all on
        // a single line. Therefore, as an optimisation, we do the
        // bare minimum of token-processing for single-line `__all__`
        // definitions:
        if is_multiline {
            let value = MultilineDunderAllValue::from_source_range(range, kind, locator)?;
            assert_eq!(value.items.len(), elts.len());
            value.into_sorted_source_code(locator, checker.stylist())
        } else {
            sort_single_line_elements_sequence(kind, elts, string_items, locator, |a, b| {
                AllItemSortKey::from(a).cmp(&AllItemSortKey::from(b))
            })
        }
    };

    Some(Fix::safe_edit(Edit::range_replacement(
        sorted_source_code,
        range,
    )))
}

/// An instance of this struct encapsulates an analysis
/// of a multiline Python tuple/list that represents an
/// `__all__` definition or augmentation.
struct MultilineDunderAllValue {
    items: Vec<DunderAllItem>,
    range: TextRange,
    ends_with_trailing_comma: bool,
}

impl MultilineDunderAllValue {
    /// Analyse the source range for a multiline Python tuple/list that
    /// represents an `__all__` definition or augmentation. Return `None`
    /// if the analysis fails for whatever reason.
    fn from_source_range(
        range: TextRange,
        kind: &SequenceKind,
        locator: &Locator,
    ) -> Option<MultilineDunderAllValue> {
        // Parse the multiline `__all__` definition using the raw tokens.
        // See the docs for `collect_dunder_all_lines()` for why we have to
        // use the raw tokens, rather than just the AST, to do this parsing.
        //
        // Step (1). Start by collecting information on each line individually:
        let (lines, ends_with_trailing_comma) = collect_dunder_all_lines(range, kind, locator)?;

        // Step (2). Group lines together into sortable "items":
        //   - Any "item" contains a single element of the `__all__` list/tuple
        //   - Assume that any comments on their own line are meant to be grouped
        //     with the element immediately below them: if the element moves,
        //     the comments above the element move with it.
        //   - The same goes for any comments on the same line as an element:
        //     if the element moves, the comment moves with it.
        let items = collect_dunder_all_items(lines, range, locator);

        Some(MultilineDunderAllValue {
            items,
            range,
            ends_with_trailing_comma,
        })
    }

    /// Sort a multiline `__all__` definition
    /// that is known to be unsorted.
    ///
    /// Panics if this is called and `self.items`
    /// has length < 2. It's redundant to call this method in this case,
    /// since lists with < 2 items cannot be unsorted,
    /// so this is a logic error.
    fn into_sorted_source_code(mut self, locator: &Locator, stylist: &Stylist) -> String {
        let (first_item_start, last_item_end) = match self.items.as_slice() {
            [first_item, .., last_item] => (first_item.start(), last_item.end()),
            _ => panic!(
                "We shouldn't be attempting an autofix if `__all__` has < 2 elements;
                an `__all__` definition with 1 or 0 elements cannot be unsorted."
            ),
        };

        // As well as the "items" in the `__all__` definition,
        // there is also a "prelude" and a "postlude":
        //  - Prelude == the region of source code from the opening parenthesis,
        //    up to the start of the first item in `__all__`.
        //  - Postlude == the region of source code from the end of the last
        //    item in `__all__` up to and including the closing parenthesis.
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
            multiline_dunder_all_prelude(first_item_start, newline, start_offset, locator);
        let postlude = multiline_dunder_all_postlude(
            last_item_end,
            newline,
            leading_indent,
            &item_indent,
            self.end(),
            locator,
        );

        self.items
            .sort_by(|this, next| AllItemSortKey::from(this).cmp(&AllItemSortKey::from(next)));
        let joined_items = join_multiline_dunder_all_items(
            &self.items,
            locator,
            &item_indent,
            newline,
            self.ends_with_trailing_comma,
        );

        format!("{prelude}{joined_items}{postlude}")
    }
}

impl Ranged for MultilineDunderAllValue {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Collect data on each line of a multiline `__all__` definition.
/// Return `None` if `__all__` appears to be invalid,
/// or if it's an edge case we don't support.
///
/// Why do we need to do this using the raw tokens,
/// when we already have the AST? The AST strips out
/// crucial information that we need to track here for
/// a multiline `__all__` definition, such as:
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
fn collect_dunder_all_lines(
    range: TextRange,
    kind: &SequenceKind,
    locator: &Locator,
) -> Option<(Vec<DunderAllLine>, bool)> {
    // These first two variables are used for keeping track of state
    // regarding the entirety of the `__all__` definition...
    let mut ends_with_trailing_comma = false;
    let mut lines = vec![];
    // ... all state regarding a single line of an `__all__` definition
    // is encapsulated in this variable
    let mut line_state = LineState::default();

    // `lex_starts_at()` gives us absolute ranges rather than relative ranges,
    // but (surprisingly) we still need to pass in the slice of code we want it to lex,
    // rather than the whole source file:
    let mut token_iter =
        lexer::lex_starts_at(locator.slice(range), Mode::Expression, range.start());
    let (first_tok, _) = token_iter.next()?.ok()?;
    if first_tok != kind.opening_token_for_multiline_definition() {
        return None;
    }
    let expected_final_token = kind.closing_token_for_multiline_definition();

    for pair in token_iter {
        let (tok, subrange) = pair.ok()?;
        match tok {
            Tok::NonLogicalNewline => {
                lines.push(line_state.into_dunder_all_line());
                line_state = LineState::default();
            }
            Tok::Comment(_) => {
                line_state.visit_comment_token(subrange);
            }
            Tok::String { value, .. } => {
                line_state.visit_string_token(value, subrange);
                ends_with_trailing_comma = false;
            }
            Tok::Comma => {
                line_state.visit_comma_token(subrange);
                ends_with_trailing_comma = true;
            }
            tok if tok == expected_final_token => {
                lines.push(line_state.into_dunder_all_line());
                break;
            }
            _ => return None,
        }
    }
    Some((lines, ends_with_trailing_comma))
}

/// This struct is for keeping track of state
/// regarding a single line in a multiline `__all__` definition.
/// It is purely internal to `collect_dunder_all_lines()`,
/// and should not be used outside that function.
///
/// There are three possible kinds of line in a multiline
/// `__all__` definition, and we don't know what kind of a line
/// we're in until all tokens in that line have been processed:
///
/// - A line with just a comment (`DunderAllLine::JustAComment)`)
/// - A line with one or more string items in it (`DunderAllLine::OneOrMoreItems`)
/// - An empty line (`DunderAllLine::Empty`)
///
/// As we process the tokens in a single line,
/// this struct accumulates the necessary state for us
/// to be able to determine what kind of a line we're in.
/// Once the entire line has been processed, `into_dunder_all_line()`
/// is called, which consumes `self` and produces the
/// classification for the line.
#[derive(Debug, Default)]
struct LineState {
    first_item_in_line: Option<(String, TextRange)>,
    following_items_in_line: Vec<(String, TextRange)>,
    comment_range_start: Option<TextSize>,
    comment_in_line: Option<TextRange>,
}

impl LineState {
    fn visit_string_token(&mut self, token_value: String, token_range: TextRange) {
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

    fn into_dunder_all_line(self) -> DunderAllLine {
        if let Some(first_item) = self.first_item_in_line {
            DunderAllLine::OneOrMoreItems(LineWithItems {
                first_item,
                following_items: self.following_items_in_line,
                trailing_comment_range: self.comment_in_line,
            })
        } else {
            self.comment_in_line
                .map_or(DunderAllLine::Empty, |comment_range| {
                    DunderAllLine::JustAComment(LineWithJustAComment(comment_range))
                })
        }
    }
}

/// Instances of this struct represent source-code lines in the middle
/// of multiline `__all__` tuples/lists where the line contains
/// 0 elements of the tuple/list, but the line does have a comment in it.
#[derive(Debug)]
struct LineWithJustAComment(TextRange);

/// Instances of this struct represent source-code lines in single-line
/// or multiline `__all__` tuples/lists where the line contains at least
/// 1 element of the tuple/list. The line may contain > 1 element of the
/// tuple/list, and may also have a trailing comment after the element(s).
#[derive(Debug)]
struct LineWithItems {
    // For elements in the list, we keep track of the value of the
    // value of the element as well as the source-code range of the element.
    // (We need to know the actual value so that we can sort the items.)
    first_item: (String, TextRange),
    following_items: Vec<(String, TextRange)>,
    // For comments, we only need to keep track of the source-code range.
    trailing_comment_range: Option<TextRange>,
}

impl LineWithItems {
    fn num_items(&self) -> usize {
        self.following_items.len() + 1
    }
}

/// An enumeration of the possible kinds of source-code lines
/// that can exist in a multiline `__all__` tuple or list:
///
/// - A line that has no string elements, but does have a comment.
/// - A line that has one or more string elements,
///   and may also have a trailing comment.
/// - An entirely empty line.
#[derive(Debug)]
enum DunderAllLine {
    JustAComment(LineWithJustAComment),
    OneOrMoreItems(LineWithItems),
    Empty,
}

/// Given data on each line in a multiline `__all__` definition,
/// group lines together into "items".
///
/// Each item contains exactly one string element,
/// but might contain multiple comments attached to that element
/// that must move with the element when `__all__` is sorted.
///
/// Note that any comments following the last item are discarded here,
/// but that doesn't matter: we add them back in `into_sorted_source_code()`
/// as part of the `postlude` (see comments in that function)
fn collect_dunder_all_items(
    lines: Vec<DunderAllLine>,
    dunder_all_range: TextRange,
    locator: &Locator,
) -> Vec<DunderAllItem> {
    let mut all_items = Vec::with_capacity(match lines.as_slice() {
        [DunderAllLine::OneOrMoreItems(single)] => single.num_items(),
        _ => lines.len(),
    });
    let mut first_item_encountered = false;
    let mut preceding_comment_ranges = vec![];
    for line in lines {
        match line {
            DunderAllLine::JustAComment(LineWithJustAComment(comment_range)) => {
                // Comments on the same line as the opening paren and before any elements
                // count as part of the "prelude"; these are not grouped into any item...
                if first_item_encountered
                    || locator.line_start(comment_range.start())
                        != locator.line_start(dunder_all_range.start())
                {
                    // ...but for all other comments that precede an element,
                    // group the comment with the element following that comment
                    // into an "item", so that the comment moves as one with the element
                    // when the `__all__` list/tuple is sorted
                    preceding_comment_ranges.push(comment_range);
                }
            }
            DunderAllLine::OneOrMoreItems(LineWithItems {
                first_item: (first_val, first_range),
                following_items,
                trailing_comment_range: comment_range,
            }) => {
                first_item_encountered = true;
                all_items.push(DunderAllItem::new(
                    first_val,
                    std::mem::take(&mut preceding_comment_ranges),
                    first_range,
                    comment_range,
                ));
                for (value, range) in following_items {
                    all_items.push(DunderAllItem::with_no_comments(value, range));
                }
            }
            DunderAllLine::Empty => continue, // discard empty lines
        }
    }
    all_items
}

/// An instance of this struct represents a single element
/// from a multiline `__all__` tuple/list, *and* any comments that
/// are "attached" to it. The comments "attached" to the element
/// will move with the element when the `__all__` tuple/list is sorted.
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
/// are grouped into the `DunderAllItem` instance
/// where the value is `"a"`, even though the source-code range
/// of `# comment1` does not form a contiguous range with the
/// source-code range of `"a"`.
#[derive(Debug)]
struct DunderAllItem {
    value: String,
    preceding_comment_ranges: Vec<TextRange>,
    element_range: TextRange,
    // total_range incorporates the ranges of preceding comments
    // (which must be contiguous with the element),
    // but doesn't incorporate any trailing comments
    // (which might be contiguous, but also might not be)
    total_range: TextRange,
    end_of_line_comments: Option<TextRange>,
}

impl DunderAllItem {
    fn new(
        value: String,
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

    fn with_no_comments(value: String, element_range: TextRange) -> Self {
        Self::new(value, vec![], element_range, None)
    }
}

impl Ranged for DunderAllItem {
    fn range(&self) -> TextRange {
        self.total_range
    }
}

/// Return a string representing the "prelude" for a
/// multiline `__all__` definition.
///
/// See inline comments in
/// `MultilineDunderAllValue::into_sorted_source_code()`
/// for a definition of the term "prelude" in this context.
fn multiline_dunder_all_prelude<'a>(
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

/// Join the elements and comments of a multiline `__all__`
/// definition into a single string.
///
/// The resulting string does not include the "prelude" or
/// "postlude" of the `__all__` definition.
/// (See inline comments in `MultilineDunderAllValue::into_sorted_source_code()`
/// for definitions of the terms "prelude" and "postlude"
/// in this context.)
fn join_multiline_dunder_all_items(
    sorted_items: &[DunderAllItem],
    locator: &Locator,
    item_indent: &str,
    newline: &str,
    needs_trailing_comma: bool,
) -> String {
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
/// multiline `__all__` definition.
///
/// See inline comments in
/// `MultilineDunderAllValue::into_sorted_source_code()`
/// for a definition of the term "postlude" in this context.
fn multiline_dunder_all_postlude<'a>(
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

    // The rest of this function uses heuristics to
    // avoid very long indents for the closing paren
    // that don't match the style for the rest of the
    // new `__all__` definition.
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
    let newline_chars = ['\r', '\n'];
    if !postlude.starts_with(newline_chars) {
        return Cow::Borrowed(postlude);
    }
    if TextSize::of(leading_indentation(
        postlude.trim_start_matches(newline_chars),
    )) <= TextSize::of(item_indent)
    {
        return Cow::Borrowed(postlude);
    }
    let trimmed_postlude = postlude.trim_start();
    if trimmed_postlude.starts_with([']', ')']) {
        return Cow::Owned(format!("{newline}{leading_indent}{trimmed_postlude}"));
    }
    Cow::Borrowed(postlude)
}
