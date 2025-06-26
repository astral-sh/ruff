use ruff_db::parsed::ParsedModuleRef;
use rustc_hash::FxHashMap;

use ruff_python_ast::{self as ast, AnyNodeRef};

use crate::Db;
use crate::semantic_index::ast_ids::{HasScopedExpressionId, ScopedExpressionId};
use crate::semantic_index::place::ScopeId;
use crate::types::tuple::{Splatter, SplatterError, TupleElement, TupleLength, TupleType};
use crate::types::{KnownClass, Type, TypeCheckDiagnostics, infer_expression_types};
use crate::unpack::{UnpackKind, UnpackValue};

use super::context::InferContext;
use super::diagnostic::INVALID_ASSIGNMENT;

/// Unpacks the value expression type to their respective targets.
pub(crate) struct Unpacker<'db, 'ast> {
    context: InferContext<'db, 'ast>,
    target_scope: ScopeId<'db>,
    value_scope: ScopeId<'db>,
    targets: FxHashMap<ScopedExpressionId, Type<'db>>,
}

impl<'db, 'ast> Unpacker<'db, 'ast> {
    pub(crate) fn new(
        db: &'db dyn Db,
        target_scope: ScopeId<'db>,
        value_scope: ScopeId<'db>,
        module: &'ast ParsedModuleRef,
    ) -> Self {
        Self {
            context: InferContext::new(db, target_scope, module),
            targets: FxHashMap::default(),
            target_scope,
            value_scope,
        }
    }

    fn db(&self) -> &'db dyn Db {
        self.context.db()
    }

    fn module(&self) -> &'ast ParsedModuleRef {
        self.context.module()
    }

    /// Unpack the value to the target expression.
    pub(crate) fn unpack(&mut self, target: &ast::Expr, value: UnpackValue<'db>) {
        debug_assert!(
            matches!(target, ast::Expr::List(_) | ast::Expr::Tuple(_)),
            "Unpacking target must be a list or tuple expression"
        );

        let value_type = infer_expression_types(self.db(), value.expression()).expression_type(
            value.scoped_expression_id(self.db(), self.value_scope, self.module()),
        );

        let value_type = match value.kind() {
            UnpackKind::Assign => {
                if self.context.in_stub()
                    && value
                        .expression()
                        .node_ref(self.db(), self.module())
                        .is_ellipsis_literal_expr()
                {
                    Type::unknown()
                } else {
                    value_type
                }
            }
            UnpackKind::Iterable => value_type.try_iterate(self.db()).unwrap_or_else(|err| {
                err.report_diagnostic(
                    &self.context,
                    value_type,
                    value.as_any_node_ref(self.db(), self.module()),
                );
                err.fallback_element_type(self.db())
            }),
            UnpackKind::ContextManager => value_type.try_enter(self.db()).unwrap_or_else(|err| {
                err.report_diagnostic(
                    &self.context,
                    value_type,
                    value.as_any_node_ref(self.db(), self.module()),
                );
                err.fallback_enter_type(self.db())
            }),
        };

        self.unpack_inner(
            target,
            value.as_any_node_ref(self.db(), self.module()),
            value_type,
        );
    }

    fn unpack_inner(
        &mut self,
        target: &ast::Expr,
        value_expr: AnyNodeRef<'_>,
        value_ty: Type<'db>,
    ) {
        match target {
            ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                self.targets.insert(
                    target.scoped_expression_id(self.db(), self.target_scope),
                    value_ty,
                );
            }
            ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
                self.unpack_inner(value, value_expr, value_ty);
            }
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let target_len = match elts.iter().position(ast::Expr::is_starred_expr) {
                    Some(starred_index) => {
                        TupleLength::Variable(starred_index, elts.len() - (starred_index + 1))
                    }
                    None => TupleLength::Fixed(elts.len()),
                };
                let mut splatter = Splatter::new(self.db(), target_len);

                let unpack_types = match value_ty {
                    Type::Union(union_ty) => union_ty.elements(self.db()),
                    _ => std::slice::from_ref(&value_ty),
                };

                for ty in unpack_types.iter().copied() {
                    // Deconstruct certain types to delegate the inference back to the tuple type
                    // for correct handling of starred expressions.
                    let ty = match ty {
                        Type::StringLiteral(string_literal_ty) => {
                            // We could go further and deconstruct to an array of `StringLiteral`
                            // with each individual character, instead of just an array of
                            // `LiteralString`, but there would be a cost and it's not clear that
                            // it's worth it.
                            TupleType::from_elements(
                                self.db(),
                                std::iter::repeat_n(
                                    Type::LiteralString,
                                    string_literal_ty.python_len(self.db()),
                                ),
                            )
                        }
                        _ => ty,
                    };

                    if let Type::Tuple(tuple_ty) = ty {
                        let tuple = tuple_ty.tuple(self.db());
                        if let Err(err) = splatter.add_values(tuple) {
                            splatter.add_unknown();
                            if let Some(builder) =
                                self.context.report_lint(&INVALID_ASSIGNMENT, target)
                            {
                                match err {
                                    SplatterError::TooManyValues => {
                                        let mut diag =
                                            builder.into_diagnostic("Too many values to unpack");
                                        diag.set_primary_message(format_args!(
                                            "Expected {}",
                                            target_len.display_minimum(),
                                        ));
                                        diag.annotate(self.context.secondary(value_expr).message(
                                            format_args!("Got {}", tuple.len().display_minimum()),
                                        ));
                                    }
                                    SplatterError::TooFewValues => {
                                        let mut diag =
                                            builder.into_diagnostic("Not enough values to unpack");
                                        diag.set_primary_message(format_args!(
                                            "Expected {}",
                                            target_len.display_minimum(),
                                        ));
                                        diag.annotate(self.context.secondary(value_expr).message(
                                            format_args!("Got {}", tuple.len().display_maximum()),
                                        ));
                                    }
                                }
                            }
                        }
                    } else {
                        let ty = if ty.is_literal_string() {
                            Type::LiteralString
                        } else {
                            ty.try_iterate(self.db()).unwrap_or_else(|err| {
                                err.report_diagnostic(&self.context, ty, value_expr);
                                err.fallback_element_type(self.db())
                            })
                        };
                        splatter.add_list_element(ty);
                    }
                }

                // We constructed splatter above using the length of elts, so the zip should
                // consume the same number of elements from each.
                for (target, value) in elts.iter().zip(splatter.into_all_elements()) {
                    let value_ty = match value {
                        TupleElement::Variable(value) => KnownClass::List.to_specialized_instance(
                            self.db(),
                            [value.try_build().unwrap_or_else(Type::unknown)],
                        ),
                        TupleElement::Fixed(value)
                        | TupleElement::Prefix(value)
                        | TupleElement::Suffix(value) => {
                            value.try_build().unwrap_or_else(Type::unknown)
                        }
                    };
                    self.unpack_inner(target, value_expr, value_ty);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn finish(mut self) -> UnpackResult<'db> {
        self.targets.shrink_to_fit();
        UnpackResult {
            diagnostics: self.context.finish(),
            targets: self.targets,
            cycle_fallback_type: None,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, salsa::Update)]
