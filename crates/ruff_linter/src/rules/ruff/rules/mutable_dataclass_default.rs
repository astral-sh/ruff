use ruff_python_ast::{self as ast, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{is_class_var_annotation, is_dataclass};

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
/// ## Examples
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
#[violation]
pub struct MutableDataclassDefault;

impl Violation for MutableDataclassDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable default values for dataclass attributes")
    }
}

/// RUF008
pub(crate) fn mutable_dataclass_default(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    if !is_dataclass(class_def, checker.semantic()) {
        return;
    }

    for statement in &class_def.body {
        if let Stmt::AnnAssign(ast::StmtAnnAssign {
            annotation,
            value: Some(value),
            ..
        }) = statement
        {
            if is_mutable_expr(value, checker.semantic())
                && !is_class_var_annotation(annotation, checker.semantic())
                && !is_immutable_annotation(annotation, checker.semantic(), &[])
            {
                checker
                    .diagnostics
                    .push(Diagnostic::new(MutableDataclassDefault, value.range()));
            }
        }
    }
}
