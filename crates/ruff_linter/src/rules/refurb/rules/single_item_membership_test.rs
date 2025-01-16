use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, CmpOp, Expr, ExprStringLiteral};
use ruff_python_parser::TokenKind;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSize};
use smallvec::{smallvec, SmallVec};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for membership tests against single-item containers.
///
/// ## Why is this bad?
/// Performing a membership test against a container (like a `list` or `set`)
/// with a single item is less readable and less efficient than comparing
/// against the item directly.
///
/// ## Example
/// ```python
/// 1 in [1]
/// ```
///
/// Use instead:
/// ```python
/// 1 == 1
/// ```
///
/// ## Fix safety
///
/// When the right-hand side is a string, the fix is marked as unsafe.
/// This is because `c in "a"` is true both when `c` is `"a"` and when `c` is the empty string,
/// so the fix can change the behavior of your program in these cases.
///
/// ## References
/// - [Python documentation: Comparisons](https://docs.python.org/3/reference/expressions.html#comparisons)
/// - [Python documentation: Membership test operations](https://docs.python.org/3/reference/expressions.html#membership-test-operations)
#[derive(ViolationMetadata)]
pub(crate) struct SingleItemMembershipTest {
    membership_test: MembershipTest,
}

impl AlwaysFixableViolation for SingleItemMembershipTest {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Membership test against single-item container".to_string()
    }

    fn fix_title(&self) -> String {
        let SingleItemMembershipTest { membership_test } = self;
        match membership_test {
            MembershipTest::In { .. } => "Convert to equality test".to_string(),
            MembershipTest::NotIn { .. } => "Convert to inequality test".to_string(),
        }
    }
}

/// FURB171
pub(crate) fn single_item_membership_test(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    let ([op], [right]) = (ops, comparators) else {
        return;
    };

    let tokens = checker.tokens();
    let find_token_after = |offset: TextSize, kind: TokenKind| {
        tokens
            .after(offset)
            .iter()
            .find(|token| token.kind() == kind)
            .unwrap()
    };

    // Ensure that the comparison is a membership test.
    let membership_test = match op {
        CmpOp::In => {
            let in_token = find_token_after(left.end(), TokenKind::In);

            MembershipTest::In {
                range: in_token.range(),
            }
        }
        CmpOp::NotIn => {
            let not_token = find_token_after(left.end(), TokenKind::Not);
            let in_token = find_token_after(not_token.end(), TokenKind::In);

            MembershipTest::NotIn {
                range: TextRange::new(not_token.start(), in_token.end()),
            }
        }
        _ => return,
    };

    // Check if the right-hand side is a single-item object.
    let Some(item) = single_item(right) else {
        return;
    };

    let diagnostic = Diagnostic::new(SingleItemMembershipTest { membership_test }, expr.range());
    let fix = replace_with_comparison(membership_test, right, item, checker);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

/// Return the single item wrapped in `Some` if the expression contains a single
/// item, otherwise return `None`.
fn single_item(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::List(ast::ExprList { elts, .. })
        | Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => match elts.as_slice() {
            [Expr::Starred(_)] => None,
            [item] => Some(item),
            _ => None,
        },
        string_expr @ Expr::StringLiteral(ExprStringLiteral { value: string, .. })
            if string.chars().count() == 1 =>
        {
            Some(string_expr)
        }
        _ => None,
    }
}

fn replace_with_comparison(
    membership_test: MembershipTest,
    iterable: &Expr,
    item: &Expr,
    checker: &Checker,
) -> Fix {
    let (locator, source) = (checker.locator(), checker.source());
    let comment_ranges = checker.comment_ranges();

    let item_range = parenthesized_range(item.into(), iterable.into(), comment_ranges, source)
        .unwrap_or(item.range());
    let current_stmt_start = checker.semantic().current_statement().start();

    let replace_op = Edit::range_replacement(
        membership_test.replacement_op().to_string(),
        membership_test.range(),
    );
    let mut other_edits: SmallVec<[Edit; 2]> = smallvec![];

    let item_in_source = locator.slice(item_range);
    let replace_iterable_with_item =
        Edit::range_replacement(item_in_source.to_string(), iterable.range());

    other_edits.push(replace_iterable_with_item);

    let aggregated_comments =
        merge_to_be_removed_comments(iterable, item_range, current_stmt_start, checker);

    if !aggregated_comments.is_empty() {
        let move_comments = Edit::insertion(aggregated_comments, current_stmt_start);
        other_edits.push(move_comments);
    }

    let applicability = if iterable.is_string_literal_expr() {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Fix::applicable_edits(replace_op, other_edits, applicability)
}

fn merge_to_be_removed_comments(
    iterable: &Expr,
    item_range: TextRange,
    current_stmt_start: TextSize,
    checker: &Checker,
) -> String {
    let (locator, tokens) = (checker.locator(), checker.tokens());

    let mut aggregated_comments = String::new();

    let stmt_indentation_range =
        TextRange::new(locator.line_start(current_stmt_start), current_stmt_start);
    let stmt_indentation = locator.slice(stmt_indentation_range);
    let line_ending = checker.stylist().line_ending().to_string();

    let iterable_start_to_item_start = TextRange::new(iterable.start(), item_range.start());
    let item_end_to_iterable_end = TextRange::new(item_range.end(), iterable.end());

    tokens
        .in_range(iterable_start_to_item_start)
        .iter()
        .chain(tokens.in_range(item_end_to_iterable_end))
        .filter(|token| matches!(token.kind(), TokenKind::Comment))
        .map(|token| locator.slice(token))
        .for_each(|comment| {
            aggregated_comments.push_str(&format!("{comment}{line_ending}{stmt_indentation}"));
        });

    aggregated_comments
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MembershipTest {
    /// Ex) `1 in [1]`
    In { range: TextRange },
    /// Ex) `1 not in [1]`
    NotIn { range: TextRange },
}

impl MembershipTest {
    /// Returns the replacement comparison operator for this membership test.
    fn replacement_op(self) -> CmpOp {
        match self {
            Self::In { .. } => CmpOp::Eq,
            Self::NotIn { .. } => CmpOp::NotEq,
        }
    }
}

impl Ranged for MembershipTest {
    /// The original range of the operator
    fn range(&self) -> TextRange {
        match self {
            Self::In { range } => *range,
            Self::NotIn { range } => *range,
        }
    }
}
