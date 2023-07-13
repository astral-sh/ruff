use rustpython_parser::ast::{
    self, ArgWithDefault, Arguments, Constant, Expr, Operator, Ranged, Stmt, UnaryOp,
};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::CallPath;
use ruff_python_ast::source_code::Locator;
use ruff_python_semantic::{ScopeKind, SemanticModel};

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

#[violation]
pub struct UnassignedSpecialVariableInStub {
    name: String,
}

/// ## What it does
/// Checks that `__all__`, `__match_args__`, and `__slots__` variables are
/// assigned to values when defined in stub files.
///
/// ## Why is this bad?
/// Special variables like `__all__` have the same semantics in stub files
/// as they do in Python modules, and so should be consistent with their
/// runtime counterparts.
///
/// ## Example
/// ```python
/// __all__: list[str]
/// ```
///
/// Use instead:
/// ```python
/// __all__: list[str] = ["foo", "bar"]
/// ```
impl Violation for UnassignedSpecialVariableInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnassignedSpecialVariableInStub { name } = self;
        format!("`{name}` in a stub file must have a value, as it has the same semantics as `{name}` at runtime")
    }
}

fn is_allowed_negated_math_attribute(call_path: &CallPath) -> bool {
    matches!(call_path.as_slice(), ["math", "inf" | "e" | "pi" | "tau"])
}

fn is_allowed_math_attribute(call_path: &CallPath) -> bool {
    matches!(
        call_path.as_slice(),
        ["math", "inf" | "nan" | "e" | "pi" | "tau"]
            | [
                "sys",
                "stdin"
                    | "stdout"
                    | "stderr"
                    | "version"
                    | "version_info"
                    | "platform"
                    | "executable"
                    | "prefix"
                    | "exec_prefix"
                    | "base_prefix"
                    | "byteorder"
                    | "maxsize"
                    | "hexversion"
                    | "winver"
            ]
    )
}

