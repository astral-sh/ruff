use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};

use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::is_known_dunder_method;

/// ## What it does
/// Checks for explicit use of dunder methods.
///
/// ## Why is this bad?
/// Dunder names are not meant to be called explicitly.
///
/// ## Example
/// ```python
/// three = (3.0).__str__()
/// twelve = "1".__add__("2")
///
///
/// def is_bigger_than_two(x: int) -> bool:
///     return x.__gt__(2)
/// ```
///
/// Use instead:
/// ```python
/// three = str(3.0)
/// twelve = "1" + "2"
///
///
/// def is_bigger_than_two(x: int) -> bool:
///     return x > 2
/// ```
///
#[violation]
pub struct UnnecessaryDunderCall {
    call: String,
}

impl Violation for UnnecessaryDunderCall {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryDunderCall { call } = self;
        format!("Unnecessary dunder call `{call}`")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Remove unnecessary dunder call"))
    }
}

fn get_operator(dunder_method: &str) -> Option<&str> {
    match dunder_method {
        "__add__" => Some("+"),
        "__and__" => Some("&"),
        "__contains__" => Some("in"),
        "__eq__" => Some("=="),
        "__floordiv__" => Some("//"),
        "__ge__" => Some(">="),
        "__gt__" => Some(">"),
        "__iadd__" => Some("+="),
        "__iand__" => Some("&="),
        "__ifloordiv__" => Some("//="),
        "__ilshift__" => Some("<<="),
        "__imod__" => Some("%="),
        "__imul__" => Some("*="),
        "__ior__" => Some("|="),
        "__ipow__" => Some("**="),
        "__irshift__" => Some(">>="),
        "__isub__" => Some("-="),
        "__itruediv__" => Some("/="),
        "__ixor__" => Some("^="),
        "__le__" => Some("<="),
        "__lshift__" => Some("<<"),
        "__lt__" => Some("<"),
        "__mod__" => Some("%"),
        "__mul__" => Some("*"),
        "__ne__" => Some("!="),
        "__or__" => Some("|"),
        "__rshift__" => Some(">>"),
        "__sub__" => Some("-"),
        "__truediv__" => Some("/"),
        "__xor__" => Some("^"),
        _ => None,
    }
}

fn get_r_operator(dunder_method: &str) -> Option<&str> {
    match dunder_method {
        "__radd__" => Some("+"),
        "__rand__" => Some("&"),
        "__rfloordiv__" => Some("//"),
        "__rlshift__" => Some("<<"),
        "__rmod__" => Some("%"),
        "__rmul__" => Some("*"),
        "__ror__" => Some("|"),
        "__rrshift__" => Some(">>"),
        "__rsub__" => Some("-"),
        "__rtruediv__" => Some("/"),
        "__rxor__" => Some("^"),
        _ => None,
    }
}

fn get_builtin(dunder_method: &str) -> Option<&str> {
    match dunder_method {
        "__abs__" => Some("abs"),
        "__bool__" => Some("bool"),
        "__bytes__" => Some("bytes"),
        "__complex__" => Some("complex"),
        "__dir__" => Some("dir"),
        "__float__" => Some("float"),
        "__hash__" => Some("hash"),
        "__int__" => Some("int"),
        "__iter__" => Some("iter"),
        "__len__" => Some("len"),
        "__next__" => Some("next"),
        "__repr__" => Some("repr"),
        "__reversed__" => Some("reversed"),
        "__round__" => Some("round"),
        "__str__" => Some("str"),
        _ => None,
    }
}

/// PLC2801
pub(crate) fn unnecessary_dunder_call(checker: &mut Checker, expr: &Expr) {
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = expr
    else {
        return;
    };

    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return;
    };

    if !is_known_dunder_method(attr) {
        return;
    }

    let mut fixed: Option<String> = None;

    match arguments.args.len() {
        0 => {
            if let Some(builtin) = get_builtin(attr) {
                if !checker.semantic().is_builtin(builtin) {
                    // duck out if the builtin was shadowed
                    return;
                }

                fixed = Some(format!("{}({})", builtin, checker.generator().expr(value)));
            }
        }
        1 => {
            if let Some(operator) = get_operator(attr) {
                fixed = Some(format!(
                    "{} {} {}",
                    checker.generator().expr(value),
                    operator,
                    checker.generator().expr(arguments.args.first().unwrap()),
                ));
            } else if let Some(operator) = get_r_operator(attr) {
                fixed = Some(format!(
                    "{} {} {}",
                    checker.generator().expr(arguments.args.first().unwrap()),
                    operator,
                    checker.generator().expr(value),
                ));
            }
        }
        _ => {}
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryDunderCall {
            call: checker.generator().expr(expr),
        },
        expr.range(),
    );

    if let Some(fixed) = fixed {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(fixed, expr.range())));
    };

    checker.diagnostics.push(diagnostic);
}
