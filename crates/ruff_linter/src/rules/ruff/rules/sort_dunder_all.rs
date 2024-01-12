use std::cmp::Ordering;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::whitespace::indentation;
use ruff_python_codegen::Stylist;
use ruff_python_parser::{lexer, Mode, Tok};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

use itertools::Itertools;
use natord;

/// ## What it does
/// Checks for `__all__` definitions that are not ordered
/// according to a [natural sort](https://en.wikipedia.org/wiki/Natural_sort_order).
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

impl Violation for UnsortedDunderAll {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__all__` is not sorted")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Sort `__all__` according to a natural sort".to_string())
    }
}

pub(crate) fn sort_dunder_all_assign(
    checker: &mut Checker,
    ast::StmtAssign { value, targets, .. }: &ast::StmtAssign,
    parent: &ast::Stmt,
) {
    let [ast::Expr::Name(ast::ExprName { id, .. })] = targets.as_slice() else {
        return;
    };
    sort_dunder_all(checker, id, value, parent);
}

pub(crate) fn sort_dunder_all_aug_assign(
    checker: &mut Checker,
    node: &ast::StmtAugAssign,
    parent: &ast::Stmt,
) {
    let ast::StmtAugAssign {
        value,
        target,
        op: ast::Operator::Add,
        ..
    } = node
    else {
        return;
    };
    let ast::Expr::Name(ast::ExprName { id, .. }) = target.as_ref() else {
        return;
    };
    sort_dunder_all(checker, id, value, parent);
}

pub(crate) fn sort_dunder_all_ann_assign(
    checker: &mut Checker,
    node: &ast::StmtAnnAssign,
    parent: &ast::Stmt,
) {
    let ast::StmtAnnAssign {
        target,
        value: Some(val),
        ..
    } = node
    else {
        return;
    };
    let ast::Expr::Name(ast::ExprName { id, .. }) = target.as_ref() else {
        return;
    };
    sort_dunder_all(checker, id, val, parent);
}

fn sort_dunder_all(checker: &mut Checker, target: &str, node: &ast::Expr, parent: &ast::Stmt) {
    if target != "__all__" {
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

    let new_dunder_all =
        match dunder_all_val.into_sorted_source_code(locator, parent, checker.stylist()) {
            SortedDunderAll::AlreadySorted => return,
            SortedDunderAll::Sorted(value) => value,
        };

    let mut diagnostic = Diagnostic::new(UnsortedDunderAll, range);

    if let Some(new_dunder_all) = new_dunder_all {
        let applicability = {
            if multiline && checker.indexer().comment_ranges().intersects(node.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            }
        };
        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(new_dunder_all, range),
            applicability,
        ));
    }

    checker.diagnostics.push(diagnostic);
}

struct DunderAllValue {
    items: Vec<DunderAllItem>,
    range: TextRange,
    multiline: bool,
    ends_with_trailing_comma: bool,
}

impl DunderAllValue {
    fn from_expr(value: &ast::Expr, locator: &Locator) -> Option<DunderAllValue> {
        // Step (1): inspect the AST to check that we're looking at something vaguely sane:
        let is_multiline = locator.contains_line_break(value.range());
        let (elts, range) = match value {
            ast::Expr::List(ast::ExprList { elts, range, .. }) => (elts, range),
            ast::Expr::Tuple(ast::ExprTuple { elts, range, .. }) => (elts, range),
            _ => return None,
        };

        // An `__all__` definition with < 2 elements can't be unsorted;
        // no point in proceeding any further here
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
            multiline: is_multiline,
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

    fn into_sorted_source_code(
        self,
        locator: &Locator,
        parent: &ast::Stmt,
        stylist: &Stylist,
    ) -> SortedDunderAll {
        // As well as saving us unnecessary work,
        // returning early here also means that we can rely on the invariant
        // throughout the rest of this function that both `items` and `sorted_items`
        // have length of at least two. If there are fewer than two items in `__all__`,
        // it is impossible for them *not* to compare equal here:
        if self.is_already_sorted() {
            return SortedDunderAll::AlreadySorted;
        }
        let [first_item, .., last_item] = self.items.as_slice() else {
            panic!("Expected to have already returned if the list had < 2 items")
        };

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
        //   just after `# comment2` and ending just before the lgoical newline
        //   that follows the closing paren. `# comment2` is part of the last item,
        //   as it's an inline comment on the same line as an element,
        //   but `# comment3` becomes part of the postlude because there are no items
        //   below it.
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
        let mut prelude = locator
            .slice(TextRange::new(self.start(), prelude_end))
            .to_string();
        let postlude = locator.slice(TextRange::new(postlude_start, self.end()));

