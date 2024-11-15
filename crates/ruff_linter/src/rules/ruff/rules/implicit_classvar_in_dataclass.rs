use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, Stmt, StmtAssign, StmtClassDef};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{dataclass_kind, DataclassKind};

/// ## What it does
/// Checks for implicit class variables in dataclasses.
///
/// ## Why is this bad?
/// Class variables are shared between all instances of that class.
/// In dataclasses, fields with no annotations at all
/// are implicitly considered class variables.
///
/// This might cause confusing behavior:
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
#[violation]
pub struct ImplicitClassVarInDataclass;

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

    if !matches!(dataclass_kind, Some(DataclassKind::Stdlib)) {
        return;
    };

    for statement in &class_def.body {
        let Stmt::Assign(StmtAssign { targets, .. }) = statement else {
            continue;
        };

        if targets.len() > 1 {
            continue;
        }

        let target = targets.first().unwrap();

        if !matches!(target, Expr::Name(..)) {
            continue;
        }

        let diagnostic = Diagnostic::new(ImplicitClassVarInDataclass, target.range());

        checker.diagnostics.push(diagnostic);
    }
}
