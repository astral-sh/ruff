use ruff_python_ast::{self as ast, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{
    dataclass_kind, has_default_copy_semantics, is_class_var_annotation, is_final_annotation,
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
/// Generally speaking, you probably want to avoid having mutable default
/// values in the class body at all; instead, these variables should usually
/// be initialized in `__init__`. However, other possible fixes for the issue
/// can include:
/// - Explicitly annotating the variable with [`typing.ClassVar`][ClassVar] to
///   indicate that it is intended to be shared across all instances.
/// - Using an immutable data type (e.g. a tuple instead of a list)
///   for the default value.
///
/// ## Example
///
/// ```python
/// class A:
///     variable_1: list[int] = []
///     variable_2: set[int] = set()
///     variable_3: dict[str, int] = {}
/// ```
///
/// Use instead:
///
/// ```python
/// class A:
///     def __init__(self) -> None:
///         self.variable_1: list[int] = []
///         self.variable_2: set[int] = set()
///         self.variable_3: dict[str, int] = {}
/// ```
///
/// Or:
///
/// ```python
/// from typing import ClassVar
///
///
/// class A:
///     variable_1: ClassVar[list[int]] = []
///     variable_2: ClassVar[set[int]] = set()
///     variable_3: ClassVar[dict[str, int]] = {}
/// ```
///
/// Or:
///
/// ```python
/// class A:
///     variable_1: list[int] | None = None
///     variable_2: set[int] | None = None
///     variable_3: dict[str, int] | None = None
/// ```
///
/// Or:
///
/// ```python
/// from collections.abc import Sequence, Mapping, Set as AbstractSet
/// from types import MappingProxyType
///
///
/// class A:
///     variable_1: Sequence[int] = ()
///     variable_2: AbstractSet[int] = frozenset()
///     variable_3: Mapping[str, int] = MappingProxyType({})
/// ```
///
/// [ClassVar]: https://docs.python.org/3/library/typing.html#typing.ClassVar
#[derive(ViolationMetadata)]
pub(crate) struct MutableClassDefault;

impl Violation for MutableClassDefault {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Mutable class attributes should be annotated with `typing.ClassVar`".to_string()
    }
}

/// RUF012
pub(crate) fn mutable_class_default(checker: &Checker, class_def: &ast::StmtClassDef) {
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
                {
                    if dataclass_kind(class_def, checker.semantic()).is_some() {
                        continue;
                    }

                    // Avoid, e.g., Pydantic and msgspec models, which end up copying defaults on instance creation.
                    if has_default_copy_semantics(class_def, checker.semantic()) {
                        return;
                    }

                    checker.report_diagnostic(Diagnostic::new(MutableClassDefault, value.range()));
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

                    checker.report_diagnostic(Diagnostic::new(MutableClassDefault, value.range()));
                }
            }
            _ => (),
        }
    }
}
