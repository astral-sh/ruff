use std::cmp::Ordering;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::whitespace::indentation;
use ruff_python_codegen::Stylist;
use ruff_python_parser::{lexer, Mode, Tok};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;

use itertools::Itertools;
use strum_macros::EnumIs;

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

#[derive(EnumIs, Eq, PartialEq)]
enum DunderAllKind {
    List,
    Tuple { is_parenthesized: bool },
}

fn tuple_is_parenthesized(tuple: &ast::ExprTuple, locator: &Locator) -> bool {
    let toks = lexer::lex(locator.slice(tuple).trim(), Mode::Expression).collect::<Vec<_>>();
    matches!(
        (toks.first(), toks.get(toks.len() - 2)),
        (Some(Ok((Tok::Lpar, _))), Some(Ok((Tok::Rpar, _))))
    )
}

#[derive(Clone, Debug)]
struct DunderAllItem {
    value: String,
    // Each `AllItem` in any given list should have a unique `original_index`:
    original_index: u16,
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
    fn sort_index(&self) -> (&str, u16) {
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

fn collect_dunder_all_items(lines: Vec<DunderAllLine>) -> Vec<DunderAllItem> {
    let mut all_items = vec![];

    let mut this_range = None;
    let mut idx = 0;
    for line in lines {
        let DunderAllLine { items, comment } = line;
        match (items.as_slice(), comment) {
            ([], Some(_)) => {
                this_range = comment;
            }
            ([(first_val, first_range), rest @ ..], _) => {
                let range = this_range.map_or(*first_range, |r| {
                    TextRange::new(r.start(), first_range.end())
                });
                all_items.push(DunderAllItem {
                    value: first_val.clone(),
                    original_index: idx,
                    range,
                    additional_comments: comment,
                });
                this_range = None;
                idx += 1;
                for (value, range) in rest {
                    all_items.push(DunderAllItem {
                        value: value.clone(),
                        original_index: idx,
                        range: *range,
                        additional_comments: None,
                    });
                    idx += 1;
                }
            }
            _ => unreachable!(
                "This should be unreachable.
                Any lines that have neither comments nor items
                should have been filtered out by this point."
            ),
        }
    }

    all_items
}

#[derive(Debug)]
struct DunderAllLine {
    items: Vec<(String, TextRange)>,
    comment: Option<TextRange>,
}

impl DunderAllLine {
    fn new(items: &[(String, TextRange)], comment: Option<TextRange>, offset: TextSize) -> Self {
        assert!(comment.is_some() || !items.is_empty());
        Self {
            items: items
                .iter()
                .map(|(s, r)| (s.to_owned(), r + offset))
                .collect(),
            comment: comment.map(|c| c + offset),
        }
    }
}

fn collect_dunder_all_lines(range: TextRange, locator: &Locator) -> Option<Vec<DunderAllLine>> {
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
                if !(items_in_line.is_empty() && comment_in_line.is_none()) {
                    lines.push(DunderAllLine::new(
                        &items_in_line,
                        comment_in_line,
                        range.start(),
                    ));
                }
                break;
            }
            Tok::NonLogicalNewline => {
                if !(items_in_line.is_empty() && comment_in_line.is_none()) {
                    lines.push(DunderAllLine::new(
                        &items_in_line,
                        comment_in_line,
                        range.start(),
                    ));
                    items_in_line = vec![];
                    comment_in_line = None;
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

#[derive(PartialEq, Eq)]
struct DunderAllValue<'a> {
    kind: DunderAllKind,
    items: Vec<DunderAllItem>,
    range: &'a TextRange,
    ctx: &'a ast::ExprContext,
    multiline: bool,
}

struct SortedDunderAll {
    was_already_sorted: bool,
    new_dunder_all: Option<String>,
}

fn join_multiline_dunder_all_items(
    sorted_items: &[DunderAllItem],
    locator: &Locator,
    parent: &ast::Stmt,
    additional_indent: &str,
    newline: &str,
) -> Option<String> {
    let Some(indent) = indentation(locator, parent) else {
        return None;
    };

    let mut new_dunder_all = String::new();
    for (i, item) in sorted_items.iter().enumerate() {
        new_dunder_all.push_str(indent);
        new_dunder_all.push_str(additional_indent);
        new_dunder_all.push_str(locator.slice(item));
        new_dunder_all.push(',');
        if let Some(comment) = item.additional_comments {
            new_dunder_all.push_str("  ");
            new_dunder_all.push_str(locator.slice(comment));
        }
        if i < (sorted_items.len() - 1) {
            new_dunder_all.push_str(newline);
        }
    }

    Some(new_dunder_all)
}

fn join_singleline_dunder_all_items(sorted_items: &[DunderAllItem], locator: &Locator) -> String {
    sorted_items
        .iter()
        .map(|item| locator.slice(item))
        .join(", ")
}

impl<'a> DunderAllValue<'a> {
    fn from_expr(value: &'a ast::Expr, locator: &Locator) -> Option<DunderAllValue<'a>> {
        let is_multiline = locator.contains_line_break(value.range());
        let (kind, elts, range, ctx) = match value {
            ast::Expr::List(ast::ExprList { elts, range, ctx }) => {
                (DunderAllKind::List, elts, range, ctx)
            }
            ast::Expr::Tuple(tuple @ ast::ExprTuple { elts, range, ctx }) => {
                let is_parenthesized = tuple_is_parenthesized(tuple, locator);
                (DunderAllKind::Tuple { is_parenthesized }, elts, range, ctx)
            }
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
        let lines = collect_dunder_all_lines(*range, locator)?;
        let items = collect_dunder_all_items(lines);
        Some(DunderAllValue {
            kind,
            items,
            range,
            ctx,
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
        if sorted_items == self.items {
            return SortedDunderAll {
                was_already_sorted: true,
                new_dunder_all: None,
            };
        }
        let prelude_end = {
            if let Some(first_item) = self.items.first() {
                let first_item_line_offset = locator.line_start(first_item.start());
                if first_item_line_offset == locator.line_start(self.start()) {
                    first_item.start()
                } else {
                    first_item_line_offset
                }
            } else {
                self.start() + TextSize::new(1)
            }
        };
        let postlude_start = {
            if let Some(last_item) = self.items.last() {
                let last_item_line_offset = locator.line_end(last_item.end());
                if last_item_line_offset == locator.line_end(self.end()) {
                    last_item.end()
                } else {
                    last_item_line_offset
                }
            } else {
                self.end() - TextSize::new(1)
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
            join_multiline_dunder_all_items(&sorted_items, locator, parent, indentation, newline)
        } else {
            Some(join_singleline_dunder_all_items(&sorted_items, locator))
        };

        let new_dunder_all = joined_items.map(|items| format!("{prelude}{items}{postlude}"));
        SortedDunderAll {
            was_already_sorted: false,
            new_dunder_all,
        }
    }
}

impl Ranged for DunderAllValue<'_> {
    fn range(&self) -> TextRange {
        *self.range
    }
}

pub(crate) fn sort_dunder_all(checker: &mut Checker, stmt: &ast::Stmt) {
    // We're only interested in `__all__` in the global scope
    if !checker.semantic().current_scope().kind.is_module() {
        return;
    }

    // We're only interested in `__all__ = ...` and `__all__ += ...`
    let (target, original_value) = match stmt {
        ast::Stmt::Assign(ast::StmtAssign { value, targets, .. }) => match targets.as_slice() {
            [ast::Expr::Name(ast::ExprName { id, .. })] => (id, value.as_ref()),
            _ => return,
        },
        ast::Stmt::AugAssign(ast::StmtAugAssign {
            value,
            target,
            op: ast::Operator::Add,
            ..
        }) => match target.as_ref() {
            ast::Expr::Name(ast::ExprName { id, .. }) => (id, value.as_ref()),
            _ => return,
        },
        _ => return,
    };

    if target != "__all__" {
        return;
    }

    let locator = checker.locator();

    let Some(dunder_all_val) = DunderAllValue::from_expr(original_value, locator) else {
        return;
    };

    let sorting_result = dunder_all_val.construct_sorted_all(locator, stmt, checker.stylist());

    if sorting_result.was_already_sorted {
        return;
    }

    let dunder_all_range = dunder_all_val.range();
    let mut diagnostic = Diagnostic::new(UnsortedDunderAll, dunder_all_range);

    if let Some(new_dunder_all) = sorting_result.new_dunder_all {
        let applicability = {
            if dunder_all_val.multiline {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            }
        };
        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(new_dunder_all, dunder_all_range),
            applicability,
        ));
    }

    checker.diagnostics.push(diagnostic);
}
