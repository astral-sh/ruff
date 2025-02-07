use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_diagnostics::{Applicability, Edit};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::{self as ast, Arguments, CmpOp, Comprehension, Expr};
use ruff_python_semantic::analyze::typing;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

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
/// ## Fix safety
/// Given `key in obj.keys()`, `obj` _could_ be a dictionary, or it could be
/// another type that defines a `.keys()` method. In the latter case, removing
/// the `.keys()` attribute could lead to a runtime error. The fix is marked
/// as safe when the type of `obj` is known to be a dictionary; otherwise, it
/// is marked as unsafe.
///
/// ## References
/// - [Python documentation: Mapping Types](https://docs.python.org/3/library/stdtypes.html#mapping-types-dict)
#[derive(ViolationMetadata)]
pub(crate) struct InDictKeys {
    operator: String,
}

impl AlwaysFixableViolation for InDictKeys {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InDictKeys { operator } = self;
        format!("Use `key {operator} dict` instead of `key {operator} dict.keys()`")
    }

    fn fix_title(&self) -> String {
        "Remove `.keys()`".to_string()
    }
}

/// SIM118
fn key_in_dict(checker: &Checker, left: &Expr, right: &Expr, operator: CmpOp, parent: AnyNodeRef) {
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
    let left_range = parenthesized_range(
        left.into(),
        parent,
        checker.comment_ranges(),
        checker.locator().contents(),
    )
    .unwrap_or(left.range());
    let right_range = parenthesized_range(
        right.into(),
        parent,
        checker.comment_ranges(),
        checker.locator().contents(),
    )
    .unwrap_or(right.range());

    let mut diagnostic = Diagnostic::new(
        InDictKeys {
            operator: operator.as_str().to_string(),
        },
        TextRange::new(left_range.start(), right_range.end()),
    );
    // Delete from the start of the dot to the end of the expression.
    if let Some(dot) = SimpleTokenizer::starts_at(value.end(), checker.locator().contents())
        .skip_trivia()
        .find(|token| token.kind == SimpleTokenKind::Dot)
    {
        // The fix is only safe if we know the expression is a dictionary, since other types
        // can define a `.keys()` method.
        let applicability = {
            let is_dict = value.as_name_expr().is_some_and(|name| {
                let Some(binding) = checker
                    .semantic()
                    .only_binding(name)
                    .map(|id| checker.semantic().binding(id))
                else {
                    return false;
                };
                typing::is_dict(binding, checker.semantic())
            });
            if is_dict {
                Applicability::Safe
            } else {
                Applicability::Unsafe
            }
        };

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
            diagnostic.set_fix(Fix::applicable_edit(
                Edit::range_replacement(" ".to_string(), range),
                applicability,
            ));
        } else {
            diagnostic.set_fix(Fix::applicable_edit(
                Edit::range_deletion(range),
                applicability,
            ));
        }
    }
    checker.report_diagnostic(diagnostic);
}

/// SIM118 in a `for` loop.
pub(crate) fn key_in_dict_for(checker: &Checker, for_stmt: &ast::StmtFor) {
    key_in_dict(
        checker,
        &for_stmt.target,
        &for_stmt.iter,
        CmpOp::In,
        for_stmt.into(),
    );
}

/// SIM118 in a comprehension.
pub(crate) fn key_in_dict_comprehension(checker: &Checker, comprehension: &Comprehension) {
    key_in_dict(
        checker,
        &comprehension.target,
        &comprehension.iter,
        CmpOp::In,
        comprehension.into(),
    );
}

/// SIM118 in a comparison.
pub(crate) fn key_in_dict_compare(checker: &Checker, compare: &ast::ExprCompare) {
    let [op] = &*compare.ops else {
        return;
    };

    if !matches!(op, CmpOp::In | CmpOp::NotIn) {
        return;
    }

    let [right] = &*compare.comparators else {
        return;
    };

    key_in_dict(checker, &compare.left, right, *op, compare.into());
}
