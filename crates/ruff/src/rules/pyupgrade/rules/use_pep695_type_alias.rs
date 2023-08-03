use ruff_python_ast::{
    visitor::{self, Visitor},
    Expr, ExprName, ExprSubscript, Identifier, Ranged, Stmt, StmtAnnAssign, StmtAssign,
    StmtTypeAlias, TypeParam, TypeParamTypeVar,
};
use ruff_python_semantic::SemanticModel;

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
pub struct NonPEP695TypeAlias {
    name: String,
}

impl Violation for NonPEP695TypeAlias {
    const AUTOFIX: AutofixKind = AutofixKind::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NonPEP695TypeAlias { name } = self;
        format!("Type alias `{name}` uses `TypeAlias` annotation instead of the `type` keyword")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Use the `type` keyword".to_string())
    }
}

/// UP040
pub(crate) fn non_pep695_type_alias(checker: &mut Checker, stmt: &StmtAnnAssign) {
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

    // TODO(zanie): We should check for generic type variables used in the value and define them
    //              as type params instead
    let mut diagnostic = Diagnostic::new(NonPEP695TypeAlias { name: name.clone() }, stmt.range());
    if checker.patch(diagnostic.kind.rule()) {
        let mut visitor = TypeVarNameVisitor {
            names: vec![],
            semantic: checker.semantic(),
        };
        visitor.visit_expr(value);

        let type_params = if visitor.names.is_empty() {
            None
        } else {
            Some(ruff_python_ast::TypeParams {
                range: TextRange::default(),
                type_params: visitor
                    .names
                    .iter()
                    .map(|name| {
                        TypeParam::TypeVar(TypeParamTypeVar {
                            range: TextRange::default(),
                            name: Identifier::new(name.id.clone(), TextRange::default()),
                            bound: None,
                        })
                    })
                    .collect(),
            })
        };

        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            checker.generator().stmt(&Stmt::from(StmtTypeAlias {
                range: TextRange::default(),
                name: target.clone(),
                type_params,
                value: value.clone(),
            })),
            stmt.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

struct TypeVarNameVisitor<'a> {
    names: Vec<&'a ExprName>,
    semantic: &'a SemanticModel<'a>,
}

impl<'a> Visitor<'a> for TypeVarNameVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) if name.ctx.is_load() => {
                let Some(Stmt::Assign(StmtAssign { value, .. })) =
                    self.semantic
                        .scope()
                        .get(name.id.as_str())
                        .and_then(|binding_id| {
                            self.semantic
                                .binding(binding_id)
                                .source
                                .map(|node_id| self.semantic.stmts[node_id])
                        }) else {
                            return;
                        };

                // Only support type variables declared as TypeVar['<name>'] for now
                // Type variables declared with `TypeVar(<name>, ...)` can include more complex features
                // like bounds and variance
                let Expr::Subscript(ExprSubscript {value: ref subscript_value, .. })= value.as_ref() else {
                    return;
                };

                if self.semantic.match_typing_expr(subscript_value, "TypeVar") {
                    self.names.push(name);
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}
