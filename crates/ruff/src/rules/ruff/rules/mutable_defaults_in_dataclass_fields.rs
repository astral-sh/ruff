use ruff_python_ast::call_path::{from_qualified_name, CallPath};
use rustpython_parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{call_path::compose_call_path, helpers::map_callable};
use ruff_python_semantic::{
    analyze::typing::{is_immutable_annotation, is_immutable_func},
    context::Context,
};

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

/// ## What it does
/// Checks for function calls in dataclass defaults.
///
/// ## Why is it bad?
/// Function calls are only performed once, at definition time. The returned
/// value is then reused by all instances of the dataclass.
///
/// ## Options
/// - `flake8-bugbear.extend-immutable-calls`
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

const ALLOWED_DATACLASS_SPECIFIC_FUNCTIONS: &[&[&str]] = &[&["dataclasses", "field"]];

fn is_allowed_dataclass_function(context: &Context, func: &Expr) -> bool {
    context.resolve_call_path(func).map_or(false, |call_path| {
        ALLOWED_DATACLASS_SPECIFIC_FUNCTIONS
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

/// RUF009
pub fn function_call_in_dataclass_defaults(checker: &mut Checker, body: &[Stmt]) {
    let extend_immutable_calls: Vec<CallPath> = checker
        .settings
        .flake8_bugbear
        .extend_immutable_calls
        .iter()
        .map(|target| from_qualified_name(target))
        .collect();

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
                if !is_immutable_func(&checker.ctx, func, &extend_immutable_calls)
                    && !is_allowed_dataclass_function(&checker.ctx, func)
                {
                    checker.diagnostics.push(Diagnostic::new(
                        FunctionCallInDataclassDefaultArgument {
                            name: compose_call_path(func),
                        },
                        expr.range(),
                    ));
                }
            }
        }
    }
}

/// RUF008
pub fn mutable_dataclass_default(checker: &mut Checker, body: &[Stmt]) {
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
                        .push(Diagnostic::new(MutableDataclassDefault, value.range()));
                }
            }
            StmtKind::Assign { value, .. } => {
                if is_mutable_expr(value) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(MutableDataclassDefault, value.range()));
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