pub(crate) struct UnpackResult<'db> {
    targets: FxHashMap<ScopedExpressionId, Type<'db>>,
    diagnostics: TypeCheckDiagnostics,

    /// The fallback type for missing expressions.
    ///
    /// This is used only when constructing a cycle-recovery `UnpackResult`.
    cycle_fallback_type: Option<Type<'db>>,
}

impl<'db> UnpackResult<'db> {
    /// Returns the inferred type for a given sub-expression of the left-hand side target
    /// of an unpacking assignment.
    ///
    /// # Panics
    ///
    /// May panic if a scoped expression ID is passed in that does not correspond to a sub-
    /// expression of the target.
    #[track_caller]
    pub(crate) fn expression_type(&self, expr_id: ScopedExpressionId) -> Type<'db> {
        self.try_expression_type(expr_id).expect(
            "expression should belong to this `UnpackResult` and \
            `Unpacker` should have inferred a type for it",
        )
    }

    pub(crate) fn try_expression_type(&self, expr_id: ScopedExpressionId) -> Option<Type<'db>> {
        self.targets
            .get(&expr_id)
            .copied()
            .or(self.cycle_fallback_type)
    }

    /// Returns the diagnostics in this unpacking assignment.
    pub(crate) fn diagnostics(&self) -> &TypeCheckDiagnostics {
        &self.diagnostics
    }

    pub(crate) fn cycle_fallback(cycle_fallback_type: Type<'db>) -> Self {
        Self {
            targets: FxHashMap::default(),
            diagnostics: TypeCheckDiagnostics::default(),
            cycle_fallback_type: Some(cycle_fallback_type),
        }
    }
}
