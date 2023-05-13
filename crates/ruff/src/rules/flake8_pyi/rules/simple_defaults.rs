use rustpython_parser::ast::{self, Arguments, Constant, Expr, Operator, Ranged, Unaryop};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::context::Context;
use ruff_python_semantic::scope::{ClassDef, ScopeKind};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct TypedArgumentDefaultInStub;

impl AlwaysAutofixableViolation for TypedArgumentDefaultInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Only simple default values allowed for typed arguments")
    }

    fn autofix_title(&self) -> String {
        "Replace default value with `...`".to_string()
    }
}

#[violation]
pub struct ArgumentDefaultInStub;

impl AlwaysAutofixableViolation for ArgumentDefaultInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Only simple default values allowed for arguments")
    }

    fn autofix_title(&self) -> String {
        "Replace default value with `...`".to_string()
    }
}

#[violation]
pub struct AssignmentDefaultInStub;

impl AlwaysAutofixableViolation for AssignmentDefaultInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Only simple default values allowed for assignments")
    }

    fn autofix_title(&self) -> String {
        "Replace default value with `...`".to_string()
    }
}

#[violation]
pub struct UnannotatedAssignmentInStub {
    name: String,
}

impl Violation for UnannotatedAssignmentInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnannotatedAssignmentInStub { name } = self;
        format!("Need type annotation for `{name}`")
    }
}

const ALLOWED_MATH_ATTRIBUTES_IN_DEFAULTS: &[&[&str]] = &[
    &["math", "inf"],
    &["math", "nan"],
    &["math", "e"],
    &["math", "pi"],
    &["math", "tau"],
];

const ALLOWED_ATTRIBUTES_IN_DEFAULTS: &[&[&str]] = &[
    &["sys", "stdin"],
    &["sys", "stdout"],
    &["sys", "stderr"],
    &["sys", "version"],
    &["sys", "version_info"],
    &["sys", "platform"],
    &["sys", "executable"],
    &["sys", "prefix"],
    &["sys", "exec_prefix"],
    &["sys", "base_prefix"],
    &["sys", "byteorder"],
    &["sys", "maxsize"],
    &["sys", "hexversion"],
    &["sys", "winver"],
];

fn is_valid_default_value_with_annotation(
    default: &Expr,
    checker: &Checker,
    allow_container: bool,
) -> bool {
    match &default {
        Expr::List(ast::ExprList { elts, .. })
        | Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::Set(ast::ExprSet { elts, range: _ }) => {
            return allow_container
                && elts.len() <= 10
                && elts
                    .iter()
                    .all(|e| is_valid_default_value_with_annotation(e, checker, false));
        }
        Expr::Dict(ast::ExprDict {
            keys,
            values,
            range: _,
        }) => {
            return allow_container
                && keys.len() <= 10
                && keys.iter().zip(values).all(|(k, v)| {
                    k.as_ref().map_or(false, |k| {
                        is_valid_default_value_with_annotation(k, checker, false)
                    }) && is_valid_default_value_with_annotation(v, checker, false)
                });
        }
        Expr::Constant(ast::ExprConstant {
            value: Constant::Ellipsis | Constant::None,
            ..
        }) => {
            return true;
        }
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(..),
            ..
        }) => return checker.locator.slice(default.range()).len() <= 50,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bytes(..),
            ..
        }) => return checker.locator.slice(default.range()).len() <= 50,
        // Ex) `123`, `True`, `False`, `3.14`
        Expr::Constant(ast::ExprConstant {
            value: Constant::Int(..) | Constant::Bool(..) | Constant::Float(..),
            ..
        }) => {
            return checker.locator.slice(default.range()).len() <= 10;
        }
        // Ex) `2j`
        Expr::Constant(ast::ExprConstant {
            value: Constant::Complex { real, .. },
            ..
        }) => {
            if *real == 0.0 {
                return checker.locator.slice(default.range()).len() <= 10;
            }
        }
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: Unaryop::USub,
            operand,
            range: _,
        }) => {
            // Ex) `-1`, `-3.14`
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Int(..) | Constant::Float(..),
                ..
            }) = operand.as_ref()
            {
                return checker.locator.slice(operand.range()).len() <= 10;
            }
            // Ex) `-2j`
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Complex { real, .. },
                ..
            }) = operand.as_ref()
            {
                if *real == 0.0 {
                    return checker.locator.slice(operand.range()).len() <= 10;
                }
            }
            // Ex) `-math.inf`, `-math.pi`, etc.
            if let Expr::Attribute(_) = operand.as_ref() {
                if checker
                    .ctx
                    .resolve_call_path(operand)
                    .map_or(false, |call_path| {
                        ALLOWED_MATH_ATTRIBUTES_IN_DEFAULTS.iter().any(|target| {
                            // reject `-math.nan`
                            call_path.as_slice() == *target && *target != ["math", "nan"]
                        })
                    })
                {
                    return true;
                }
            }
        }
        Expr::BinOp(ast::ExprBinOp {
            left,
            op: Operator::Add | Operator::Sub,
            right,
            range: _,
        }) => {
            // Ex) `1 + 2j`, `1 - 2j`, `-1 - 2j`, `-1 + 2j`
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Complex { .. },
                ..
            }) = right.as_ref()
            {
                // Ex) `1 + 2j`, `1 - 2j`
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Int(..) | Constant::Float(..),
                    ..
                }) = left.as_ref()
                {
                    return checker.locator.slice(left.range()).len() <= 10;
                } else if let Expr::UnaryOp(ast::ExprUnaryOp {
                    op: Unaryop::USub,
                    operand,
                    range: _,
                }) = left.as_ref()
                {
                    // Ex) `-1 + 2j`, `-1 - 2j`
                    if let Expr::Constant(ast::ExprConstant {
                        value: Constant::Int(..) | Constant::Float(..),
                        ..
                    }) = operand.as_ref()
                    {
                        return checker.locator.slice(operand.range()).len() <= 10;
                    }
                }
            }
        }
        // Ex) `math.inf`, `sys.stdin`, etc.
        Expr::Attribute(_) => {
            if checker
                .ctx
                .resolve_call_path(default)
                .map_or(false, |call_path| {
                    ALLOWED_MATH_ATTRIBUTES_IN_DEFAULTS
                        .iter()
                        .chain(ALLOWED_ATTRIBUTES_IN_DEFAULTS.iter())
                        .any(|target| call_path.as_slice() == *target)
                })
            {
                return true;
            }
        }
        _ => {}
    }
    false
}

