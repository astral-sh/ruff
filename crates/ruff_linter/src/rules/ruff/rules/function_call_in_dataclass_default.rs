use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::name::{QualifiedName, UnqualifiedName};
use ruff_python_semantic::analyze::typing::is_immutable_func;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{
    is_class_var_annotation, is_dataclass, is_dataclass_field, is_descriptor_class,
};

/// ## What it does
/// Checks for function calls in dataclass attribute defaults.
///
/// ## Why is this bad?
/// Function calls are only performed once, at definition time. The returned
/// value is then reused by all instances of the dataclass. This can lead to
/// unexpected behavior when the function call returns a mutable object, as
/// changes to the object will be shared across all instances.
///
/// If a field needs to be initialized with a mutable object, use the
/// `field(default_factory=...)` pattern.
///
/// ## Examples
/// ```python
/// from dataclasses import dataclass
///
///
/// def simple_list() -> list[int]:
///     return [1, 2, 3, 4]
///
///
/// @dataclass
/// class A:
///     mutable_default: list[int] = simple_list()
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
/// ```
///
/// ## Options
/// - `lint.flake8-bugbear.extend-immutable-calls`
#[violation]
pub struct FunctionCallInDataclassDefaultArgument {
    name: Option<String>,
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

/// RUF009
pub(crate) fn function_call_in_dataclass_default(
    checker: &mut Checker,
    class_def: &ast::StmtClassDef,
) {
    if !is_dataclass(class_def, checker.semantic()) {
        return;
    }

    let extend_immutable_calls: Vec<QualifiedName> = checker
        .settings
        .flake8_bugbear
        .extend_immutable_calls
        .iter()
        .map(|target| QualifiedName::from_dotted_name(target))
        .collect();

    for statement in &class_def.body {
        if let Stmt::AnnAssign(ast::StmtAnnAssign {
            annotation,
            value: Some(expr),
            ..
        }) = statement
        {
            if let Expr::Call(ast::ExprCall { func, .. }) = expr.as_ref() {
                if !is_class_var_annotation(annotation, checker.semantic())
                    && !is_immutable_func(func, checker.semantic(), &extend_immutable_calls)
                    && !is_dataclass_field(func, checker.semantic())
                    && !is_descriptor_class(func, checker.semantic())
                {
                    checker.diagnostics.push(Diagnostic::new(
                        FunctionCallInDataclassDefaultArgument {
                            name: UnqualifiedName::from_expr(func).map(|name| name.to_string()),
                        },
                        expr.range(),
                    ));
                }
            }
        }
    }
}
