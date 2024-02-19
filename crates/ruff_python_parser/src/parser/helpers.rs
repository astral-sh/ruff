use std::hash::BuildHasherDefault;

use ast::CmpOp;
use ruff_python_ast::{self as ast, Expr, ExprContext};
use rustc_hash::FxHashSet;

use crate::{ParseError, ParseErrorType, TokenKind};

/// Set the `ctx` for `Expr::Id`, `Expr::Attribute`, `Expr::Subscript`, `Expr::Starred`,
/// `Expr::Tuple` and `Expr::List`. If `expr` is either `Expr::Tuple` or `Expr::List`,
/// recursively sets the `ctx` for their elements.
pub(super) fn set_expr_ctx(expr: &mut Expr, new_ctx: ExprContext) {
    match expr {
        Expr::Name(ast::ExprName { ctx, .. })
        | Expr::Attribute(ast::ExprAttribute { ctx, .. })
        | Expr::Subscript(ast::ExprSubscript { ctx, .. }) => *ctx = new_ctx,
        Expr::Starred(ast::ExprStarred { value, ctx, .. }) => {
            *ctx = new_ctx;
            set_expr_ctx(value, new_ctx);
        }
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => {
            set_expr_ctx(operand, new_ctx);
        }
        Expr::List(ast::ExprList { elts, ctx, .. })
        | Expr::Tuple(ast::ExprTuple { elts, ctx, .. }) => {
            *ctx = new_ctx;
            elts.iter_mut()
                .for_each(|element| set_expr_ctx(element, new_ctx));
        }
        _ => {}
    }
}

/// Check if the given expression is itself or contains an expression that is
/// valid on the left hand side of an assignment. For example, identifiers,
/// starred expressions, attribute expressions, subscript expressions,
/// list and tuple unpacking are valid assignment targets.
pub(super) fn is_valid_assignment_target(expr: &Expr) -> bool {
    match expr {
        Expr::Starred(ast::ExprStarred { value, .. }) => is_valid_assignment_target(value),
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            elts.iter().all(is_valid_assignment_target)
        }
        Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) => true,
        _ => false,
    }
}

/// Check if the given expression is itself or contains an expression that is
/// valid on the left hand side of an augmented assignment. For example, identifiers,
/// attribute and subscript expressions are valid augmented assignment targets.
pub(super) fn is_valid_aug_assignment_target(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_)
    )
}

/// Converts a [`TokenKind`] array of size 2 to its correspondent [`CmpOp`].
pub(super) fn token_kind_to_cmp_op(kind: [TokenKind; 2]) -> Result<CmpOp, ()> {
    Ok(match kind {
        [TokenKind::Is, TokenKind::Not] => CmpOp::IsNot,
        [TokenKind::Is, _] => CmpOp::Is,
        [TokenKind::In, _] => CmpOp::In,
        [TokenKind::EqEqual, _] => CmpOp::Eq,
        [TokenKind::Less, _] => CmpOp::Lt,
        [TokenKind::Greater, _] => CmpOp::Gt,
        [TokenKind::NotEqual, _] => CmpOp::NotEq,
        [TokenKind::LessEqual, _] => CmpOp::LtE,
        [TokenKind::GreaterEqual, _] => CmpOp::GtE,
        [TokenKind::Not, TokenKind::In] => CmpOp::NotIn,
        _ => return Err(()),
    })
}

// Perform validation of function/lambda parameters in a function definition.
pub(super) fn validate_parameters(parameters: &ast::Parameters) -> Result<(), ParseError> {
    let mut all_arg_names = FxHashSet::with_capacity_and_hasher(
        parameters.posonlyargs.len()
            + parameters.args.len()
            + usize::from(parameters.vararg.is_some())
            + parameters.kwonlyargs.len()
            + usize::from(parameters.kwarg.is_some()),
        BuildHasherDefault::default(),
    );

    let posonlyargs = parameters.posonlyargs.iter();
    let args = parameters.args.iter();
    let kwonlyargs = parameters.kwonlyargs.iter();

    let vararg: Option<&ast::Parameter> = parameters.vararg.as_deref();
    let kwarg: Option<&ast::Parameter> = parameters.kwarg.as_deref();

    for arg in posonlyargs
        .chain(args)
        .chain(kwonlyargs)
        .map(|arg| &arg.parameter)
        .chain(vararg)
        .chain(kwarg)
    {
        let range = arg.range;
        let arg_name = arg.name.as_str();
        if !all_arg_names.insert(arg_name) {
            return Err(ParseError {
                error: ParseErrorType::DuplicateArgumentError(arg_name.to_string()),
                location: range,
            });
        }
    }

    Ok(())
}

pub(super) fn validate_arguments(arguments: &ast::Arguments) -> Result<(), ParseError> {
    let mut all_arg_names = FxHashSet::with_capacity_and_hasher(
        arguments.keywords.len(),
        BuildHasherDefault::default(),
    );

    for (name, range) in arguments
        .keywords
        .iter()
        .filter_map(|argument| argument.arg.as_ref().map(|arg| (arg, argument.range)))
    {
        let arg_name = name.as_str();
        if !all_arg_names.insert(arg_name) {
            return Err(ParseError {
                error: ParseErrorType::DuplicateKeywordArgumentError(arg_name.to_string()),
                location: range,
            });
        }
    }

    Ok(())
}