/// Returns `true` if an [`Expr`] appears to be a valid PEP 604 union. (e.g. `int | None`)
fn is_valid_pep_604_union(annotation: &Expr) -> bool {
    match &annotation {
        Expr::BinOp(ast::ExprBinOp {
            left,
            op: Operator::BitOr,
            right,
            range: _,
        }) => is_valid_pep_604_union(left) && is_valid_pep_604_union(right),
        Expr::Name(_)
        | Expr::Subscript(_)
        | Expr::Attribute(_)
        | Expr::Constant(ast::ExprConstant {
            value: Constant::None,
            ..
        }) => true,
        _ => false,
    }
}

/// Returns `true` if an [`Expr`] appears to be a valid default value without an annotation.
fn is_valid_default_value_without_annotation(default: &Expr) -> bool {
    matches!(
        &default,
        Expr::Call(_)
            | Expr::Name(_)
            | Expr::Attribute(_)
            | Expr::Subscript(_)
            | Expr::Constant(ast::ExprConstant {
                value: Constant::Ellipsis | Constant::None,
                ..
            })
    ) || is_valid_pep_604_union(default)
}

/// Returns `true` if an [`Expr`] appears to be `TypeVar`, `TypeVarTuple`, `NewType`, or `ParamSpec`
/// call.
fn is_type_var_like_call(context: &Context, expr: &Expr) -> bool {
    let Expr::Call(ast::ExprCall { func, .. } )= &expr else {
        return false;
    };
    context.resolve_call_path(func).map_or(false, |call_path| {
        matches!(
            call_path.as_slice(),
            [
                "typing" | "typing_extensions",
                "TypeVar" | "TypeVarTuple" | "NewType" | "ParamSpec"
            ]
        )
    })
}

/// Returns `true` if this is a "special" assignment which must have a value (e.g., an assignment to
/// `__all__`).
fn is_special_assignment(context: &Context, target: &Expr) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = &target {
        match id.as_str() {
            "__all__" => context.scope().kind.is_module(),
            "__match_args__" | "__slots__" => context.scope().kind.is_class(),
            _ => false,
        }
    } else {
        false
    }
}

/// Returns `true` if the a class is an enum, based on its base classes.
fn is_enum(context: &Context, bases: &[Expr]) -> bool {
    return bases.iter().any(|expr| {
        context.resolve_call_path(expr).map_or(false, |call_path| {
            matches!(
                call_path.as_slice(),
                [
                    "enum",
                    "Enum" | "Flag" | "IntEnum" | "IntFlag" | "StrEnum" | "ReprEnum"
                ]
            )
        })
    });
}

