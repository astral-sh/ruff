use std::borrow::Cow;
use std::cmp::Ordering;

use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_codegen::Stylist;
use ruff_python_parser::{lexer, Mode, Tok};
use ruff_python_stdlib::str::is_cased_uppercase;
use ruff_python_trivia::leading_indentation;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;

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
/// This rule's fix should be safe for single-line `__all__` definitions
/// and for multiline `__all__` definitions without comments.
/// For multiline `__all__` definitions that include comments,
/// the fix is marked as unsafe, as it can be hard to tell where the comments
/// should be moved to when sorting the contents of `__all__`.
#[violation]
pub struct UnsortedDunderAll;

impl AlwaysFixableViolation for UnsortedDunderAll {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__all__` is not sorted")
    }

    fn fix_title(&self) -> String {
        "Apply an isort-style sorting to `__all__`".to_string()
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

/// Sort an `__all__` mutation from a call to `.extend()`.
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

    let locator = checker.locator();

    let Some(
        dunder_all_val @ DunderAllValue {
            range, multiline, ..
        },
    ) = DunderAllValue::from_expr(node, locator)
    else {
        return;
    };

    let new_dunder_all = match dunder_all_val.into_sorted_source_code(locator, checker.stylist()) {
        SortedDunderAll::AlreadySorted => return,
        SortedDunderAll::Sorted(value) => value,
    };

    let applicability = {
        if multiline && checker.indexer().comment_ranges().intersects(node.range()) {
            Applicability::Unsafe
        } else {
            Applicability::Safe
        }
    };

    let edit = Edit::range_replacement(new_dunder_all, range);

    checker.diagnostics.push(
        Diagnostic::new(UnsortedDunderAll, range)
            .with_fix(Fix::applicable_edit(edit, applicability)),
    );
}

/// An instance of this struct encapsulates an analysis
/// of a Python tuple/list that represents an `__all__`
/// definition or augmentation.
struct DunderAllValue {
    items: Vec<DunderAllItem>,
    range: TextRange,
    multiline: bool,
    ends_with_trailing_comma: bool,
}

impl DunderAllValue {
    /// Analyse an AST node for a Python tuple/list that represents an `__all__`
    /// definition or augmentation. Return `None` if the analysis fails
    /// for whatever reason, or if it looks like we're not actually looking at a
    /// tuple/list after all.
    fn from_expr(value: &ast::Expr, locator: &Locator) -> Option<DunderAllValue> {
        // Step (1): inspect the AST to check that we're looking at something vaguely sane:
        let (elts, range) = match value {
            ast::Expr::List(ast::ExprList { elts, range, .. }) => (elts, range),
            ast::Expr::Tuple(ast::ExprTuple { elts, range, .. }) => (elts, range),
            _ => return None,
        };

        // An `__all__` definition with < 2 elements can't be unsorted;
        // no point in proceeding any further here.
        //
        // N.B. Here, this is just an optimisation
        // (and to avoid us rewriting code when we don't have to).
        //
        // While other parts of this file *do* depend on there being a
        // minimum of 2 elements in `__all__`, that invariant
        // is maintained elsewhere. (For example, see comments at the
        // start of `into_sorted_source_code()`.)
        if elts.len() < 2 {
            return None;
        }

        for elt in elts {
            // Only consider sorting it if __all__ only has strings in it
            let string_literal = elt.as_string_literal_expr()?;
            // And if any strings are implicitly concatenated, don't bother
            if string_literal.value.is_implicit_concatenated() {
                return None;
            }
        }

        // Step (2): parse the `__all__` definition using the raw tokens.
        // See the docs for `collect_dunder_all_lines()` for why we have to
        // use the raw tokens, rather than just the AST, to do this parsing.
        //
        // (2a). Start by collecting information on each line individually:
        let (lines, ends_with_trailing_comma) = collect_dunder_all_lines(*range, locator)?;

        // (2b). Group lines together into sortable "items":
        //   - Any "item" contains a single element of the `__all__` list/tuple
        //   - "Items" are ordered according to the element they contain
        //   - Assume that any comments on their own line are meant to be grouped
        //     with the element immediately below them: if the element moves,
        //     the comments above the element move with it.
        //   - The same goes for any comments on the same line as an element:
        //     if the element moves, the comment moves with it.
        let items = collect_dunder_all_items(lines, *range, locator);

        Some(DunderAllValue {
            items,
            range: *range,
            multiline: locator.contains_line_break(value.range()),
            ends_with_trailing_comma,
        })
    }

    /// Implementation of the unstable [`&[T].is_sorted`] function.
    /// See <https://github.com/rust-lang/rust/issues/53485>
    fn is_already_sorted(&self) -> bool {
        // tuple_windows() clones,
        // but here that's okay: we're only cloning *references*, rather than the items themselves
        for (this, next) in self.items.iter().tuple_windows() {
            if next < this {
                return false;
            }
        }
        true
    }

    /// Determine whether `__all__` is already sorted.
    /// If it is not already sorted, attempt to sort `__all__`,
    /// and return a string with the sorted `__all__ definition/augmentation`
    /// that can be inserted into the source code as a range replacement.
    fn into_sorted_source_code(self, locator: &Locator, stylist: &Stylist) -> SortedDunderAll {
        // As well as saving us unnecessary work,
        // returning early here also means that we can rely on the invariant
        // throughout the rest of this function that both `items` and `sorted_items`
        // have length of at least two.
        let [first_item, .., last_item] = self.items.as_slice() else {
            return SortedDunderAll::AlreadySorted;
        };
        if self.is_already_sorted() {
            return SortedDunderAll::AlreadySorted;
        }

        // As well as the "items" in the `__all__` definition,
        // there is also a "prelude" and a "postlude":
        //  - Prelude == the region of source code from the opening parenthesis
        //    (if there was one), up to the start of the first item in `__all__`.
        //  - Postlude == the region of source code from the end of the last
        //    item in `__all__` up to and including the closing parenthesis
        //    (if there was one).
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
        //                                   <-- Tokenizer emits a LogicalNewline here
        // ```
        //
        // - The prelude in the above example is the source code region
        //   starting at the opening `[` and ending just before `# comment1`.
        //   `comment0` here counts as part of the prelude because it is on
        //   the same line as the opening paren, and because we haven't encountered
        //   any elements of `__all__` yet, but `comment1` counts as part of the first item,
        //   as it's on its own line, and all comments on their own line are grouped
        //   with the next element below them to make "items",
        //   (an "item" being a region of source code that all moves as one unit
        //   when `__all__` is sorted).
        // - The postlude in the above example is the source code region starting
        //   just after `# comment2` and ending just before the logical newline
        //   that follows the closing paren. `# comment2` is part of the last item,
        //   as it's an inline comment on the same line as an element,
        //   but `# comment3` becomes part of the postlude because there are no items
        //   below it.
        //
        // "Prelude" and "postlude" could both possibly be empty strings, for example
        // in a situation like this, where there is neither an opening parenthesis
        // nor a closing parenthesis:
        //
        // ```python
        // __all__ = "foo", "bar", "baz"
        // ```
        //
        let prelude_end = {
            let first_item_line_offset = locator.line_start(first_item.start());
            if first_item_line_offset == locator.line_start(self.start()) {
                first_item.start()
            } else {
                first_item_line_offset
            }
        };
        let postlude_start = {
            let last_item_line_offset = locator.line_end(last_item.end());
            if last_item_line_offset == locator.line_end(self.end()) {
                last_item.end()
            } else {
                last_item_line_offset
            }
        };
        let mut prelude = Cow::Borrowed(locator.slice(TextRange::new(self.start(), prelude_end)));
        let mut postlude = Cow::Borrowed(locator.slice(TextRange::new(postlude_start, self.end())));

