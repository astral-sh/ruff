use rustpython_parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{call_path::compose_call_path, helpers::map_callable};
use ruff_python_semantic::analyze::typing::is_immutable_annotation;
use ruff_python_semantic::context::Context;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for mutable default values in dataclasses without the use of
/// `dataclasses.field`.
///
/// ## Why is it bad?
/// Mutable default values share state across all instances of the dataclass,
/// while not being obvious. This can lead to bugs when the attributes are
/// changed in one instance, as those changes will unexpectedly affect all
/// other instances.
///
/// ## Examples:
/// ```python
/// from dataclasses import dataclass
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = []
/// ```
///
/// Use instead:
/// ```python
/// from dataclasses import dataclass, field
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = field(default_factory=list)
/// ```
///
/// Alternatively, if you _want_ shared behaviour, make it more obvious
/// by assigning to a module-level variable:
/// ```python
/// from dataclasses import dataclass
///
/// I_KNOW_THIS_IS_SHARED_STATE = [1, 2, 3, 4]
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = I_KNOW_THIS_IS_SHARED_STATE
/// ```
#[violation]
pub struct MutableDataclassDefault;

impl Violation for MutableDataclassDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable default values for dataclass attributes")
    }
}

/// This rule is same as MutableDataclassDefault, but for any class. The same arguments apply.
#[violation]
pub struct MutableClassDefault;

impl Violation for MutableClassDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable default values for class attributes")
    }
}

/// ## What it does
/// Checks for function calls in dataclass defaults.
///
/// ## Why is it bad?
/// Function calls are only performed once, at definition time. The returned
/// value is then reused by all instances of the dataclass.
///
/// ## Examples:
/// ```python
/// from dataclasses import dataclass
///
/// def creating_list() -> list[]:
///     return [1, 2, 3, 4]
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = creating_list()
///
/// # also:
///
/// @dataclass
/// class B:
///     also_mutable_default_but_sneakier: A = A()
/// ```
///
/// Use instead:
/// ```python
/// from dataclasses import dataclass, field
///
/// def creating_list() -> list[]:
///     return [1, 2, 3, 4]
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = field(default_factory=creating_list)
///
/// @dataclass
/// class B:
///     also_mutable_default_but_sneakier: A = field(default_factory=A)
/// ```
///
/// Alternatively, if you _want_ the shared behaviour, make it more obvious
/// by assigning it to a module-level variable:
/// ```python
/// from dataclasses import dataclass
///
/// def creating_list() -> list[]:
///     return [1, 2, 3, 4]
///
/// I_KNOW_THIS_IS_SHARED_STATE = creating_list()
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = I_KNOW_THIS_IS_SHARED_STATE
/// ```
#[violation]
pub struct FunctionCallInDataclassDefaultArgument {
    pub name: Option<String>,
}

impl Violation for FunctionCallInDataclassDefaultArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FunctionCallInDataclassDefaultArgument { name } = self;
        if let Some(name) = name {
            format!("Do not perform function call `{name}` in dataclass defaults")
        } else {
            format!("Do not perform function call in dataclass defaults")
        }
    }
}

/// Same as FunctionCallInDataclassDefaultArgument, but for any class.
/// Importantly, this error will be issued on calls to dataclasses.field
#[violation]
pub struct FunctionCallInClassDefaultArgument {
    pub name: Option<String>,
}

impl Violation for FunctionCallInClassDefaultArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FunctionCallInClassDefaultArgument { name } = self;
        if let Some(name) = name {
            format!("Do not perform function call `{name}` in non-dataclass attribute defaults")
        } else {
            format!("Do not perform function call in non-dataclass attribute defaults")
        }
    }
}

fn is_mutable_expr(expr: &Expr) -> bool {
    matches!(
        &expr.node,
        ExprKind::List { .. }
            | ExprKind::Dict { .. }
            | ExprKind::Set { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::DictComp { .. }
            | ExprKind::SetComp { .. }
    )
}

const ALLOWED_FUNCS: &[&[&str]] = &[&["dataclasses", "field"]];

fn is_allowed_dataclass_func(context: &Context, func: &Expr) -> bool {
    context.resolve_call_path(func).map_or(false, |call_path| {
        ALLOWED_FUNCS
            .iter()
            .any(|target| call_path.as_slice() == *target)
    })
}

/// Returns `true` if the given [`Expr`] is a `typing.ClassVar` annotation.
fn is_class_var_annotation(context: &Context, annotation: &Expr) -> bool {
    let ExprKind::Subscript { value, .. } = &annotation.node else {
        return false;
    };
    context.match_typing_expr(value, "ClassVar")
}

/// RUF009/RUF011
pub fn function_call_in_class_defaults(
    checker: &mut Checker,
    body: &[Stmt],
    is_dataclass: bool,
    emit_dataclass_error: bool,
) {
    for statement in body {
        if let StmtKind::AnnAssign {
            annotation,
            value: Some(expr),
            ..
        } = &statement.node
        {
            if is_class_var_annotation(&checker.ctx, annotation) {
                continue;
            }
            if let ExprKind::Call { func, .. } = &expr.node {
                if !is_dataclass || !is_allowed_dataclass_func(&checker.ctx, func) {
                    let diagnostic: Diagnostic = if emit_dataclass_error {
                        Diagnostic::new(
                            FunctionCallInDataclassDefaultArgument {
                                name: compose_call_path(func),
                            },
                            expr.range(),
                        )
                    } else {
                        Diagnostic::new(
                            FunctionCallInClassDefaultArgument {
                                name: compose_call_path(func),
                            },
                            expr.range(),
                        )
                    };
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}

/// RUF008/RUF010
pub fn mutable_class_default(checker: &mut Checker, emit_dataclass_error: bool, body: &[Stmt]) {
    fn diagnostic(emit_dataclass_error: bool, value: &Expr) -> Diagnostic {
        if emit_dataclass_error {
            Diagnostic::new(MutableDataclassDefault, value.range())
        } else {
            Diagnostic::new(MutableClassDefault, value.range())
        }
    }

    for statement in body {
        match &statement.node {
            StmtKind::AnnAssign {
                annotation,
                value: Some(value),
                ..
            } => {
                if !is_class_var_annotation(&checker.ctx, annotation)
                    && !is_immutable_annotation(&checker.ctx, annotation)
                    && is_mutable_expr(value)
                {
                    checker
                        .diagnostics
                        .push(diagnostic(emit_dataclass_error, value));
                }
            }
            StmtKind::Assign { value, .. } => {
                if is_mutable_expr(value) {
                    checker
                        .diagnostics
                        .push(diagnostic(emit_dataclass_error, value));
                }
            }
            _ => (),
        }
    }
}

pub fn is_dataclass(checker: &Checker, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|decorator| {
        checker
            .ctx
            .resolve_call_path(map_callable(decorator))
            .map_or(false, |call_path| {
                call_path.as_slice() == ["dataclasses", "dataclass"]
            })
    })
}
