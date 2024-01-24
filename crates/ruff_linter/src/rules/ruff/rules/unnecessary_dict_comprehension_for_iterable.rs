use ast::ExprName;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::is_constant;
use ruff_python_ast::{self as ast, Arguments, Comprehension, Expr, ExprCall, ExprContext};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unnecessary `dict` comprehension when creating a dictionary from
/// an iterable.
///
/// ## Why is this bad?
/// It's unnecessary to use a `dict` comprehension to build a dictionary from
/// an iterable when the value is static.
///
/// Prefer `dict.fromkeys(iterable)` over `{value: None for value in iterable}`,
/// as `dict.fromkeys` is more readable and efficient.
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

impl Violation for UnnecessaryDictComprehensionForIterable {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary dict comprehension for iterable; use `dict.fromkeys` instead")
    }

    fn fix_title(&self) -> Option<String> {
        if self.is_value_none_literal {
            Some(format!("Replace with `dict.fromkeys(iterable, value)`)"))
        } else {
            Some(format!("Replace with `dict.fromkeys(iterable)`)"))
        }
    }
}

/// RUF025
pub(crate) fn unnecessary_dict_comprehension_for_iterable(
    checker: &mut Checker,
    dict_comp: &ast::ExprDictComp,
) {
    let [generator] = dict_comp.generators.as_slice() else {
        return;
    };

    // Don't suggest `dict.fromkeys` for:
    // - async generator expressions, because `dict.fromkeys` is not async.
    // - nested generator expressions, because `dict.fromkeys` might be error-prone option at least for fixing.
    // - generator expressions with `if` clauses, because `dict.fromkeys` might not be valid option.
    if !generator.ifs.is_empty() && generator.is_async {
        return;
    }

    // Don't suggest `dict.keys` if the target is not the same as the key.
    if ComparableExpr::from(&generator.target) != ComparableExpr::from(dict_comp.key.as_ref()) {
        return;
    }

    if !is_constant(dict_comp.value.as_ref()) {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryDictComprehensionForIterable {
            is_value_none_literal: dict_comp.value.is_none_literal_expr(),
        },
        dict_comp.range(),
    );

    if checker.semantic().is_builtin("dict") {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            checker
                .generator()
                .expr(&fix_unnecessary_dict_comprehension(
                    dict_comp.value.as_ref(),
                    generator,
                )),
            dict_comp.range(),
        )));
    }

    checker.diagnostics.push(diagnostic);
}

/// Generate a [`Fix`] to replace `dict` comprehension with `dict.fromkeys`.
///
/// For example:
/// - Given `{n: None for n in [1,2,3]}`, generate `dict.fromkeys([1,2,3])`.
/// - Given `{n: 1 for n in [1,2,3]}`, generate `dict.fromkeys([1,2,3], 1)`.
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
