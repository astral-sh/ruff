use anyhow::{bail, Result};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for lambda expressions and function definitions that can be replaced with a function
/// from `operator` module.
///
/// ## Why is this bad?
/// Using functions from `operator` module is more concise and readable.
///
/// ## Example
/// ```python
/// import functools
/// nums = [1, 2, 3]
/// sum = functools.reduce(lambda x, y: x + y, nums)
/// ```
///
/// Use instead:
/// ```python
/// import functools
/// import operator
/// nums = [1, 2, 3]
/// sum = functools.reduce(operator.add, nums)
/// ```
///
/// ## References
#[violation]
pub struct ReimplementedOperator {
    target: &'static str,
    operator: &'static str,
}

impl Violation for ReimplementedOperator {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ReimplementedOperator { operator, target } = self;
        format!("Use `operator.{operator}` instead of defining a {target}")
    }

    fn fix_title(&self) -> Option<String> {
        let ReimplementedOperator { operator, .. } = self;
        Some(format!("Replace with `operator.{operator}`"))
    }
}

/// FURB118
pub(crate) fn reimplemented_operator(checker: &mut Checker, target: &LambdaLike) {
    let Some(params) = target.parameters() else {
        return;
    };
    let Some(body) = target.body() else { return };
    let Some(operator) = get_operator(body, params) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(
        ReimplementedOperator {
            operator,
            target: target.kind(),
        },
        target.range(),
    );
    diagnostic.try_set_fix(|| target.try_fix(checker, operator));
    checker.diagnostics.push(diagnostic);
}

/// Candidate for lambda expression or function definition consisting of a return statement.
pub(crate) enum LambdaLike<'a> {
    Lambda(&'a ast::ExprLambda),
    Function(&'a ast::StmtFunctionDef),
}

impl<'a> From<&'a ast::ExprLambda> for LambdaLike<'a> {
    fn from(lambda: &'a ast::ExprLambda) -> Self {
        Self::Lambda(lambda)
    }
}

impl<'a> From<&'a ast::StmtFunctionDef> for LambdaLike<'a> {
    fn from(function: &'a ast::StmtFunctionDef) -> Self {
        Self::Function(function)
    }
}

impl Ranged for LambdaLike<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Lambda(expr) => expr.range(),
            Self::Function(stmt) => stmt.range(),
        }
    }
}

impl LambdaLike<'_> {
    fn parameters(&self) -> Option<&ast::Parameters> {
        match self {
            Self::Lambda(expr) => expr.parameters.as_deref(),
            Self::Function(stmt) => Some(&stmt.parameters),
        }
    }

    fn body(&self) -> Option<&Expr> {
        match self {
            Self::Lambda(expr) => Some(&expr.body),
            Self::Function(stmt) => match stmt.body.as_slice() {
                [Stmt::Return(ast::StmtReturn { value, .. })] => value.as_deref(),
                _ => None,
            },
        }
    }

    fn try_fix(&self, checker: &Checker, operator: &'static str) -> Result<Fix> {
        match self {
            Self::Lambda(_) => {
                let (edit, binding) = checker.importer().get_or_import_symbol(
                    &ImportRequest::import("operator", operator),
                    self.start(),
                    checker.semantic(),
                )?;
                Ok(Fix::safe_edits(
                    Edit::range_replacement(binding, self.range()),
                    [edit],
                ))
            }
            Self::Function(_) => bail!("No fix available"),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Lambda(_) => "lambda",
            Self::Function(_) => "function",
        }
    }
}

fn get_operator(expr: &Expr, params: &ast::Parameters) -> Option<&'static str> {
    match expr {
        Expr::UnaryOp(expr) => unary_op(expr, params),
        Expr::BinOp(expr) => bin_op(expr, params),
        Expr::Compare(expr) => cmp_op(expr, params),
        _ => None,
    }
}

fn unary_op(expr: &ast::ExprUnaryOp, params: &ast::Parameters) -> Option<&'static str> {
    let [arg] = params.args.as_slice() else {
        return None;
    };
    if !is_same(arg, &expr.operand) {
        return None;
    }
    Some(match expr.op {
        ast::UnaryOp::Invert => "invert",
        ast::UnaryOp::Not => "not_",
        ast::UnaryOp::UAdd => "pos",
        ast::UnaryOp::USub => "neg",
    })
}

fn bin_op(expr: &ast::ExprBinOp, params: &ast::Parameters) -> Option<&'static str> {
    let [arg1, arg2] = params.args.as_slice() else {
        return None;
    };
    if !is_same(arg1, &expr.left) || !is_same(arg2, &expr.right) {
        return None;
    }
    Some(match expr.op {
        ast::Operator::Add => "add",
        ast::Operator::Sub => "sub",
        ast::Operator::Mult => "mul",
        ast::Operator::MatMult => "matmul",
        ast::Operator::Div => "truediv",
        ast::Operator::Mod => "mod",
        ast::Operator::Pow => "pow",
        ast::Operator::LShift => "lshift",
        ast::Operator::RShift => "rshift",
        ast::Operator::BitOr => "or_",
        ast::Operator::BitXor => "xor",
        ast::Operator::BitAnd => "and_",
        ast::Operator::FloorDiv => "floordiv",
    })
}

fn cmp_op(expr: &ast::ExprCompare, params: &ast::Parameters) -> Option<&'static str> {
    let [arg1, arg2] = params.args.as_slice() else {
        return None;
    };
    let [op] = expr.ops.as_slice() else {
        return None;
    };
    let [right] = expr.comparators.as_slice() else {
        return None;
    };
    if !is_same(arg1, &expr.left) || !is_same(arg2, right) {
        return None;
    }
    match op {
        ast::CmpOp::Eq => Some("eq"),
        ast::CmpOp::NotEq => Some("ne"),
        ast::CmpOp::Lt => Some("lt"),
        ast::CmpOp::LtE => Some("le"),
        ast::CmpOp::Gt => Some("gt"),
        ast::CmpOp::GtE => Some("ge"),
        ast::CmpOp::Is => Some("is_"),
        ast::CmpOp::IsNot => Some("is_not"),
        ast::CmpOp::In => Some("contains"),
        ast::CmpOp::NotIn => None,
    }
}

fn is_same(arg: &ast::ParameterWithDefault, expr: &Expr) -> bool {
    if arg.default.is_some() {
        false
    } else if let Expr::Name(name) = expr {
        name.id == arg.parameter.name.as_str()
    } else {
        false
    }
}