        let mut sorted_items = self.items;
        sorted_items.sort();

        let joined_items = if self.multiline {
            let indentation = stylist.indentation();
            let newline = stylist.line_ending().as_str();
            prelude = format!("{}{}", prelude.trim_end(), newline);
            join_multiline_dunder_all_items(
                &sorted_items,
                locator,
                parent,
                indentation,
                newline,
                self.ends_with_trailing_comma,
            )
        } else {
            Some(join_singleline_dunder_all_items(&sorted_items, locator))
        };

        let new_dunder_all = joined_items.map(|items| format!("{prelude}{items}{postlude}"));
        SortedDunderAll::Sorted(new_dunder_all)
    }
}

impl Ranged for DunderAllValue {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Debug)]
enum SortedDunderAll {
    AlreadySorted,
    Sorted(Option<String>),
}

fn collect_dunder_all_lines(
    range: TextRange,
    locator: &Locator,
) -> Option<(Vec<DunderAllLine>, bool)> {
    // Collect data on each line of `__all__`.
    // Return `None` if `__all__` appears to be invalid,
    // or if it's an edge case we don't care about.
    let mut parentheses_open = false;
    let mut lines = vec![];
    let mut items_in_line = vec![];
    let mut comment_range_start = None;
    let mut comment_in_line = None;
    let mut ends_with_trailing_comma = false;
    // lex_starts_at gives us absolute ranges rather than relative ranges,
    // but (surprisingly) we still need to pass in the slice of code we want it to lex,
    // rather than the whole source file
    for pair in lexer::lex_starts_at(locator.slice(range), Mode::Expression, range.start()) {
        let (tok, subrange) = pair.ok()?;
        match tok {
            Tok::Lpar | Tok::Lsqb => {
                if parentheses_open {
                    return None;
                }
                parentheses_open = true;
            }
            Tok::Rpar | Tok::Rsqb | Tok::Newline => {
                if items_in_line.is_empty() {
                    if let Some(comment) = comment_in_line {
                        lines.push(DunderAllLine::JustAComment(LineWithJustAComment(comment)));
                    }
                } else {
                    lines.push(DunderAllLine::OneOrMoreItems(LineWithItems::new(
                        items_in_line,
                        comment_in_line,
                    )));
                }
                break;
            }
            Tok::NonLogicalNewline => {
                if items_in_line.is_empty() {
                    if let Some(comment) = comment_in_line {
                        lines.push(DunderAllLine::JustAComment(LineWithJustAComment(comment)));
                        comment_in_line = None;
                        comment_range_start = None;
                    }
                } else {
                    lines.push(DunderAllLine::OneOrMoreItems(LineWithItems::new(
                        std::mem::take(&mut items_in_line),
                        comment_in_line,
                    )));
                    comment_in_line = None;
                    comment_range_start = None;
                }
            }
            Tok::Comment(_) => {
                comment_in_line = {
                    if let Some(range_start) = comment_range_start {
                        Some(TextRange::new(range_start, subrange.end()))
                    } else {
                        Some(subrange)
                    }
                }
            }
            Tok::String { value, .. } => {
                items_in_line.push((value, subrange));
                ends_with_trailing_comma = false;
                comment_range_start = Some(subrange.end());
            }
            Tok::Comma => {
                comment_range_start = Some(subrange.end());
                ends_with_trailing_comma = true;
            }
            _ => return None,
        }
    }
    Some((lines, ends_with_trailing_comma))
}

#[derive(Debug)]
struct LineWithJustAComment(TextRange);

#[derive(Debug)]
struct LineWithItems {
    items: Vec<(String, TextRange)>,
    comment_range: Option<TextRange>,
}

