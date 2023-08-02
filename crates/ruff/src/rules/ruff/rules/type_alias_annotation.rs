use ruff_python_ast::{Expr, ExprName, Ranged, Stmt, StmtAnnAssign, StmtTypeAlias};

use crate::{registry::AsRule, settings::types::PythonVersion};
use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for use of `TypeAlias` annotation for declaring type aliases.
///
/// ## Why is this bad?
/// The `type` keyword was introduced in Python 3.12 by PEP-695 for defining type aliases.
/// The type keyword is easier to read and provides cleaner support for generics.
///
/// ## Example
/// ```python
/// ListOfInt: TypeAlias = list[int]
/// ```
///
/// Use instead:
/// ```python
/// type ListOfInt = list[int]
/// ```
#[violation]
pub struct TypeAliasAnnotation {
    name: String,
}

impl Violation for TypeAliasAnnotation {
    const AUTOFIX: AutofixKind = AutofixKind::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeAliasAnnotation { name } = self;
        format!("Type alias `{name}` uses `TypeAlias` annotation instead of the `type` keyword.")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Use the `type` keyword instead".to_string())
    }
}

/// RUF017
pub(crate) fn type_alias_annotation(checker: &mut Checker, stmt: &StmtAnnAssign) {
    let StmtAnnAssign {
        target,
        annotation,
        value,
        ..
    } = stmt;

    // Syntax only available in 3.12+
    if checker.settings.target_version < PythonVersion::Py312 {
        return;
    }

    if !checker
        .semantic()
        .match_typing_expr(annotation, "TypeAlias")
    {
        return;
    }

    let Expr::Name(ExprName { id: name, .. }) = target.as_ref() else {
        return;
    };

    let Some(value) = value else {
        return;
    };

    // todo(zanie): We should check for generic type variables used in the value and define them
    //              as type params instead
    let mut diagnostic = Diagnostic::new(TypeAliasAnnotation { name: name.clone() }, stmt.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            checker.generator().stmt(&Stmt::from(StmtTypeAlias {
                range: TextRange::default(),
                name: target.clone(),
                type_params: None,
                value: value.clone(),
            })),
            stmt.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
