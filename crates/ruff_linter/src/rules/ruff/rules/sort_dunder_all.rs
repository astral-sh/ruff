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

/// ## What it does
/// Checks for `__all__` definitions that are not alphabetically sorted.
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
        format!("`__all__` is not alphabetically sorted")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Sort `__all__` alphabetically".to_string())
    }
}

pub(crate) fn sort_dunder_all_assign(
    checker: &mut Checker,
    node: &ast::StmtAssign,
    parent: &ast::Stmt,
) {
    let ast::StmtAssign { value, targets, .. } = node;
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

    let Some(dunder_all_val) = DunderAllValue::from_expr(node, locator) else {
        return;
    };

    let new_dunder_all =
        match dunder_all_val.construct_sorted_all(locator, parent, checker.stylist()) {
            SortedDunderAll::AlreadySorted => return,
            SortedDunderAll::Sorted(value) => value,
        };

    let mut diagnostic = Diagnostic::new(UnsortedDunderAll, dunder_all_val.range());

    if let Some(new_dunder_all) = new_dunder_all {
        let applicability = {
            if dunder_all_val.multiline
                && checker.indexer().comment_ranges().intersects(node.range())
            {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            }
        };
        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(new_dunder_all, dunder_all_val.range()),
            applicability,
        ));
    }

    checker.diagnostics.push(diagnostic);
}

struct DunderAllValue {
    items: Vec<DunderAllItem>,
    range: TextRange,
    multiline: bool,
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
        
        // An `__all__` definition with <2 elements can't be unsorted;
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
        let lines = collect_dunder_all_lines(*range, locator)?;

        // (2b). Group lines together into sortable "items":
        //   - Any "item" contains a single element of the `__all__` list/tuple
        //   - "Items" are ordered according to the element they contain
        //   - Assume that any comments on their own line are meant to be grouped
        //     with the element immediately below them: if the element moves,
        //     the comments above the element move with it.
        //   - The same goes for any comments on the same line as an element:
        //     if the element moves, the comment moves with it.
        let items = collect_dunder_all_items(&lines);

        Some(DunderAllValue {
            items,
            range: *range,
            multiline: is_multiline,
        })
    }

    fn construct_sorted_all(
        &self,
        locator: &Locator,
        parent: &ast::Stmt,
        stylist: &Stylist,
    ) -> SortedDunderAll {
        let mut sorted_items = self.items.clone();
        sorted_items.sort();

        // As well as saving us unnecessary work,
        // returning early here also means that we can rely on the invariant
        // throughout the rest of this function that both `items` and `sorted_items`
        // have length of at least two. If there are fewer than two items in `__all__`,
        // it is impossible for them *not* to compare equal here:
        if sorted_items == self.items {
            return SortedDunderAll::AlreadySorted;
        }
        assert!(self.items.len() >= 2);

        // As well as the "items" in the `__all__` definition,
        // there is also a "prelude" and a "postlude":
        //  - Prelude == the region of source code from the opening parenthesis
        //    (if there was one), up to the start of the first element in `__all__`.
        //  - Postlude == the region of source code from the end of the last
        //    element in `__all__` up to and including the closing parenthesis
        //    (if there was one).
        let prelude_end = {
            // We should already have returned by now if there are 0 items:
            // see earlier comments in this function
            let first_item = self
                .items
                .first()
                .expect("Expected there to be at least two items in the list");
            let first_item_line_offset = locator.line_start(first_item.start());
            if first_item_line_offset == locator.line_start(self.start()) {
                first_item.start()
            } else {
                first_item_line_offset
            }
        };
        let (needs_trailing_comma, postlude_start) = {
            // We should already have returned by now if there are 0 items:
            // see earlier comments in this function
            let last_item = self
                .items
                .last()
                .expect("Expected there to be at least two items in the list");
            let last_item_line_offset = locator.line_end(last_item.end());
            if last_item_line_offset == locator.line_end(self.end()) {
                (false, last_item.end())
            } else {
                (true, last_item_line_offset)
            }
        };
        let mut prelude = locator
            .slice(TextRange::new(self.start(), prelude_end))
            .to_string();
        let postlude = locator.slice(TextRange::new(postlude_start, self.end()));

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
                needs_trailing_comma,
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

fn collect_dunder_all_lines(range: TextRange, locator: &Locator) -> Option<Vec<DunderAllLine>> {
    // Collect data on each line of `__all__`.
    // Return `None` if `__all__` appears to be invalid,
    // or if it's an edge case we don't care about.
    let mut parentheses_open = false;
    let mut lines = vec![];
    let mut items_in_line = vec![];
    let mut comment_in_line = None;
    for pair in lexer::lex(locator.slice(range).trim(), Mode::Expression) {
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
                        lines.push(DunderAllLine::JustAComment(LineWithJustAComment::new(
                            comment, range,
                        )));
                    }
                } else {
                    lines.push(DunderAllLine::OneOrMoreItems(LineWithItems::new(
                        &items_in_line,
                        comment_in_line,
                        range,
                    )));
                }
                break;
            }
            Tok::NonLogicalNewline => {
                if items_in_line.is_empty() {
                    if let Some(comment) = comment_in_line {
                        lines.push(DunderAllLine::JustAComment(LineWithJustAComment::new(
                            comment, range,
                        )));
                        comment_in_line = None;
                    }
                } else {
                    lines.push(DunderAllLine::OneOrMoreItems(LineWithItems::new(
                        &items_in_line,
                        comment_in_line,
                        range,
                    )));
                    comment_in_line = None;
                    items_in_line.clear();
                }
            }
            Tok::Comment(_) => comment_in_line = Some(subrange),
            Tok::String { value, .. } => items_in_line.push((value, subrange)),
            Tok::Comma => continue,
            _ => return None,
        }
    }
    Some(lines)
}