impl LineWithItems {
    fn new(items: Vec<(String, TextRange)>, comment_range: Option<TextRange>) -> Self {
        assert!(
            !items.is_empty(),
            "Use the 'JustAComment' variant to represent lines with 0 items"
        );
        Self {
            items,
            comment_range,
        }
    }
}

#[derive(Debug)]
enum DunderAllLine {
    JustAComment(LineWithJustAComment),
    OneOrMoreItems(LineWithItems),
}

fn collect_dunder_all_items(
    lines: Vec<DunderAllLine>,
    dunder_all_range: TextRange,
    locator: &Locator,
) -> Vec<DunderAllItem> {
    // Given data on each line in `__all__`, group lines together into "items".
    // Each item contains exactly one element,
    // but might contain multiple comments attached to that element
    // that must move with the element when `__all__` is sorted.
    let mut all_items = Vec::with_capacity(match lines.as_slice() {
        [DunderAllLine::OneOrMoreItems(single)] => single.items.len(),
        _ => lines.len(),
    });
    let mut first_item_encountered = false;
    let mut this_range: Option<TextRange> = None;
    for line in lines {
        match line {
            DunderAllLine::JustAComment(LineWithJustAComment(comment_range)) => {
                if first_item_encountered
                    || locator.line_start(comment_range.start())
                        != locator.line_start(dunder_all_range.start())
                {
                    this_range = Some(this_range.map_or(comment_range, |range| {
                        TextRange::new(range.start(), comment_range.end())
                    }));
                }
            }
            DunderAllLine::OneOrMoreItems(LineWithItems {
                items,
                comment_range,
            }) => {
                first_item_encountered = true;
                let mut owned_items = items.into_iter();
                let (first_val, first_range) = owned_items
                    .next()
                    .expect("LineWithItems::new() should uphold the invariant that this list is always non-empty");
                let range = this_range.map_or(first_range, |r| {
                    TextRange::new(r.start(), first_range.end())
                });
                all_items.push(DunderAllItem {
                    value: first_val,
                    original_index: all_items.len(),
                    range,
                    additional_comments: comment_range,
                });
                this_range = None;
                for (value, range) in owned_items {
                    all_items.push(DunderAllItem {
                        value,
                        original_index: all_items.len(),
                        range,
                        additional_comments: None,
                    });
                }
            }
        }
    }
    all_items
}

#[derive(Clone, Debug)]
struct DunderAllItem {
    value: String,
    // Each `AllItem` in any given list should have a unique `original_index`:
    original_index: usize,
    // Note that this range might include comments, etc.
    range: TextRange,
    additional_comments: Option<TextRange>,
}

impl Ranged for DunderAllItem {
    fn range(&self) -> TextRange {
        self.range
    }
}

impl PartialEq for DunderAllItem {
    fn eq(&self, other: &Self) -> bool {
        self.original_index == other.original_index
    }
}

impl Eq for DunderAllItem {}

impl Ord for DunderAllItem {
    fn cmp(&self, other: &Self) -> Ordering {
        natord::compare(&self.value, &other.value)
            .then_with(|| self.original_index.cmp(&other.original_index))
    }
}

impl PartialOrd for DunderAllItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn join_singleline_dunder_all_items(sorted_items: &[DunderAllItem], locator: &Locator) -> String {
    sorted_items
        .iter()
        .map(|item| locator.slice(item))
        .join(", ")
}

fn join_multiline_dunder_all_items(
    sorted_items: &[DunderAllItem],
    locator: &Locator,
    parent: &ast::Stmt,
    additional_indent: &str,
    newline: &str,
    needs_trailing_comma: bool,
) -> Option<String> {
    let indent = indentation(locator, parent)?;
    let mut new_dunder_all = String::new();
    for (i, item) in sorted_items.iter().enumerate() {
        new_dunder_all.push_str(indent);
        new_dunder_all.push_str(additional_indent);
        new_dunder_all.push_str(locator.slice(item));
        let is_final_item = i == (sorted_items.len() - 1);
        if !is_final_item || needs_trailing_comma {
            new_dunder_all.push(',');
        }
        if let Some(comment) = item.additional_comments {
            new_dunder_all.push_str(locator.slice(comment));
        }
        if !is_final_item {
            new_dunder_all.push_str(newline);
        }
    }
    Some(new_dunder_all)
}
