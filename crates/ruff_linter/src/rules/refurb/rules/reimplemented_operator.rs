use anyhow::Result;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::{ImportRequest, Importer};

/// ## What it does
/// Checks for lambda expressions and function definitions that can be replaced
/// with a function from the `operator` module.
///
/// ## Why is this bad?
/// The `operator` module provides functions that implement the same functionality
/// as the corresponding operators. For example, `operator.add` is equivalent to
/// `lambda x, y: x + y`. Using the functions from the `operator` module is more
/// concise and communicates the intent of the code more clearly.
///
/// ## Example
/// ```python
/// import functools
///
/// nums = [1, 2, 3]
/// sum = functools.reduce(lambda x, y: x + y, nums)
/// ```
///
/// Use instead:
/// ```python
/// import functools
/// import operator
///
/// nums = [1, 2, 3]
/// sum = functools.reduce(operator.add, nums)
/// ```
///
/// ## References
#[violation]
pub struct ReimplementedOperator {
    operator: &'static str,
    target: FunctionLikeKind,
}

impl Violation for ReimplementedOperator {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ReimplementedOperator { operator, target } = self;
        match target {
            FunctionLikeKind::Function => {
                format!("Use `operator.{operator}` instead of defining a function")
            }
            FunctionLikeKind::Lambda => {
                format!("Use `operator.{operator}` instead of defining a lambda")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let ReimplementedOperator { operator, .. } = self;
        Some(format!("Replace with `operator.{operator}`"))
    }
}

/// FURB118
pub(crate) fn reimplemented_operator(checker: &mut Checker, target: &FunctionLike) {
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
    diagnostic
        .try_set_optional_fix(|| target.try_fix(operator, checker.importer(), checker.semantic()));
    checker.diagnostics.push(diagnostic);
}

/// Candidate for lambda expression or function definition consisting of a return statement.
#[derive(Debug)]
pub(crate) enum FunctionLike<'a> {
    Lambda(&'a ast::ExprLambda),
    Function(&'a ast::StmtFunctionDef),
}

impl<'a> From<&'a ast::ExprLambda> for FunctionLike<'a> {
    fn from(lambda: &'a ast::ExprLambda) -> Self {
        Self::Lambda(lambda)
    }
}

impl<'a> From<&'a ast::StmtFunctionDef> for FunctionLike<'a> {
    fn from(function: &'a ast::StmtFunctionDef) -> Self {
        Self::Function(function)
    }
}

impl Ranged for FunctionLike<'_> {
    fn range(&self) -> TextRange {
        match self {
            Self::Lambda(expr) => expr.range(),
            Self::Function(stmt) => stmt.range(),
        }
    }
}

impl FunctionLike<'_> {
    /// Return the [`ast::Parameters`] of the function-like node.
    fn parameters(&self) -> Option<&ast::Parameters> {
        match self {
            Self::Lambda(expr) => expr.parameters.as_deref(),
            Self::Function(stmt) => Some(&stmt.parameters),
        }
    }

    /// Return the body of the function-like node.
    ///
    /// If the node is a function definition that consists of more than a single return statement,
    /// returns `None`.
    fn body(&self) -> Option<&Expr> {
        match self {
            Self::Lambda(expr) => Some(&expr.body),
            Self::Function(stmt) => match stmt.body.as_slice() {
                [Stmt::Return(ast::StmtReturn { value, .. })] => value.as_deref(),
                _ => None,
            },
        }
    }

    /// Return the display kind of the function-like node.
    fn kind(&self) -> FunctionLikeKind {
        match self {
            Self::Lambda(_) => FunctionLikeKind::Lambda,
            Self::Function(_) => FunctionLikeKind::Function,
        }
    }

    /// Attempt to fix the function-like node by replacing it with a call to the corresponding
    /// function from `operator` module.
    fn try_fix(
        &self,
        operator: &'static str,
        importer: &Importer,
        semantic: &SemanticModel,
    ) -> Result<Option<Fix>> {
        match self {
            Self::Lambda(_) => {
                let (edit, binding) = importer.get_or_import_symbol(
                    &ImportRequest::import("operator", operator),
                    self.start(),
                    semantic,
                )?;
                Ok(Some(Fix::safe_edits(
                    Edit::range_replacement(binding, self.range()),
                    [edit],
                )))
            }
            Self::Function(_) => Ok(None),
        }
    }
}

