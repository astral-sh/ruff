use ast::ExprName;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::any_over_expr;
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
    if !generator.ifs.is_empty() {
        return;
    }
    if generator.is_async {
        return;
    }

    // Don't suggest `dict.keys` if the target is not the same as the key.
    if ComparableExpr::from(&generator.target) != ComparableExpr::from(dict_comp.key.as_ref()) {
        return;
    }

    // Don't suggest `dict.fromkeys` if the value is not a constant or constant-like.
    if !is_constant_like(dict_comp.value.as_ref()) {
        return;
    }

    // Don't suggest `dict.fromkeys` if any of the expressions in the value are defined within
    // the comprehension (e.g., by the target).
    let self_referential = any_over_expr(dict_comp.value.as_ref(), &|expr| {
        let Expr::Name(name) = expr else {
            return false;
        };

        let Some(id) = checker.semantic().resolve_name(name) else {
            return false;
        };

        let binding = checker.semantic().binding(id);

        dict_comp.range().contains_range(binding.range())
    });
    if self_referential {
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

/// Returns `true` if the expression can be shared across multiple values.
///
/// When converting from `{key: value for key in iterable}` to `dict.fromkeys(iterable, value)`,
/// the `value` is shared across all values without being evaluated multiple times. If the value
/// contains, e.g., a function call, it cannot be shared, as the function might have side effects.
/// Similarly, if the value contains a list comprehension, it cannot be shared, as `dict.fromkeys`
/// would leave each value with a reference to the same list.
fn is_constant_like(expr: &Expr) -> bool {
    !any_over_expr(expr, &|expr| {
        matches!(
            expr,
            Expr::Lambda(_)
                | Expr::List(_)
                | Expr::Dict(_)
                | Expr::Set(_)
                | Expr::ListComp(_)
                | Expr::SetComp(_)
                | Expr::DictComp(_)
                | Expr::GeneratorExp(_)
                | Expr::Await(_)
                | Expr::Yield(_)
                | Expr::YieldFrom(_)
                | Expr::Call(_)
                | Expr::NamedExpr(_)
        )
    })
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
            Box::from([iterable])
        } else {
            Box::from([iterable, value.clone()])
        },
        keywords: Box::from([]),
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
