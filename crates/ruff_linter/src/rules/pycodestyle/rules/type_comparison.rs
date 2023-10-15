use itertools::Itertools;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_none;
use ruff_python_ast::{self as ast, CmpOp, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for object type comparisons without using `isinstance()`.
///
/// ## Why is this bad?
/// Do not compare types directly.
///
/// When checking if an object is a instance of a certain type, keep in mind
/// that it might be subclassed. For example, `bool` inherits from `int`, and
/// `Exception` inherits from `BaseException`.
///
/// ## Example
/// ```python
/// if type(obj) is type(1):
///     pass
///
/// if type(obj) is int:
///     pass
/// ```
///
/// Use instead:
/// ```python
/// if isinstance(obj, int):
///     pass
/// ```
#[violation]
pub struct TypeComparison;

impl AlwaysFixableViolation for TypeComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not compare types, use `isinstance()`")
    }

    fn fix_title(&self) -> String {
        format!("Do not compare types, use `isinstance()`")
    }
}

/// E721
pub(crate) fn type_comparison(checker: &mut Checker, compare: &ast::ExprCompare) {
    for ((left, right), op) in std::iter::once(compare.left.as_ref())
        .chain(compare.comparators.iter())
        .tuple_windows()
        .zip(compare.ops.iter())
    {
        if !matches!(op, CmpOp::Is | CmpOp::IsNot | CmpOp::Eq | CmpOp::NotEq) {
            continue;
        }

        // Left-hand side must be, e.g., `type(obj)`.
        let Expr::Call(ast::ExprCall {
            func: left_func,
            arguments: left_arguments,
            ..
        }) = left
        else {
            continue;
        };

        let Expr::Name(ast::ExprName { id, .. }) = left_func.as_ref() else {
            continue;
        };

        if !(id == "type" && checker.semantic().is_builtin("type")) {
            continue;
        }

        let isinstance_prefix = match op {
            CmpOp::Eq | CmpOp::Is => String::new(),
            CmpOp::NotEq | CmpOp::IsNot => "not ".to_string(),
            _ => continue,
        };

        let left_argument = left_arguments.args.first().unwrap();

        // Right-hand side must be, e.g., `type(1)` or `int`.
        match right {
            Expr::Call(ast::ExprCall {
                func: right_func,
                arguments: right_arguments,
                ..
            }) => {
                // Ex) `type(obj) is type(1)`
                let Expr::Name(ast::ExprName { id, .. }) = right_func.as_ref() else {
                    continue;
                };

                if id == "type" && checker.semantic().is_builtin("type") {
                    let right_argument = right_arguments.args.first();

                    // Allow comparison for types which are not obvious.
                    if right_argument.is_some_and(|arg| !arg.is_name_expr() && !is_const_none(arg))
                    {
                        // find the type of argument if it resolves into a builtin type
                        let right_side: String = match right_argument.unwrap() {
                            Expr::Constant(ast::ExprConstant { value, .. }) => match value {
                                ast::Constant::Str(..) => "str".to_string(),
                                ast::Constant::Bytes(..) => "bytes".to_string(),
                                ast::Constant::Int(..) => "int".to_string(),
                                ast::Constant::Float(..) => "float".to_string(),
                                ast::Constant::Complex { .. } => "complex".to_string(),
                                ast::Constant::Bool(..) => "bool".to_string(),
                                _ => continue,
                            },
                            Expr::FString(ast::ExprFString { .. }) => "str".to_string(),
                            Expr::Tuple(ast::ExprTuple { .. }) => "tuple".to_string(),
                            Expr::List(ast::ExprList { .. }) => "list".to_string(),
                            Expr::Set(ast::ExprSet { .. }) => "set".to_string(),
                            Expr::Dict(ast::ExprDict { .. }) => "dict".to_string(),
                            Expr::DictComp(_) => "dict".to_string(),
                            Expr::BoolOp(_) => "bool".to_string(),
                            Expr::ListComp(_) => "list".to_string(),
                            Expr::SetComp(_) => "set".to_string(),
                            _ => checker.generator().expr(right),
                        };

                        let mut diagnostic = Diagnostic::new(TypeComparison, compare.range());
                        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                            isinstance_prefix
                                + &format!(
                                    "isinstance({}, {})",
                                    checker.generator().expr(left_argument),
                                    right_side
                                ),
                            compare.range(),
                        )));

                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
            Expr::Attribute(ast::ExprAttribute { value, .. }) => {
                // Ex) `type(obj) is types.NoneType`
                if checker
                    .semantic()
                    .resolve_call_path(value.as_ref())
                    .is_some_and(|call_path| matches!(call_path.as_slice(), ["types", ..]))
                {
                    let mut diagnostic = Diagnostic::new(TypeComparison, compare.range());
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        isinstance_prefix
                            + &format!(
                                "isinstance({}, {})",
                                checker.generator().expr(left_argument),
                                checker.generator().expr(right)
                            ),
                        compare.range(),
                    )));

                    checker.diagnostics.push(diagnostic);
                }
            }
            Expr::Name(ast::ExprName { id, .. }) => {
                // Ex) `type(obj) is int`
                if matches!(
                    id.as_str(),
                    "int"
                        | "str"
                        | "float"
                        | "bool"
                        | "complex"
                        | "bytes"
                        | "list"
                        | "dict"
                        | "set"
                ) && checker.semantic().is_builtin(id)
                {
                    let mut diagnostic = Diagnostic::new(TypeComparison, compare.range());
                    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                        isinstance_prefix
                            + &format!(
                                "isinstance({}, {})",
                                checker.generator().expr(left_argument),
                                id
                            ),
                        compare.range(),
                    )));

                    checker.diagnostics.push(diagnostic);
                }
            }
            _ => {}
        }
    }
}
