use ruff_python_ast::{self as ast, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{
    has_default_copy_semantics, is_class_var_annotation, is_dataclass, is_final_annotation,
    is_special_attribute,
};

/// ## What it does
/// Checks for mutable default values in class attributes.
///
/// ## Why is this bad?
/// Mutable default values share state across all instances of the class,
/// while not being obvious. This can lead to bugs when the attributes are
/// changed in one instance, as those changes will unexpectedly affect all
/// other instances.
///
/// When mutable values are intended, they should be annotated with
/// `typing.ClassVar`. When mutability is not required, values should be
/// immutable types, like `tuple` or `frozenset`.
///
/// ## Examples
/// ```python
/// class A:
///     mutable_default: list[int] = []
///     immutable_default: list[int] = []
/// ```
///
/// Use instead:
/// ```python
/// from typing import ClassVar
///
///
/// class A:
///     mutable_default: ClassVar[list[int]] = []
///     immutable_default: tuple[int, ...] = ()
/// ```
#[violation]
pub struct MutableClassDefault;

impl Violation for MutableClassDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Mutable class attributes should be annotated with `typing.ClassVar`")
    }
}

/// RUF012
pub(crate) fn mutable_class_default(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    for statement in &class_def.body {
        match statement {
            Stmt::AnnAssign(ast::StmtAnnAssign {
                annotation,
                target,
                value: Some(value),
                ..
            }) => {
                if !is_special_attribute(target)
                    && is_mutable_expr(value, checker.semantic())
                    && !is_class_var_annotation(annotation, checker.semantic())
                    && !is_final_annotation(annotation, checker.semantic())
                    && !is_immutable_annotation(annotation, checker.semantic(), &[])
                    && !is_dataclass(class_def, checker.semantic())
                {
                    // Avoid, e.g., Pydantic and msgspec models, which end up copying defaults on instance creation.
                    if has_default_copy_semantics(class_def, checker.semantic()) {
                        return;
                    }

                    checker
                        .diagnostics
                        .push(Diagnostic::new(MutableClassDefault, value.range()));
                }
            }
            Stmt::Assign(ast::StmtAssign { value, targets, .. }) => {
                if !targets.iter().all(is_special_attribute)
                    && is_mutable_expr(value, checker.semantic())
                {
                    // Avoid, e.g., Pydantic and msgspec models, which end up copying defaults on instance creation.
                    if has_default_copy_semantics(class_def, checker.semantic()) {
                        return;
                    }

                    checker
                        .diagnostics
                        .push(Diagnostic::new(MutableClassDefault, value.range()));
                }
            }
            _ => (),
        }
    }
}