#[derive(Debug)]
struct LineWithJustAComment(TextRange);

impl LineWithJustAComment {
    fn new(comment_range: TextRange, total_dunder_all_range: TextRange) -> Self {
        Self(comment_range + total_dunder_all_range.start())
    }
}

#[derive(Debug)]
struct LineWithItems {
    items: Vec<(String, TextRange)>,
    comment_range: Option<TextRange>,
}

impl LineWithItems {
    fn new(
        items: &[(String, TextRange)],
        comment_range: Option<TextRange>,
        total_dunder_all_range: TextRange,
    ) -> Self {
        assert!(
            !items.is_empty(),
            "Use the 'JustAComment' variant to represent lines with 0 items"
        );
        let offset = total_dunder_all_range.start();
        Self {
            items: items
                .iter()
                .map(|(s, r)| (s.to_owned(), r + offset))
                .collect(),
            comment_range: comment_range.map(|c| c + offset),
        }
    }
}

#[derive(Debug)]
enum DunderAllLine {
    JustAComment(LineWithJustAComment),
    OneOrMoreItems(LineWithItems),
}

fn collect_dunder_all_items(lines: &[DunderAllLine]) -> Vec<DunderAllItem> {
    // Given data on each line in `__all__`, group lines together into "items".
    // Each item contains exactly one element,
    // but might contain multiple comments attached to that element
    // that must move with the element when `__all__` is sorted.
    let mut all_items = vec![];
    let mut this_range = None;
    for line in lines {
        match line {
            DunderAllLine::JustAComment(LineWithJustAComment(comment_range)) => {
                this_range = Some(*comment_range);
            }
            DunderAllLine::OneOrMoreItems(LineWithItems {
                items,
                comment_range,
            }) => {
                let [(first_val, first_range), rest @ ..] = items.as_slice() else {
                    unreachable!(
                        "LineWithItems::new() should uphold the invariant that this list is always non-empty"
                    )
                };
                let range = this_range.map_or(*first_range, |r| {
                    TextRange::new(r.start(), first_range.end())
                });
                all_items.push(DunderAllItem {
                    value: first_val.clone(),
                    original_index: all_items.len(),
                    range,
                    additional_comments: *comment_range,
                });
                this_range = None;
                all_items.extend(rest.map(|(value, range)| DunderAllItem {
                    value,
                    original_index: all_items.len(),
                    range,
                    additional_comments: None,
                }));
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

impl DunderAllItem {
    fn sort_index(&self) -> (&str, usize) {
        (&self.value, self.original_index)
    }
}

impl Ord for DunderAllItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_index().cmp(&other.sort_index())
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
            new_dunder_all.push_str("  ");
            new_dunder_all.push_str(locator.slice(comment));
        }
        if !is_final_item {
            new_dunder_all.push_str(newline);
        }
    }
    Some(new_dunder_all)
}