/// PYI011
pub(crate) fn typed_argument_simple_defaults(checker: &mut Checker, args: &Arguments) {
    if !args.defaults.is_empty() {
        let defaults_start = args.posonlyargs.len() + args.args.len() - args.defaults.len();
        for (i, arg) in args.posonlyargs.iter().chain(&args.args).enumerate() {
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.defaults.get(i))
            {
                if arg.annotation.is_some() {
                    if !is_valid_default_value_with_annotation(default, checker, true) {
                        let mut diagnostic =
                            Diagnostic::new(TypedArgumentDefaultInStub, default.range());

                        if checker.patch(diagnostic.kind.rule()) {
                            #[allow(deprecated)]
                            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                                "...".to_string(),
                                default.range(),
                            )));
                        }

                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }

    if !args.kw_defaults.is_empty() {
        let defaults_start = args.kwonlyargs.len() - args.kw_defaults.len();
        for (i, kwarg) in args.kwonlyargs.iter().enumerate() {
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.kw_defaults.get(i))
            {
                if kwarg.annotation.is_some() {
                    if !is_valid_default_value_with_annotation(default, checker, true) {
                        let mut diagnostic =
                            Diagnostic::new(TypedArgumentDefaultInStub, default.range());

                        if checker.patch(diagnostic.kind.rule()) {
                            #[allow(deprecated)]
                            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                                "...".to_string(),
                                default.range(),
                            )));
                        }

                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }
}

/// PYI014
pub(crate) fn argument_simple_defaults(checker: &mut Checker, args: &Arguments) {
    if !args.defaults.is_empty() {
        let defaults_start = args.posonlyargs.len() + args.args.len() - args.defaults.len();
        for (i, arg) in args.posonlyargs.iter().chain(&args.args).enumerate() {
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.defaults.get(i))
            {
                if arg.annotation.is_none() {
                    if !is_valid_default_value_with_annotation(default, checker, true) {
                        let mut diagnostic =
                            Diagnostic::new(ArgumentDefaultInStub, default.range());

                        if checker.patch(diagnostic.kind.rule()) {
                            #[allow(deprecated)]
                            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                                "...".to_string(),
                                default.range(),
                            )));
                        }

                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }

    if !args.kw_defaults.is_empty() {
        let defaults_start = args.kwonlyargs.len() - args.kw_defaults.len();
        for (i, kwarg) in args.kwonlyargs.iter().enumerate() {
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.kw_defaults.get(i))
            {
                if kwarg.annotation.is_none() {
                    if !is_valid_default_value_with_annotation(default, checker, true) {
                        let mut diagnostic =
                            Diagnostic::new(ArgumentDefaultInStub, default.range());

                        if checker.patch(diagnostic.kind.rule()) {
                            #[allow(deprecated)]
                            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                                "...".to_string(),
                                default.range(),
                            )));
                        }

                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }
}

/// PYI015
pub(crate) fn assignment_default_in_stub(checker: &mut Checker, targets: &[Expr], value: &Expr) {
    if targets.len() != 1 {
        return;
    }
    let target = &targets[0];
    if !target.is_name_expr() {
        return;
    }
    if is_special_assignment(&checker.ctx, target) {
        return;
    }
    if is_type_var_like_call(&checker.ctx, value) {
        return;
    }
    if is_valid_default_value_without_annotation(value) {
        return;
    }
    if is_valid_default_value_with_annotation(value, checker, true) {
        return;
    }

    let mut diagnostic = Diagnostic::new(AssignmentDefaultInStub, value.range());
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            "...".to_string(),
            value.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// PYI015
pub(crate) fn annotated_assignment_default_in_stub(
    checker: &mut Checker,
    target: &Expr,
    value: &Expr,
    annotation: &Expr,
) {
    if checker.ctx.match_typing_expr(annotation, "TypeAlias") {
        return;
    }
    if is_special_assignment(&checker.ctx, target) {
        return;
    }
    if is_type_var_like_call(&checker.ctx, value) {
        return;
    }
    if is_valid_default_value_with_annotation(value, checker, true) {
        return;
    }

    let mut diagnostic = Diagnostic::new(AssignmentDefaultInStub, value.range());
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            "...".to_string(),
            value.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// PYI052
pub(crate) fn unannotated_assignment_in_stub(
    checker: &mut Checker,
    targets: &[Expr],
    value: &Expr,
) {
    if targets.len() != 1 {
        return;
    }
    let target = &targets[0];
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };
    if is_special_assignment(&checker.ctx, target) {
        return;
    }
    if is_type_var_like_call(&checker.ctx, value) {
        return;
    }
    if is_valid_default_value_without_annotation(value) {
        return;
    }
    if !is_valid_default_value_with_annotation(value, checker, true) {
        return;
    }

    if let ScopeKind::Class(ClassDef { bases, .. }) = &checker.ctx.scope().kind {
        if is_enum(&checker.ctx, bases) {
            return;
        }
    }
    checker.diagnostics.push(Diagnostic::new(
        UnannotatedAssignmentInStub {
            name: id.to_string(),
        },
        value.range(),
    ));
}
