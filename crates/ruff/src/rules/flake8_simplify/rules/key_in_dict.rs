use ruff_diagnostics::Edit;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::node::AnyNodeRef;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, Arguments, CmpOp, Comprehension, Expr};
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for key-existence checks against `dict.keys()` calls.
///
/// ## Why is this bad?
/// When checking for the existence of a key in a given dictionary, using
/// `key in dict` is more readable and efficient than `key in dict.keys()`,
/// while having the same semantics.
///
/// ## Example
/// ```python
/// key in foo.keys()
/// ```
///
/// Use instead:
/// ```python
/// key in foo
/// ```
///
/// ## References
/// - [Python documentation: Mapping Types](https://docs.python.org/3/library/stdtypes.html#mapping-types-dict)
#[violation]
pub struct InDictKeys {
    operator: String,
}

impl AlwaysAutofixableViolation for InDictKeys {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InDictKeys { operator } = self;
        format!("Use `key {operator} dict` instead of `key {operator} dict.keys()`")
    }

    fn autofix_title(&self) -> String {
        let InDictKeys { operator: _ } = self;
        format!("Remove `.keys()`")
    }
}

/// SIM118
fn key_in_dict(
    checker: &mut Checker,
    left: &Expr,
    right: &Expr,
    operator: CmpOp,
    parent: AnyNodeRef,
) {
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, keywords, .. },
        range: _,
    }) = &right
    else {
        return;
    };
    if !(args.is_empty() && keywords.is_empty()) {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };
    if attr != "keys" {
        return;
    }

    // Ignore `self.keys()`, which will almost certainly be intentional, as in:
    // ```python
    // def __contains__(self, key: object) -> bool:
    //     return key in self.keys()
    // ```
    if value
        .as_name_expr()
        .is_some_and(|name| matches!(name.id.as_str(), "self"))
    {
        return;
    }

    // Extract the exact range of the left and right expressions.
    let left_range = parenthesized_range(left.into(), parent, checker.locator().contents())
        .unwrap_or(left.range());
    let right_range = parenthesized_range(right.into(), parent, checker.locator().contents())
        .unwrap_or(right.range());

    let mut diagnostic = Diagnostic::new(
        InDictKeys {
            operator: operator.as_str().to_string(),
        },
        TextRange::new(left_range.start(), right_range.end()),
    );
    if checker.patch(diagnostic.kind.rule()) {
        // Delete from the start of the dot to the end of the expression.
        if let Some(dot) = SimpleTokenizer::starts_at(value.end(), checker.locator().contents())
            .skip_trivia()
            .find(|token| token.kind == SimpleTokenKind::Dot)
        {
            // If the `.keys()` is followed by (e.g.) a keyword, we need to insert a space,
            // since we're removing parentheses, which could lead to invalid syntax, as in:
            // ```python
            // if key in foo.keys()and bar:
            // ```
            let range = TextRange::new(dot.start(), right.end());
            if checker
                .locator()
                .after(range.end())
                .chars()
                .next()
                .is_some_and(|char| char.is_ascii_alphabetic())
            {
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    " ".to_string(),
                    range,
                )));
            } else {
                diagnostic.set_fix(Fix::suggested(Edit::range_deletion(range)));
            }
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM118 in a `for` loop.
pub(crate) fn key_in_dict_for(checker: &mut Checker, for_stmt: &ast::StmtFor) {
    key_in_dict(
        checker,
        &for_stmt.target,
        &for_stmt.iter,
        CmpOp::In,
        for_stmt.into(),
    );
}

/// SIM118 in a comprehension.
pub(crate) fn key_in_dict_comprehension(checker: &mut Checker, comprehension: &Comprehension) {
    key_in_dict(
        checker,
        &comprehension.target,
        &comprehension.iter,
        CmpOp::In,
        comprehension.into(),
    );
}

/// SIM118 in a comparison.
pub(crate) fn key_in_dict_compare(checker: &mut Checker, compare: &ast::ExprCompare) {
    let [op] = compare.ops.as_slice() else {
        return;
    };

    if !matches!(op, CmpOp::In | CmpOp::NotIn) {
        return;
    }

    let [right] = compare.comparators.as_slice() else {
        return;
    };

    key_in_dict(checker, &compare.left, right, *op, compare.into());
}
