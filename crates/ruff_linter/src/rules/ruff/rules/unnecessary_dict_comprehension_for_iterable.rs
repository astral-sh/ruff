use ast::ExprName;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Arguments, Comprehension, Expr, ExprCall, ExprContext};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary `dict` comprehension when creating a new dictionary from iterable.
///
/// ## Why is this bad?
/// It's unnecessary to use a `dict` comprehension to build a dictionary from an iterable when the value is `static`.
/// Use `dict.fromkeys(iterable)` instead of `{value: None for value in iterable}`.
///
/// `dict.fromkeys(iterable)` is more readable and faster than `{value: None for value in iterable}`.
///
///
/// ## Examples
/// ```python
/// {a: None for a in iterable}
/// {a: 1 for a in iterable}
/// ```
///
/// Use instead:
/// ```python
/// dict.fromkeys(iterable)
/// dict.fromkeys(iterable, 1)
/// ```
#[violation]
pub struct UnnecessaryDictComprehensionForIterable {
    is_value_none_literal: bool,
}

impl AlwaysFixableViolation for UnnecessaryDictComprehensionForIterable {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.is_value_none_literal {
            format!(
                "Unnecessary dict comprehension for iterable (rewrite using `dict.fromkeys(iterable)`)"
            )
        } else {
            format!(
                "Unnecessary dict comprehension for iterable (rewrite using `dict.fromkeys(iterable, value)`)"
            )
        }
    }

    fn fix_title(&self) -> String {
        if self.is_value_none_literal {
            format!("Rewrite using `dict.fromkeys(iterable, value)`)",)
        } else {
            format!("Rewrite using `dict.fromkeys(iterable)`)")
        }
    }
}

/// RUF025
pub(crate) fn unnecessary_dict_comprehension_for_iterable(
    checker: &mut Checker,
    expr: &Expr,
    key: &Expr,
    value: &Expr,
    generators: &[Comprehension],
) {
    let [generator] = generators else {
        return;
    };

    // Don't suggest `dict.fromkeys` for:
    // - async generator expressions, because `dict.fromkeys` is not async.
    // - nested generator expressions, because `dict.fromkeys` might be error-prone option at least for fixing.
    // - generator expressions with `if` clauses, because `dict.fromkeys` might not be valid option.
    if !generator.ifs.is_empty() && generator.is_async {
        return;
    }

    if !generator.target.is_name_expr() {
        return;
    };

    // Don't suggest `dict.fromkeys` if key and value are binded to the same name.
    if let (Expr::Name(key_name), Expr::Name(value_name)) = (key, value) {
        if key_name.id == value_name.id {
            return;
        }
    }

    if !has_valid_expression_type(value) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryDictComprehensionForIterable {
            is_value_none_literal: value.is_none_literal_expr(),
        },
        expr.range(),
    );

    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        checker
            .generator()
            .expr(&fix_unnecessary_dict_comprehension(value, generator)),
        expr.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

// only accept `None`, `Ellipsis`, `True`, `False`, `Number`, `String`, `Bytes`, `Name` as value
fn has_valid_expression_type(node: &ast::Expr) -> bool {
    matches!(
        node,
        ast::Expr::StringLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::NoneLiteral(_)
            | ast::Expr::EllipsisLiteral(_)
            | ast::Expr::Name(_)
    )
}

/// Generate a [`Fix`] to replace `dict` comprehension with `dict.fromkeys`.
/// (RUF025) Convert `{n: None for n in [1,2,3]}` to `dict.fromkeys([1,2,3])` or
/// `{n: 1 for n in [1,2,3]}` to `dict.fromkeys([1,2,3], 1)`.
fn fix_unnecessary_dict_comprehension(value: &Expr, generator: &Comprehension) -> Expr {
    let iterable = generator.iter.clone();
    let args = Arguments {
        args: if value.is_none_literal_expr() {
            vec![iterable]
        } else {
            vec![iterable, value.clone()]
        },
        keywords: vec![],
        range: TextRange::default(),
    };
    Expr::Call(ExprCall {
        func: Box::new(Expr::Name(ExprName {
            id: "dict.fromkeys".into(),
            ctx: ExprContext::Load,
            range: TextRange::default(),
        })),
        arguments: args,
        range: TextRange::default(),
    })
}