        let start_offset = self.start();
        let mut sorted_items = self.items;
        sorted_items.sort();

        let joined_items = if self.multiline {
            let leading_indent = leading_indentation(locator.full_line(start_offset));
            let item_indent = format!("{}{}", leading_indent, stylist.indentation().as_str());
            let newline = stylist.line_ending().as_str();
            prelude = Cow::Owned(format!("{}{}", prelude.trim_end(), newline));
            postlude = fixup_postlude(postlude, newline, leading_indent, &item_indent);
            join_multiline_dunder_all_items(
                &sorted_items,
                locator,
                &item_indent,
                newline,
                self.ends_with_trailing_comma,
            )
        } else {
            join_singleline_dunder_all_items(&sorted_items, locator)
        };

        SortedDunderAll::Sorted(format!("{prelude}{joined_items}{postlude}"))
    }
}

impl Ranged for DunderAllValue {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Fixup the postlude for a multiline `__all__` definition.
///
/// Without the fixup, closing `)` or `]` characters
/// at the end of sorted `__all__` definitions can sometimes
/// have strange indentations.
fn fixup_postlude<'a>(
    postlude: Cow<'a, str>,
    newline: &str,
    leading_indent: &str,
    item_indent: &str,
) -> Cow<'a, str> {
    if !postlude.starts_with(newline) {
        return postlude;
    }
    if TextSize::of(leading_indentation(postlude.trim_start_matches(newline)))
        <= TextSize::of(item_indent)
    {
        return postlude;
    }
    let trimmed_postlude = postlude.trim_start();
    if trimmed_postlude.starts_with(']') || trimmed_postlude.starts_with(')') {
        return Cow::Owned(format!("{newline}{leading_indent}{trimmed_postlude}"));
    }
    postlude
}

/// Variants of this enum are returned by `into_sorted_source_code()`.
///
/// - `SortedDunderAll::AlreadySorted` is returned if `__all__` was
///   already sorted; this means no code rewriting is required.
/// - `SortedDunderAll::Sorted` is returned if `__all__` was not already
///   sorted. The string data attached to this variant is the source
///   code of the sorted `__all__`, that can be inserted into the source
///   code as a `range_replacement` autofix.
#[derive(Debug)]
enum SortedDunderAll {
    AlreadySorted,
    Sorted(String),
}