fn is_valid_default_value_with_annotation(
    default: &Expr,
    allow_container: bool,
    locator: &Locator,
    semantic: &SemanticModel,
) -> bool {
    match default {
        Expr::Constant(_) => {
            return true;
        }
        Expr::List(ast::ExprList { elts, .. })
        | Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::Set(ast::ExprSet { elts, range: _ }) => {
            return allow_container
                && elts.len() <= 10
                && elts
                    .iter()
                    .all(|e| is_valid_default_value_with_annotation(e, false, locator, semantic));
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
                        is_valid_default_value_with_annotation(k, false, locator, semantic)
                    }) && is_valid_default_value_with_annotation(v, false, locator, semantic)
                });
        }
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: UnaryOp::USub,
            operand,
            range: _,
        }) => {
            match operand.as_ref() {
                // Ex) `-1`, `-3.14`, `2j`
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Int(..) | Constant::Float(..) | Constant::Complex { .. },
                    ..
                }) => return true,
                // Ex) `-math.inf`, `-math.pi`, etc.
                Expr::Attribute(_) => {
                    if semantic
                        .resolve_call_path(operand)
                        .as_ref()
                        .map_or(false, is_allowed_negated_math_attribute)
                    {
                        return true;
                    }
                }
                _ => {}
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
                    return locator.slice(left.range()).len() <= 10;
                } else if let Expr::UnaryOp(ast::ExprUnaryOp {
                    op: UnaryOp::USub,
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
                        return locator.slice(operand.range()).len() <= 10;
                    }
                }
            }
        }
        // Ex) `math.inf`, `sys.stdin`, etc.
        Expr::Attribute(_) => {
            if semantic
                .resolve_call_path(default)
                .as_ref()
                .map_or(false, is_allowed_math_attribute)
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
    match annotation {
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
        default,
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
fn is_type_var_like_call(expr: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };
    semantic.resolve_call_path(func).map_or(false, |call_path| {
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
fn is_special_assignment(target: &Expr, semantic: &SemanticModel) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = target {
        match id.as_str() {
            "__all__" => semantic.scope().kind.is_module(),
            "__match_args__" | "__slots__" => semantic.scope().kind.is_class(),
            _ => false,
        }
    } else {
        false
    }
}

/// Returns `true` if this is an assignment to a simple `Final`-annotated variable.
fn is_final_assignment(annotation: &Expr, value: &Expr, semantic: &SemanticModel) -> bool {
    if matches!(value, Expr::Name(_) | Expr::Attribute(_)) {
        if semantic.match_typing_expr(annotation, "Final") {
            return true;
        }
    }
    false
}

/// Returns `true` if the a class is an enum, based on its base classes.
fn is_enum(bases: &[Expr], semantic: &SemanticModel) -> bool {
    return bases.iter().any(|expr| {
        semantic.resolve_call_path(expr).map_or(false, |call_path| {
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
pub(crate) fn typed_argument_simple_defaults(checker: &mut Checker, arguments: &Arguments) {
    for ArgWithDefault {
        def,
        default,
        range: _,
    } in arguments
        .posonlyargs
        .iter()
        .chain(&arguments.args)
        .chain(&arguments.kwonlyargs)
    {
        let Some(default) = default else {
            continue;
        };
        if def.annotation.is_some() {
            if !is_valid_default_value_with_annotation(
                default,
                true,
                checker.locator,
                checker.semantic(),
            ) {
                let mut diagnostic = Diagnostic::new(TypedArgumentDefaultInStub, default.range());

                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                        "...".to_string(),
                        default.range(),
                    )));
                }

                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// PYI014
pub(crate) fn argument_simple_defaults(checker: &mut Checker, arguments: &Arguments) {
    for ArgWithDefault {
        def,
        default,
        range: _,
    } in arguments
        .posonlyargs
        .iter()
        .chain(&arguments.args)
        .chain(&arguments.kwonlyargs)
    {
        let Some(default) = default else {
            continue;
        };
        if def.annotation.is_none() {
            if !is_valid_default_value_with_annotation(
                default,
                true,
                checker.locator,
                checker.semantic(),
            ) {
                let mut diagnostic = Diagnostic::new(ArgumentDefaultInStub, default.range());

                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                        "...".to_string(),
                        default.range(),
                    )));
                }

                checker.diagnostics.push(diagnostic);
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
    if is_special_assignment(target, checker.semantic()) {
        return;
    }
    if is_type_var_like_call(value, checker.semantic()) {
        return;
    }
    if is_valid_default_value_without_annotation(value) {
        return;
    }
    if is_valid_default_value_with_annotation(value, true, checker.locator, checker.semantic()) {
        return;
    }

    let mut diagnostic = Diagnostic::new(AssignmentDefaultInStub, value.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
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
    if checker
        .semantic()
        .match_typing_expr(annotation, "TypeAlias")
    {
        return;
    }
    if is_special_assignment(target, checker.semantic()) {
        return;
    }
    if is_type_var_like_call(value, checker.semantic()) {
        return;
    }
    if is_final_assignment(annotation, value, checker.semantic()) {
        return;
    }
    if is_valid_default_value_with_annotation(value, true, checker.locator, checker.semantic()) {
        return;
    }

    let mut diagnostic = Diagnostic::new(AssignmentDefaultInStub, value.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
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
    if is_special_assignment(target, checker.semantic()) {
        return;
    }
    if is_type_var_like_call(value, checker.semantic()) {
        return;
    }
    if is_valid_default_value_without_annotation(value) {
        return;
    }
    if !is_valid_default_value_with_annotation(value, true, checker.locator, checker.semantic()) {
        return;
    }

    if let ScopeKind::Class(ast::StmtClassDef { bases, .. }) = checker.semantic().scope().kind {
        if is_enum(bases, checker.semantic()) {
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

/// PYI035
pub(crate) fn unassigned_special_variable_in_stub(
    checker: &mut Checker,
    target: &Expr,
    stmt: &Stmt,
) {
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    if !is_special_assignment(target, checker.semantic()) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        UnassignedSpecialVariableInStub {
            name: id.to_string(),
        },
        stmt.range(),
    ));
}
