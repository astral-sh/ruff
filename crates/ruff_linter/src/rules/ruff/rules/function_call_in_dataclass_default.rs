use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::{QualifiedName, UnqualifiedName};
use ruff_python_semantic::analyze::typing::{
    is_immutable_annotation, is_immutable_func, is_immutable_newtype_call,
};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{
    dataclass_kind, is_class_var_annotation, is_dataclass_field, is_descriptor_class,
    AttrsAutoAttribs, DataclassKind,
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
/// Attributes whose default arguments are `NewType` calls
/// where the original type is immutable are ignored.
///
/// ## Example
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
#[derive(ViolationMetadata)]
pub(crate) struct FunctionCallInDataclassDefaultArgument {
    name: Option<String>,
}

impl Violation for FunctionCallInDataclassDefaultArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(name) = &self.name {
            format!("Do not perform function call `{name}` in dataclass defaults")
        } else {
            "Do not perform function call in dataclass defaults".to_string()
        }
    }
}

/// RUF009
pub(crate) fn function_call_in_dataclass_default(checker: &Checker, class_def: &ast::StmtClassDef) {
    let semantic = checker.semantic();

    let Some((dataclass_kind, _)) = dataclass_kind(class_def, semantic) else {
        return;
    };

    let attrs_auto_attribs = match dataclass_kind {
        DataclassKind::Stdlib => None,

        DataclassKind::Attrs(auto_attribs) => match auto_attribs {
            AttrsAutoAttribs::Unknown => return,

            AttrsAutoAttribs::None => {
                if any_annotated(&class_def.body) {
                    Some(AttrsAutoAttribs::True)
                } else {
                    Some(AttrsAutoAttribs::False)
                }
            }

            _ => Some(auto_attribs),
        },
    };

    let dataclass_kind = match attrs_auto_attribs {
        None => DataclassKind::Stdlib,
        Some(auto_attribs) => DataclassKind::Attrs(auto_attribs),
    };

    let extend_immutable_calls: Vec<QualifiedName> = checker
        .settings
        .flake8_bugbear
        .extend_immutable_calls
        .iter()
        .map(|target| QualifiedName::from_dotted_name(target))
        .collect();

    for statement in &class_def.body {
        let Stmt::AnnAssign(ast::StmtAnnAssign {
            annotation,
            value: Some(expr),
            ..
        }) = statement
        else {
            continue;
        };
        let Expr::Call(ast::ExprCall { func, .. }) = expr.as_ref() else {
            continue;
        };

        let is_field = is_dataclass_field(func, checker.semantic(), dataclass_kind);

        // Non-explicit fields in an `attrs` dataclass
        // with `auto_attribs=False` are class variables.
        if matches!(attrs_auto_attribs, Some(AttrsAutoAttribs::False)) && !is_field {
            continue;
        }

        if is_field
            || is_immutable_annotation(annotation, checker.semantic(), &extend_immutable_calls)
            || is_class_var_annotation(annotation, checker.semantic())
            || is_immutable_func(func, checker.semantic(), &extend_immutable_calls)
            || is_descriptor_class(func, checker.semantic())
            || func.as_name_expr().is_some_and(|name| {
                is_immutable_newtype_call(name, checker.semantic(), &extend_immutable_calls)
            })
        {
            continue;
        }

        let kind = FunctionCallInDataclassDefaultArgument {
            name: UnqualifiedName::from_expr(func).map(|name| name.to_string()),
        };
        let diagnostic = Diagnostic::new(kind, expr.range());

        checker.report_diagnostic(diagnostic);
    }
}

#[inline]
fn any_annotated(class_body: &[Stmt]) -> bool {
    class_body
        .iter()
        .any(|stmt| matches!(stmt, Stmt::AnnAssign(..)))
}