/// Return the name of the `operator` implemented by the given expression.
fn get_operator(expr: &Expr, params: &ast::Parameters) -> Option<&'static str> {
    match expr {
        Expr::UnaryOp(expr) => unary_op(expr, params),
        Expr::BinOp(expr) => bin_op(expr, params),
        Expr::Compare(expr) => cmp_op(expr, params),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum FunctionLikeKind {
    Lambda,
    Function,
}

/// Return the name of the `operator` implemented by the given unary expression.
fn unary_op(expr: &ast::ExprUnaryOp, params: &ast::Parameters) -> Option<&'static str> {
    let [arg] = params.args.as_slice() else {
        return None;
    };
    if !is_same_expression(arg, &expr.operand) {
        return None;
    }
    Some(match expr.op {
        ast::UnaryOp::Invert => "invert",
        ast::UnaryOp::Not => "not_",
        ast::UnaryOp::UAdd => "pos",
        ast::UnaryOp::USub => "neg",
    })
}

/// Return the name of the `operator` implemented by the given binary expression.
fn bin_op(expr: &ast::ExprBinOp, params: &ast::Parameters) -> Option<&'static str> {
    let [arg1, arg2] = params.args.as_slice() else {
        return None;
    };
    if !is_same_expression(arg1, &expr.left) || !is_same_expression(arg2, &expr.right) {
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

/// Return the name of the `operator` implemented by the given comparison expression.
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

    match op {
        ast::CmpOp::Eq => {
            if match_arguments(arg1, arg2, &expr.left, right) {
                Some("eq")
            } else {
                None
            }
        }
        ast::CmpOp::NotEq => {
            if match_arguments(arg1, arg2, &expr.left, right) {
                Some("ne")
            } else {
                None
            }
        }
        ast::CmpOp::Lt => {
            if match_arguments(arg1, arg2, &expr.left, right) {
                Some("lt")
            } else {
                None
            }
        }
        ast::CmpOp::LtE => {
            if match_arguments(arg1, arg2, &expr.left, right) {
                Some("le")
            } else {
                None
            }
        }
        ast::CmpOp::Gt => {
            if match_arguments(arg1, arg2, &expr.left, right) {
                Some("gt")
            } else {
                None
            }
        }
        ast::CmpOp::GtE => {
            if match_arguments(arg1, arg2, &expr.left, right) {
                Some("ge")
            } else {
                None
            }
        }
        ast::CmpOp::Is => {
            if match_arguments(arg1, arg2, &expr.left, right) {
                Some("is_")
            } else {
                None
            }
        }
        ast::CmpOp::IsNot => {
            if match_arguments(arg1, arg2, &expr.left, right) {
                Some("is_not")
            } else {
                None
            }
        }
        ast::CmpOp::In => {
            // Note: `operator.contains` reverses the order of arguments. That is:
            // `operator.contains` is equivalent to `lambda x, y: y in x`, rather than
            // `lambda x, y: x in y`.
            if match_arguments(arg1, arg2, right, &expr.left) {
                Some("contains")
            } else {
                None
            }
        }
        ast::CmpOp::NotIn => None,
    }
}

/// Returns `true` if the given arguments match the expected operands.
fn match_arguments(
    arg1: &ast::ParameterWithDefault,
    arg2: &ast::ParameterWithDefault,
    operand1: &Expr,
    operand2: &Expr,
) -> bool {
    is_same_expression(arg1, operand1) && is_same_expression(arg2, operand2)
}

/// Returns `true` if the given argument is the "same" as the given expression. For example, if
/// the argument has a default, it is not considered the same as any expression; if both match the
/// same name, they are considered the same.
fn is_same_expression(arg: &ast::ParameterWithDefault, expr: &Expr) -> bool {
    if arg.default.is_some() {
        false
    } else if let Expr::Name(name) = expr {
        name.id == arg.parameter.name.as_str()
    } else {
        false
    }
}