/// Collect data on each line of `__all__`.
/// Return `None` if `__all__` appears to be invalid,
/// or if it's an edge case we don't support.
///
/// Why do we need to do this using the raw tokens,
/// when we already have the AST? The AST strips out
/// crucial information that we need to track here, such as:
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
    locator: &Locator,
) -> Option<(Vec<DunderAllLine>, bool)> {
    // These first three variables are used for keeping track of state
    // regarding the entirety of the `__all__` definition...
    let mut parentheses_open = false;
    let mut ends_with_trailing_comma = false;
    let mut lines = vec![];
    // ... all state regarding a single line of an `__all__` definition
    // is encapsulated in this variable
    let mut line_state = LineState::default();

    // `lex_starts_at()` gives us absolute ranges rather than relative ranges,
    // but (surprisingly) we still need to pass in the slice of code we want it to lex,
    // rather than the whole source file:
    for pair in lexer::lex_starts_at(locator.slice(range), Mode::Expression, range.start()) {
        let (tok, subrange) = pair.ok()?;
        match tok {
            // If exactly one `Lpar` or `Lsqb` is encountered, that's fine
            // -- a valid __all__ definition has to be a list or tuple,
            // and most (though not all) lists/tuples start with either a `(` or a `[`.
            //
            // Any more than one `(` or `[` in an `__all__` definition, however,
            // indicates that we've got something here that's just too complex
            // for us to handle. Maybe a string element in `__all__` is parenthesized;
            // maybe the `__all__` definition is in fact invalid syntax;
            // maybe there's some other thing going on that we haven't anticipated.
            //
            // Whatever the case -- if we encounter more than one `(` or `[`,
            // we evidently don't know what to do here. So just return `None` to
            // signal failure.
            Tok::Lpar | Tok::Lsqb => {
                if parentheses_open {
                    return None;
                }
                parentheses_open = true;
            }
            Tok::Rpar | Tok::Rsqb | Tok::Newline => {
                if let Some(line) = line_state.into_dunder_all_line() {
                    lines.push(line);
                }
                break;
            }
            Tok::NonLogicalNewline => {
                if let Some(line) = line_state.into_dunder_all_line() {
                    lines.push(line);
                }
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
            _ => return None,
        }
    }
    Some((lines, ends_with_trailing_comma))
}

/// This struct is for keeping track of state
/// regarding a single line in an `__all__` definition.
/// It is purely internal to `collect_dunder_all_lines()`,
/// and should not be used outside that function.
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

    fn visit_comment_token(&mut self, token_range: TextRange) {
        self.comment_in_line = {
            if let Some(comment_range_start) = self.comment_range_start {
                Some(TextRange::new(comment_range_start, token_range.end()))
            } else {
                Some(token_range)
            }
        }
    }

    fn into_dunder_all_line(self) -> Option<DunderAllLine> {
        if let Some(first_item) = self.first_item_in_line {
            Some(DunderAllLine::OneOrMoreItems(LineWithItems {
                first_item,
                following_items: self.following_items_in_line,
                trailing_comment_range: self.comment_in_line,
            }))
        } else {
            self.comment_in_line.map(|comment_range| {
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

#[derive(Debug)]
enum DunderAllLine {
    JustAComment(LineWithJustAComment),
    OneOrMoreItems(LineWithItems),
}

/// Given data on each line in `__all__`, group lines together into "items".
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
        }
    }
    all_items
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
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
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
        } else if value.chars().next().is_some_and(char::is_uppercase) {
            Self::Class
        // E.g. `some_variable` or `some_function`
        } else {
            Self::Other
        }
    }
}

/// An instance of this struct represents a single element
/// from the original tuple/list, *and* any comments that
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
    category: InferredMemberType,
    preceding_comment_ranges: Vec<TextRange>,
    element_range: TextRange,
    // total_range incorporates the ranges of preceding comments
    // (which must be contiguous with the element),
    // but doesn't incorporate any trailing comments
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
        let category = InferredMemberType::of(value.as_str());
        let total_range = {
            if let Some(first_comment_range) = preceding_comment_ranges.first() {
                TextRange::new(first_comment_range.start(), element_range.end())
            } else {
                element_range
            }
        };
        Self {
            value,
            category,
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

impl Ord for DunderAllItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.category
            .cmp(&other.category)
            .then_with(|| natord::compare(&self.value, &other.value))
    }
}

impl PartialOrd for DunderAllItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for DunderAllItem {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for DunderAllItem {}

fn join_singleline_dunder_all_items(sorted_items: &[DunderAllItem], locator: &Locator) -> String {
    sorted_items
        .iter()
        .map(|item| locator.slice(item))
        .join(", ")
}

fn join_multiline_dunder_all_items(
    sorted_items: &[DunderAllItem],
    locator: &Locator,
    item_indent: &str,
    newline: &str,
    needs_trailing_comma: bool,
) -> String {
    let max_index = sorted_items.len() - 1;

    let mut new_dunder_all = String::new();
    for (i, item) in sorted_items.iter().enumerate() {
        let is_final_item = i == max_index;
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
