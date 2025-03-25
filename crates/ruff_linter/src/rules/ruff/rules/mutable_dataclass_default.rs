use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{
    dataclass_kind, is_class_var_annotation, is_dataclass_field,
};

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

        // Check for direct assignment to a mutable expression (e.g., []).
        if is_mutable_expr(value, semantic)
            && !is_class_var_annotation(annotation, semantic)
            && !is_immutable_annotation(annotation, semantic, &[])
        {
            let diagnostic = Diagnostic::new(MutableDataclassDefault, value.range());
            checker.report_diagnostic(diagnostic);
            continue;
        }

        // If the value is a call to a dataclass/attrs field helper, check
        // for an explicit `default` of a mutable expression. Only emit
        // diagnostics for this case in preview mode, since it's an expansion
        // of the existing `RUF008` rule.
        if checker.settings.preview.is_enabled() {
            if let Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) = value.as_ref()
            {
                if is_dataclass_field(func, semantic, dataclass_kind) {
                    if let Some(default) = arguments.find_keyword("default") {
                        if is_mutable_expr(&default.value, semantic)
                            && !is_class_var_annotation(annotation, semantic)
                            && !is_immutable_annotation(annotation, semantic, &[])
                        {
                            let diagnostic =
                                Diagnostic::new(MutableDataclassDefault, default.value.range());
                            checker.report_diagnostic(diagnostic);
                        }
                    }
                }
            }
        }
    }
}
