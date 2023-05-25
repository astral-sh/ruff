use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::{from_qualified_name, CallPath};
use ruff_python_ast::{call_path::compose_call_path, helpers::map_callable};
use ruff_python_semantic::{
    analyze::typing::{is_immutable_annotation, is_immutable_func},
    model::SemanticModel,
};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for mutable default values in class attribute defaults.
///
/// ## Why is this bad?
/// Mutable default values share state across all instances of the class,
/// while not being obvious. This can lead to bugs when the attributes are
/// changed in one instance, as those changes will unexpectedly affect all
/// other instances.
///
/// ## Examples:
/// ```python
/// from dataclasses import dataclass
///
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
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = I_KNOW_THIS_IS_SHARED_STATE
/// ```
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
/// ## Why is this bad?
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
///
/// def creating_list() -> list[int]:
///     return [1, 2, 3, 4]
///
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = creating_list()
///
///
/// # also:
///
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
///
/// def creating_list() -> list[int]:
///     return [1, 2, 3, 4]
///
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = field(default_factory=creating_list)
///
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
///
/// def creating_list() -> list[int]:
///     return [1, 2, 3, 4]
///
///
/// I_KNOW_THIS_IS_SHARED_STATE = creating_list()
///
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
        expr,
        Expr::List(_)
            | Expr::Dict(_)
            | Expr::Set(_)
            | Expr::ListComp(_)
            | Expr::DictComp(_)
            | Expr::SetComp(_)
    )
}

const ALLOWED_DATACLASS_SPECIFIC_FUNCTIONS: &[&[&str]] = &[&["dataclasses", "field"]];

fn is_allowed_dataclass_function(model: &SemanticModel, func: &Expr) -> bool {
    model.resolve_call_path(func).map_or(false, |call_path| {
        ALLOWED_DATACLASS_SPECIFIC_FUNCTIONS
            .iter()
            .any(|target| call_path.as_slice() == *target)
    })
}

/// Returns `true` if the given [`Expr`] is a `typing.ClassVar` annotation.
fn is_class_var_annotation(model: &SemanticModel, annotation: &Expr) -> bool {
    let Expr::Subscript(ast::ExprSubscript { value, .. }) = &annotation else {
        return false;
    };
    model.match_typing_expr(value, "ClassVar")
}

/// RUF009
pub(crate) fn function_call_in_dataclass_defaults(checker: &mut Checker, body: &[Stmt]) {
    let extend_immutable_calls: Vec<CallPath> = checker
        .settings
        .flake8_bugbear
        .extend_immutable_calls
        .iter()
        .map(|target| from_qualified_name(target))
        .collect();

    for statement in body {
        if let Stmt::AnnAssign(ast::StmtAnnAssign {
            annotation,
            value: Some(expr),
            ..
        }) = statement
        {
            if is_class_var_annotation(checker.semantic_model(), annotation) {
                continue;
            }
            if let Expr::Call(ast::ExprCall { func, .. }) = expr.as_ref() {
                if !is_immutable_func(checker.semantic_model(), func, &extend_immutable_calls)
                    && !is_allowed_dataclass_function(checker.semantic_model(), func)
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
pub(crate) fn mutable_class_default(checker: &mut Checker, body: &[Stmt]) {
    for statement in body {
        match statement {
            Stmt::AnnAssign(ast::StmtAnnAssign {
                annotation,
                value: Some(value),
                ..
            }) => {
                if !is_class_var_annotation(checker.semantic_model(), annotation)
                    && !is_immutable_annotation(checker.semantic_model(), annotation)
                    && is_mutable_expr(value)
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(MutableClassDefault, value.range()));
                }
            }
            Stmt::Assign(ast::StmtAssign { value, .. }) => {
                if is_mutable_expr(value) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(MutableClassDefault, value.range()));
                }
            }
            _ => (),
        }
    }
}

pub(crate) fn is_dataclass(model: &SemanticModel, decorator_list: &[Expr]) -> bool {
    decorator_list.iter().any(|decorator| {
        model
            .resolve_call_path(map_callable(decorator))
            .map_or(false, |call_path| {
                call_path.as_slice() == ["dataclasses", "dataclass"]
            })
    })
}
