use anyhow::{Context, Result};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::HashableExpr;
use ruff_python_ast::Expr;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
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
#[derive(ViolationMetadata)]
pub(crate) struct DuplicateValue {
    value: String,
    existing: String,
}

impl Violation for DuplicateValue {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateValue { value, existing } = self;
        if value == existing {
            format!("Sets should not contain duplicate item `{value}`")
        } else {
            format!(
                "Sets should not contain duplicate items, but `{existing}` and `{value}` has the same value"
            )
        }
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove duplicate item".to_string())
    }
}

/// B033
pub(crate) fn duplicate_value(checker: &Checker, set: &ast::ExprSet) {
    let mut seen_values: FxHashMap<HashableExpr, &Expr> = FxHashMap::default();
    for (index, value) in set.iter().enumerate() {
        if value.is_literal_expr() {
            if let Some(existing) = seen_values.insert(HashableExpr::from(value), value) {
                let mut diagnostic = Diagnostic::new(
                    DuplicateValue {
                        value: checker.generator().expr(value),
                        existing: checker.generator().expr(existing),
                    },
                    value.range(),
                );

                diagnostic.try_set_fix(|| {
                    remove_member(set, index, checker.locator().contents()).map(Fix::safe_edit)
                });

                checker.report_diagnostic(diagnostic);
            }
        }
    }
}

/// Remove the member at the given index from the [`ast::ExprSet`].
fn remove_member(set: &ast::ExprSet, index: usize, source: &str) -> Result<Edit> {
    if index < set.len() - 1 {
        // Case 1: the expression is _not_ the last node, so delete from the start of the
        // expression to the end of the subsequent comma.
        // Ex) Delete `"a"` in `{"a", "b", "c"}`.
        let mut tokenizer = SimpleTokenizer::starts_at(set.elts[index].end(), source);

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

        Ok(Edit::deletion(set.elts[index].start(), next.start()))
    } else if index > 0 {
        // Case 2: the expression is the last node, but not the _only_ node, so delete from the
        // start of the previous comma to the end of the expression.
        // Ex) Delete `"c"` in `{"a", "b", "c"}`.
        let mut tokenizer = SimpleTokenizer::starts_at(set.elts[index - 1].end(), source);

        // Find the trailing comma.
        let comma = tokenizer
            .find(|token| token.kind == SimpleTokenKind::Comma)
            .context("Unable to find trailing comma")?;

        Ok(Edit::deletion(comma.start(), set.elts[index].end()))
    } else {
        // Case 3: expression is the only node, so delete it.
        // Ex) Delete `"a"` in `{"a"}`.
        Ok(Edit::range_deletion(set.elts[index].range()))
    }
}
