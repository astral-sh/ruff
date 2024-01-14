use anyhow::{Context, Result};

use ruff_python_ast::{Expr, ExprSet};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use rustc_hash::FxHashSet;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for set literals that contain duplicate items.
///
/// ## Why is this bad?
/// In Python, sets are unordered collections of unique elements. Including a
/// duplicate item in a set literal is redundant, as the duplicate item will be
/// replaced with a single item at runtime.
///
/// ## Example
/// ```python
/// {1, 2, 3, 1}
/// ```
///
/// Use instead:
/// ```python
/// {1, 2, 3}
/// ```
#[violation]
pub struct DuplicateValue {
    value: String,
}

impl AlwaysFixableViolation for DuplicateValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateValue { value } = self;
        format!("Sets should not contain duplicate item `{value}`")
    }

    fn fix_title(&self) -> String {
        let DuplicateValue { value } = self;
        format!("Remove duplicate item `{value}`")
    }
}

/// B033
pub(crate) fn duplicate_value(checker: &mut Checker, expr: &Expr) {
    let Expr::Set(ExprSet { elts, .. }) = expr else {
        return;
    };

    let mut seen_values: FxHashSet<ComparableExpr> = FxHashSet::default();
    let mut duplicate_indices: Vec<usize> = Vec::new();
    let mut unique_indices: Vec<usize> = Vec::new();

    for (index, elt) in elts.iter().enumerate() {
        if elt.is_literal_expr() {
            let comparable_value: ComparableExpr = elt.into();

            if seen_values.insert(comparable_value) {
                unique_indices.push(index);
            } else {
                duplicate_indices.push(index);
            }
        } else {
            unique_indices.push(index);
        }
    }

    for index in duplicate_indices {
        let elt = &elts[index];

        let mut diagnostic = Diagnostic::new(
            DuplicateValue {
                value: checker.generator().expr(elt),
            },
            elt.range(),
        );

        diagnostic.try_set_fix(|| {
            remove_member(elt, elts, checker.locator().contents()).map(Fix::safe_edit)
        });

        checker.diagnostics.push(diagnostic);
    }
}

fn remove_member(expr: &Expr, elts: &[Expr], source: &str) -> Result<Edit> {
    let (before, after): (Vec<_>, Vec<_>) = elts
        .iter()
        .map(Ranged::range)
        .filter(|range| expr.range() != *range)
        .partition(|range| range.start() < expr.start());

    if !after.is_empty() {
        // Case 1: expr is _not_ the last node, so delete from the start of the
        // expr to the end of the subsequent comma.
        let mut tokenizer = SimpleTokenizer::starts_at(expr.end(), source);

        // Find the trailing comma.
        tokenizer
            .find(|token| token.kind == SimpleTokenKind::Comma)
            .context("Unable to find trailing comma")?;

        // Find the next non-whitespace token.
        let next = tokenizer
            .find(|token| {
                token.kind != SimpleTokenKind::Whitespace && token.kind != SimpleTokenKind::Newline
            })
            .context("Unable to find next token")?;

        Ok(Edit::deletion(expr.start(), next.start()))
    } else if let Some(previous) = before.iter().map(Ranged::end).max() {
        // Case 2: expr is the last node, so delete from the start of the
        // previous comma to the end of the expr.
        let mut tokenizer = SimpleTokenizer::starts_at(previous, source);

        // Find the trailing comma.
        let comma = tokenizer
            .find(|token| token.kind == SimpleTokenKind::Comma)
            .context("Unable to find trailing comma")?;

        Ok(Edit::deletion(comma.start(), expr.end()))
    } else {
        // Case 3: expr is the only node, so delete it
        Ok(Edit::range_deletion(expr.range()))
    }
}
