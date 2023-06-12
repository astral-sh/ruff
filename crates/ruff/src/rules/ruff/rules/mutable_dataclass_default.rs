use rustpython_parser::ast::{self, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::is_immutable_annotation;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{is_class_var_annotation, is_dataclass, is_mutable_expr};

/// ## What it does
/// Checks for mutable default values in dataclasses.
///
/// ## Why is this bad?
/// Mutable default values share state across all instances of the dataclass,
/// while not being obvious. This can lead to bugs when the attributes are
/// changed in one instance, as those changes will unexpectedly affect all
/// other instances.
///
/// Instead of sharing mutable defaults, use the `field(default_factory=...)`
/// pattern.
///
/// ## Examples
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
    if !is_dataclass(checker.semantic_model(), class_def) {
        return;
    }

    for statement in &class_def.body {
        match statement {
            Stmt::AnnAssign(ast::StmtAnnAssign {
                annotation,
                value: Some(value),
                ..
            }) => {
                if is_mutable_expr(value)
                    && !is_class_var_annotation(checker.semantic_model(), annotation)
                    && !is_immutable_annotation(checker.semantic_model(), annotation)
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(MutableDataclassDefault, value.range()));
                }
            }
            Stmt::Assign(ast::StmtAssign { value, .. }) => {
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
