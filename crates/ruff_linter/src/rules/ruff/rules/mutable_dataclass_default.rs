use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::preview::is_mutable_default_in_dataclass_field_enabled;
use crate::rules::ruff::helpers::{dataclass_kind, is_class_var_annotation, is_dataclass_field};

/// ## What it does
/// Checks for mutable default values in dataclass attributes.
///
/// ## Why is this bad?
/// Mutable default values share state across all instances of the dataclass.
/// This can lead to bugs when the attributes are changed in one instance, as
/// those changes will unexpectedly affect all other instances.
///
/// Instead of sharing mutable defaults, use the `field(default_factory=...)`
/// pattern.
///
/// If the default value is intended to be mutable, it must be annotated with
/// `typing.ClassVar`; otherwise, a `ValueError` will be raised.
///
/// In [preview](https://docs.astral.sh/ruff/preview/) this rule also detects mutable defaults passed via the `default` keyword
/// argument in `field()` (for stdlib dataclasses), `attrs.field()`, `attr.ib()`,
/// and `attr.attrib()` calls.
///
/// ## Example
/// ```python
/// from dataclasses import dataclass
///
///
/// @dataclass
/// class A:
///     # A list without a `default_factory` or `ClassVar` annotation
///     # will raise a `ValueError`.
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
/// Or:
/// ```python
/// from dataclasses import dataclass
/// from typing import ClassVar
///
///
/// @dataclass
/// class A:
///     mutable_default: ClassVar[list[int]] = []
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.262")]
pub(crate) struct MutableDataclassDefault;

impl Violation for MutableDataclassDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not use mutable default values for dataclass attributes".to_string()
    }
}

/// RUF008
pub(crate) fn mutable_dataclass_default(checker: &Checker, class_def: &ast::StmtClassDef) {
    let semantic = checker.semantic();

    let Some((dataclass_kind, _)) = dataclass_kind(class_def, semantic) else {
        return;
    };

    for statement in &class_def.body {
        let Stmt::AnnAssign(ast::StmtAnnAssign {
            annotation,
            value: Some(value),
            ..
        }) = statement
        else {
            continue;
        };

        let value = match &**value {
            Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) if is_mutable_default_in_dataclass_field_enabled(checker.settings())
                && is_dataclass_field(func, checker.semantic(), dataclass_kind) =>
            {
                arguments.find_argument_value("default", 0)
            }
            value => Some(value),
        };

        let Some(value) = value else {
            continue;
        };

        if is_mutable_expr(value, checker.semantic())
            && !is_class_var_annotation(annotation, checker.semantic())
            && !is_immutable_annotation(annotation, checker.semantic(), &[])
        {
            checker.report_diagnostic(MutableDataclassDefault, value.range());
        }
    }
}
