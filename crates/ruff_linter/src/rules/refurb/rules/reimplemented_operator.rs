use std::fmt::{Debug, Display, Formatter};

use anyhow::Result;
use itertools::Itertools;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, ExprSlice, ExprSubscript, ExprTuple, Parameters, Stmt};
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
    operator: Operator,
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
    let Some(operator) = get_operator(checker, body, params) else {
        return;
    };
    let fix = target.try_fix(&operator, checker.importer(), checker.semantic());
    let mut diagnostic = Diagnostic::new(
        ReimplementedOperator {
            operator,
            target: target.kind(),
        },
        target.range(),
    );
    diagnostic.try_set_optional_fix(|| fix);
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
        operator: &Operator,
        importer: &Importer,
        semantic: &SemanticModel,
    ) -> Result<Option<Fix>> {
        match self {
            Self::Lambda(_) => {
                let (edit, binding) = importer.get_or_import_symbol(
                    &ImportRequest::import("operator", operator.name),
                    self.start(),
                    semantic,
                )?;
                let content = if let Some(args) = operator.args.as_ref() {
                    format!("{binding}({args})")
                } else {
                    binding
                };
                Ok(Some(Fix::safe_edits(
                    Edit::range_replacement(content, self.range()),
                    [edit],
                )))
            }
            Self::Function(_) => Ok(None),
        }
    }
}

/// Convert the slice expression to the string representation of `slice` call.
/// For example, expression `1:2` will be `slice(1, 2)`, and `:` will be `slice(None)`.
fn slice_expr_to_slice_call(checker: &mut Checker, expr_slice: &ExprSlice) -> String {
    let stringify =
        |x: Option<&Box<Expr>>| x.map_or("None".into(), |x| checker.generator().expr(x));
    match (
        expr_slice.lower.as_ref(),
        expr_slice.upper.as_ref(),
        expr_slice.step.as_ref(),
    ) {
        (l, u, s @ Some(_)) => format!(
            "slice({}, {}, {})",
            stringify(l),
            stringify(u),
            stringify(s)
        ),
        (None, u, None) => format!("slice({})", stringify(u)),
        (l @ Some(_), u, None) => format!("slice({}, {})", stringify(l), stringify(u)),
    }
}

/// Convert the given expression to a string representation, suitable to be a function argument.
fn subscript_slice_to_string(checker: &mut Checker, expr: &Expr) -> String {
    if let Expr::Slice(expr_slice) = expr {
        slice_expr_to_slice_call(checker, expr_slice)
    } else {
        checker.generator().expr(expr)
    }
}

/// Return the `operator` implemented by given subscript expression.
fn itemgetter_op(
    checker: &mut Checker,
    expr: &ExprSubscript,
    params: &Parameters,
) -> Option<Operator> {
    let [arg] = params.args.as_slice() else {
        return None;
    };
    if !is_same_expression(arg, &expr.value) {
        return None;
    };
    Some(Operator {
        name: "itemgetter",
        args: Some(subscript_slice_to_string(checker, expr.slice.as_ref())),
    })
}

/// Return the `operator` implemented by given tuple expression.
fn itemgetter_op_tuple(
    checker: &mut Checker,
    expr: &ExprTuple,
    params: &Parameters,
) -> Option<Operator> {
    let [arg] = params.args.as_slice() else {
        return None;
    };
    if expr.elts.is_empty() {
        return None;
    }
    if !expr.elts.iter().all(|expr| {
        expr.as_subscript_expr()
            .is_some_and(|expr| is_same_expression(arg, &expr.value))
    }) {
        return None;
    }
    Some(Operator {
        name: "itemgetter",
        args: Some(
            expr.elts
                .iter()
                .map(|expr| {
                    subscript_slice_to_string(
                        checker,
                        // unwrap is safe, because we check that all elts are subscripts
                        expr.as_subscript_expr().unwrap().slice.as_ref(),
                    )
                })
                .join(", "),
        ),
    })
}

#[derive(Eq, PartialEq, Debug)]
struct Operator {
    name: &'static str,
    args: Option<String>,
}

impl From<&'static str> for Operator {
    fn from(value: &'static str) -> Self {
        Self {
            name: value,
            args: None,
        }
    }
}

impl Display for Operator {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        self.args
            .as_ref()
            .map_or(Ok(()), |args| write!(f, "({args})"))
    }
}

/// Return the `operator` implemented by the given expression.
fn get_operator(checker: &mut Checker, expr: &Expr, params: &ast::Parameters) -> Option<Operator> {
    match expr {
        Expr::UnaryOp(expr) => unary_op(expr, params).map(Into::into),
        Expr::BinOp(expr) => bin_op(expr, params).map(Into::into),
        Expr::Compare(expr) => cmp_op(expr, params).map(Into::into),
        Expr::Subscript(expr) => itemgetter_op(checker, expr, params),
        Expr::Tuple(expr) => itemgetter_op_tuple(checker, expr, params),
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
    let [op] = &*expr.ops else {
        return None;
    };
    let [right] = &*expr.comparators else {
        return None;
    };

    match op {
        ast::CmpOp::Eq => match_arguments(arg1, arg2, &expr.left, right).then_some("eq"),
        ast::CmpOp::NotEq => match_arguments(arg1, arg2, &expr.left, right).then_some("ne"),
        ast::CmpOp::Lt => match_arguments(arg1, arg2, &expr.left, right).then_some("lt"),
        ast::CmpOp::LtE => match_arguments(arg1, arg2, &expr.left, right).then_some("le"),
        ast::CmpOp::Gt => match_arguments(arg1, arg2, &expr.left, right).then_some("gt"),
        ast::CmpOp::GtE => match_arguments(arg1, arg2, &expr.left, right).then_some("ge"),
        ast::CmpOp::Is => match_arguments(arg1, arg2, &expr.left, right).then_some("is_"),
        ast::CmpOp::IsNot => match_arguments(arg1, arg2, &expr.left, right).then_some("is_not"),
        ast::CmpOp::In => {
            // Note: `operator.contains` reverses the order of arguments. That is:
            // `operator.contains` is equivalent to `lambda x, y: y in x`, rather than
            // `lambda x, y: x in y`.
            match_arguments(arg1, arg2, right, &expr.left).then_some("contains")
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
