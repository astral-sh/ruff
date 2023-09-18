use ast::{Constant, ExprCall, ExprConstant};
use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    self as ast,
    visitor::{self, Visitor},
    Expr, ExprName, ExprSubscript, Identifier, Stmt, StmtAnnAssign, StmtAssign, StmtTypeAlias,
    TypeParam, TypeParamTypeVar,
};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{registry::AsRule, settings::types::PythonVersion};

/// ## What it does
/// Checks for use of `TypeAlias` annotation for declaring type aliases.
///
/// ## Why is this bad?
/// The `type` keyword was introduced in Python 3.12 by [PEP 695] for defining
/// type aliases. The `type` keyword is easier to read and provides cleaner
/// support for generics.
///
/// ## Known problems
/// [PEP 695] uses inferred variance for type parameters, instead of the
/// `covariant` and `contravariant` keywords used by `TypeParam` variables. As
/// such, rewriting a `TypeParam` variable to a `type` alias may change its
/// variance.
///
/// Unlike `TypeParam` variables, [PEP 695]-style `type` aliases cannot be used
/// at runtime. For example, calling `isinstance` on a `type` alias will throw
/// a `TypeError`. As such, rewriting a `TypeParam` via the `type` keyword will
/// cause issues for parameters that are used for such runtime checks.
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
///
/// [PEP 695]: https://peps.python.org/pep-0695/
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
        let mut visitor = TypeVarReferenceVisitor {
            vars: vec![],
            semantic: checker.semantic(),
        };
        visitor.visit_expr(value);

        let type_params = if visitor.vars.is_empty() {
            None
        } else {
            Some(ast::TypeParams {
                range: TextRange::default(),
                type_params: visitor
                    .vars
                    .into_iter()
                    .map(|TypeVar { name, restriction }| {
                        TypeParam::TypeVar(TypeParamTypeVar {
                            range: TextRange::default(),
                            name: Identifier::new(name.id.clone(), TextRange::default()),
                            bound: match restriction {
                                Some(TypeVarRestriction::Bound(bound)) => {
                                    Some(Box::new(bound.clone()))
                                }
                                Some(TypeVarRestriction::Constraint(constraints)) => {
                                    Some(Box::new(Expr::Tuple(ast::ExprTuple {
                                        range: TextRange::default(),
                                        elts: constraints.into_iter().cloned().collect(),
                                        ctx: ast::ExprContext::Load,
                                    })))
                                }
                                None => None,
                            },
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

#[derive(Debug)]
enum TypeVarRestriction<'a> {
    /// A type variable with a bound, e.g., `TypeVar("T", bound=int)`.
    Bound(&'a Expr),
    /// A type variable with constraints, e.g., `TypeVar("T", int, str)`.
    Constraint(Vec<&'a Expr>),
}

#[derive(Debug)]
struct TypeVar<'a> {
    name: &'a ExprName,
    restriction: Option<TypeVarRestriction<'a>>,
}

struct TypeVarReferenceVisitor<'a> {
    vars: Vec<TypeVar<'a>>,
    semantic: &'a SemanticModel<'a>,
}

/// Recursively collects the names of type variable references present in an expression.
impl<'a> Visitor<'a> for TypeVarReferenceVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) if name.ctx.is_load() => {
                let Some(Stmt::Assign(StmtAssign { value, .. })) = self
                    .semantic
                    .lookup_symbol(name.id.as_str())
                    .and_then(|binding_id| {
                        self.semantic
                            .binding(binding_id)
                            .source
                            .map(|node_id| self.semantic.statement(node_id))
                    })
                else {
                    return;
                };

                match value.as_ref() {
                    Expr::Subscript(ExprSubscript {
                        value: ref subscript_value,
                        ..
                    }) => {
                        if self.semantic.match_typing_expr(subscript_value, "TypeVar") {
                            self.vars.push(TypeVar {
                                name,
                                restriction: None,
                            });
                        }
                    }
                    Expr::Call(ExprCall {
                        func, arguments, ..
                    }) => {
                        if self.semantic.match_typing_expr(func, "TypeVar")
                            && arguments.args.first().is_some_and(|arg| {
                                matches!(
                                    arg,
                                    Expr::Constant(ExprConstant {
                                        value: Constant::Str(_),
                                        ..
                                    })
                                )
                            })
                        {
                            let restriction = if let Some(bound) = arguments.find_keyword("bound") {
                                Some(TypeVarRestriction::Bound(&bound.value))
                            } else if arguments.args.len() > 1 {
                                Some(TypeVarRestriction::Constraint(
                                    arguments.args.iter().skip(1).collect(),
                                ))
                            } else {
                                None
                            };

                            self.vars.push(TypeVar { name, restriction });
                        }
                    }
                    _ => {}
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}
