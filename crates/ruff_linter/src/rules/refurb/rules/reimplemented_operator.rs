use std::borrow::Cow;
use std::fmt::{Debug, Display, Formatter};

use anyhow::Result;
use itertools::Itertools;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::{self as ast, Expr, ExprSlice, ExprSubscript, ExprTuple, Parameters, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::{ImportRequest, Importer};
use crate::Locator;

/// ## What it does
/// Checks for lambda expressions and function definitions that can be replaced with a function from
/// the `operator` module.
///
/// ## Why is this bad?
/// The `operator` module provides functions that implement the same functionality as the
/// corresponding operators. For example, `operator.add` is often equivalent to `lambda x, y: x + y`.
/// Using the functions from the `operator` module is more concise and communicates the intent of
/// the code more clearly.
///
/// ## Example
/// ```python
/// import functools
///
/// nums = [1, 2, 3]
/// total = functools.reduce(lambda x, y: x + y, nums)
/// ```
///
/// Use instead:
/// ```python
/// import functools
/// import operator
///
/// nums = [1, 2, 3]
/// total = functools.reduce(operator.add, nums)
/// ```
///
/// ## Fix safety
/// The fix offered by this rule is always marked as unsafe. While the changes the fix would make
/// would rarely break your code, there are two ways in which functions from the `operator` module
/// differ from user-defined functions. It would be non-trivial for Ruff to detect whether or not
/// these differences would matter in a specific situation where Ruff is emitting a diagnostic for
/// this rule.
///
/// The first difference is that `operator` functions cannot be called with keyword arguments, but
/// most user-defined functions can. If an `add` function is defined as `add = lambda x, y: x + y`,
/// replacing this function with `operator.add` will cause the later call to raise `TypeError` if
/// the function is later called with keyword arguments, e.g. `add(x=1, y=2)`.
///
/// The second difference is that user-defined functions are [descriptors], but this is not true of
/// the functions defined in the `operator` module. Practically speaking, this means that defining
/// a function in a class body (either by using a `def` statement or assigning a `lambda` function
/// to a variable) is a valid way of defining an instance method on that class; monkeypatching a
/// user-defined function onto a class after the class has been created also has the same effect.
/// The same is not true of an `operator` function: assigning an `operator` function to a variable
/// in a class body or monkeypatching one onto a class will not create a valid instance method.
/// Ruff will refrain from emitting diagnostics for this rule on function definitions in class
/// bodies; however, it does not currently have sophisticated enough type inference to avoid
/// emitting this diagnostic if a user-defined function is being monkeypatched onto a class after
/// the class has been constructed.
///
/// [descriptors]: https://docs.python.org/3/howto/descriptor.html
#[derive(ViolationMetadata)]
pub(crate) struct ReimplementedOperator {
    operator: Operator,
    target: FunctionLikeKind,
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
pub(crate) fn reimplemented_operator(checker: &Checker, target: &FunctionLike) {
    // Ignore methods.
    // Methods can be defined via a `def` statement in a class scope,
    // or via a lambda appearing on the right-hand side of an assignment in a class scope.
    if checker.semantic().current_scope().kind.is_class()
        && (target.is_function_def()
            || checker
                .semantic()
                .current_statements()
                .any(|stmt| matches!(stmt, Stmt::AnnAssign(_) | Stmt::Assign(_))))
    {
        return;
    }

    let Some(params) = target.parameters() else {
        return;
    };
    let Some(body) = target.body() else { return };
    let Some(operator) = get_operator(body, params, checker.locator()) else {
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
    checker.report_diagnostic(diagnostic);
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
            Self::Function(stmt) => stmt.identifier(),
        }
    }
}

impl FunctionLike<'_> {
    /// Return the [`Parameters`] of the function-like node.
    fn parameters(&self) -> Option<&Parameters> {
        match self {
            Self::Lambda(expr) => expr.parameters.as_deref(),
            Self::Function(stmt) => Some(&stmt.parameters),
        }
    }

    const fn is_function_def(&self) -> bool {
        matches!(self, Self::Function(_))
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
                let content = if operator.args.is_empty() {
                    binding
                } else {
                    format!("{binding}({})", operator.args.join(", "))
                };
                Ok(Some(Fix::unsafe_edits(
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
fn slice_expr_to_slice_call(slice: &ExprSlice, locator: &Locator) -> String {
    let stringify = |expr: Option<&Expr>| expr.map_or("None", |expr| locator.slice(expr));
    match (
        slice.lower.as_deref(),
        slice.upper.as_deref(),
        slice.step.as_deref(),
    ) {
        (lower, upper, step @ Some(_)) => format!(
            "slice({}, {}, {})",
            stringify(lower),
            stringify(upper),
            stringify(step)
        ),
        (None, upper, None) => format!("slice({})", stringify(upper)),
        (lower @ Some(_), upper, None) => {
            format!("slice({}, {})", stringify(lower), stringify(upper))
        }
    }
}

/// Convert the given expression to a string representation, suitable to be a function argument.
fn subscript_slice_to_string<'a>(expr: &Expr, locator: &Locator<'a>) -> Cow<'a, str> {
    if let Expr::Slice(expr_slice) = expr {
        Cow::Owned(slice_expr_to_slice_call(expr_slice, locator))
    } else if let Expr::Tuple(tuple) = expr {
        if locator.slice(tuple).contains(':') {
            // We cannot perform a trivial replacement if there's a `:` in the expression
            let inner = tuple
                .iter()
                .map(|expr| match expr {
                    Expr::Slice(expr) => Cow::Owned(slice_expr_to_slice_call(expr, locator)),
                    _ => Cow::Borrowed(locator.slice(expr)),
                })
                .join(", ");

            Cow::Owned(format!("({inner})"))
        } else if tuple.parenthesized {
            Cow::Borrowed(locator.slice(expr))
        } else {
            Cow::Owned(format!("({})", locator.slice(tuple)))
        }
    } else {
        Cow::Borrowed(locator.slice(expr))
    }
}

/// Return the `operator` implemented by given subscript expression.
fn itemgetter_op(expr: &ExprSubscript, params: &Parameters, locator: &Locator) -> Option<Operator> {
    let [arg] = params.args.as_slice() else {
        return None;
    };

    // The argument to the lambda must match the subscripted value, as in: `lambda x: x[1]`.
    if !is_same_expression(arg, &expr.value) {
        return None;
    }

    // The subscripted expression can't contain references to the argument, as in: `lambda x: x[x]`.
    if any_over_expr(expr.slice.as_ref(), &|expr| is_same_expression(arg, expr)) {
        return None;
    }

    Some(Operator {
        name: "itemgetter",
        args: vec![subscript_slice_to_string(expr.slice.as_ref(), locator).to_string()],
    })
}

/// Return the `operator` implemented by given tuple expression.
fn itemgetter_op_tuple(
    expr: &ExprTuple,
    params: &Parameters,
    locator: &Locator,
) -> Option<Operator> {
    let [arg] = params.args.as_slice() else {
        return None;
    };
    if expr.len() <= 1 {
        return None;
    }
    Some(Operator {
        name: "itemgetter",
        args: expr
            .iter()
            .map(|expr| {
                expr.as_subscript_expr()
                    .filter(|expr| is_same_expression(arg, &expr.value))
                    .map(|expr| expr.slice.as_ref())
                    .map(|slice| subscript_slice_to_string(slice, locator).to_string())
            })
            .collect::<Option<Vec<_>>>()?,
    })
}

#[derive(Eq, PartialEq, Debug)]
struct Operator {
    name: &'static str,
    args: Vec<String>,
}

impl From<&'static str> for Operator {
    fn from(value: &'static str) -> Self {
        Self {
            name: value,
            args: vec![],
        }
    }
}

impl Display for Operator {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if self.args.is_empty() {
            Ok(())
        } else {
            write!(f, "({})", self.args.join(", "))
        }
    }
}

/// Return the `operator` implemented by the given expression.
fn get_operator(expr: &Expr, params: &Parameters, locator: &Locator) -> Option<Operator> {
    match expr {
        Expr::UnaryOp(expr) => unary_op(expr, params).map(Operator::from),
        Expr::BinOp(expr) => bin_op(expr, params).map(Operator::from),
        Expr::Compare(expr) => cmp_op(expr, params).map(Operator::from),
        Expr::Subscript(expr) => itemgetter_op(expr, params, locator),
        Expr::Tuple(expr) => itemgetter_op_tuple(expr, params, locator),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum FunctionLikeKind {
    Lambda,
    Function,
}

impl FunctionLikeKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Lambda => "lambda",
            Self::Function => "function",
        }
    }
}

impl Display for FunctionLikeKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Return the name of the `operator` implemented by the given unary expression.
fn unary_op(expr: &ast::ExprUnaryOp, params: &Parameters) -> Option<&'static str> {
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
fn bin_op(expr: &ast::ExprBinOp, params: &Parameters) -> Option<&'static str> {
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
fn cmp_op(expr: &ast::ExprCompare, params: &Parameters) -> Option<&'static str> {
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
fn is_same_expression(param: &ast::ParameterWithDefault, expr: &Expr) -> bool {
    if param.default.is_some() {
        false
    } else if let Expr::Name(name) = expr {
        name.id == param.name().as_str()
    } else {
        false
    }
}
