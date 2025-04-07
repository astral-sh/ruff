use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_dunder;
use ruff_python_ast::{Expr, ExprName, Stmt, StmtAssign, StmtClassDef};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{dataclass_kind, DataclassKind};

/// ## What it does
/// Checks for implicit class variables in dataclasses.
///
/// Variables matching the [`lint.dummy-variable-rgx`] are excluded
/// from this rule.
///
/// ## Why is this bad?
/// Class variables are shared between all instances of that class.
/// In dataclasses, fields with no annotations at all
/// are implicitly considered class variables, and a `TypeError` is
/// raised if a user attempts to initialize an instance of the class
/// with this field.
///
///
/// ```python
/// @dataclass
/// class C:
///     a = 1
///     b: str = ""
///
/// C(a = 42)  # TypeError: C.__init__() got an unexpected keyword argument 'a'
/// ```
///
/// ## Example
///
/// ```python
/// @dataclass
/// class C:
///     a = 1
/// ```
///
/// Use instead:
///
/// ```python
/// from typing import ClassVar
///
///
/// @dataclass
/// class C:
///     a: ClassVar[int] = 1
/// ```
///
/// ## Options
/// - [`lint.dummy-variable-rgx`]
#[derive(ViolationMetadata)]
pub(crate) struct ImplicitClassVarInDataclass;

impl Violation for ImplicitClassVarInDataclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Assignment without annotation found in dataclass body".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use `ClassVar[...]`".to_string())
    }
}

/// RUF045
pub(crate) fn implicit_class_var_in_dataclass(checker: &mut Checker, class_def: &StmtClassDef) {
    let dataclass_kind = dataclass_kind(class_def, checker.semantic());

    if !matches!(dataclass_kind, Some((DataclassKind::Stdlib, _))) {
        return;
    }

    for statement in &class_def.body {
        let Stmt::Assign(StmtAssign { targets, .. }) = statement else {
            continue;
        };

        if targets.len() > 1 {
            continue;
        }

        let target = targets.first().unwrap();
        let Expr::Name(ExprName { id, .. }) = target else {
            continue;
        };

        if checker.settings.dummy_variable_rgx.is_match(id.as_str()) {
            continue;
        }

        if is_dunder(id.as_str()) {
            continue;
        }

        let diagnostic = Diagnostic::new(ImplicitClassVarInDataclass, target.range());

        checker.report_diagnostic(diagnostic);
    }
}
