use std::borrow::Cow;

use ruff_db::parsed::ParsedModuleRef;
use rustc_hash::FxHashMap;

use ruff_python_ast::{self as ast, AnyNodeRef};

use crate::Db;
use crate::semantic_index::ast_ids::node_key::ExpressionNodeKey;
use crate::semantic_index::scope::ScopeId;
use crate::types::tuple::{ResizeTupleError, Tuple, TupleLength, TupleSpec, TupleUnpacker};
use crate::types::{Type, TypeCheckDiagnostics, TypeContext, infer_expression_types};
use crate::unpack::{UnpackKind, UnpackValue};

use super::context::InferContext;
use super::diagnostic::INVALID_ASSIGNMENT;

/// Unpacks the value expression type to their respective targets.
pub(crate) struct Unpacker<'db, 'ast> {
    context: InferContext<'db, 'ast>,
    targets: FxHashMap<ExpressionNodeKey, Type<'db>>,
}

impl<'db, 'ast> Unpacker<'db, 'ast> {
    pub(crate) fn new(
        db: &'db dyn Db,
        target_scope: ScopeId<'db>,
        module: &'ast ParsedModuleRef,
    ) -> Self {
        Self {
            context: InferContext::new(db, target_scope, module),
            targets: FxHashMap::default(),
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

        let value_type =
            infer_expression_types(self.db(), value.expression(), TypeContext::default())
                .expression_type(value.expression().node_ref(self.db(), self.module()));

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
            UnpackKind::Iterable { mode } => value_type
                .try_iterate_with_mode(self.db(), mode)
                .map(|tuple| tuple.homogeneous_element_type(self.db()))
                .unwrap_or_else(|err| {
                    err.report_diagnostic(
                        &self.context,
                        value_type,
                        value.as_any_node_ref(self.db(), self.module()),
                    );
                    err.fallback_element_type(self.db())
                }),
            UnpackKind::ContextManager { mode } => value_type
                .try_enter_with_mode(self.db(), mode)
                .unwrap_or_else(|err| {
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
                self.targets.insert(target.into(), value_ty);
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
                let mut unpacker = TupleUnpacker::new(self.db(), target_len);

                // N.B. `Type::try_iterate` internally handles unions, but in a lossy way.
                // For our purposes here, we get better error messages and more precise inference
                // if we manually map over the union and call `try_iterate` on each union element.
                // See <https://github.com/astral-sh/ruff/pull/20377#issuecomment-3401380305>
                // for more discussion.
                let unpack_types = match value_ty {
                    Type::Union(union_ty) => union_ty.elements(self.db()),
                    _ => std::slice::from_ref(&value_ty),
                };

                for ty in unpack_types.iter().copied() {
                    let tuple = ty.try_iterate(self.db()).unwrap_or_else(|err| {
                        err.report_diagnostic(&self.context, ty, value_expr);
                        Cow::Owned(TupleSpec::homogeneous(err.fallback_element_type(self.db())))
                    });

                    if let Err(err) = unpacker.unpack_tuple(tuple.as_ref()) {
                        unpacker
                            .unpack_tuple(&Tuple::homogeneous(Type::unknown()))
                            .expect("adding a homogeneous tuple should always succeed");
                        if let Some(builder) = self.context.report_lint(&INVALID_ASSIGNMENT, target)
                        {
                            match err {
                                ResizeTupleError::TooManyValues => {
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
                                ResizeTupleError::TooFewValues => {
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
                }

                // We constructed unpacker above using the length of elts, so the zip should
                // consume the same number of elements from each.
                for (target, value_ty) in elts.iter().zip(unpacker.into_types()) {
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
            cycle_recovery: None,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct UnpackResult<'db> {
    targets: FxHashMap<ExpressionNodeKey, Type<'db>>,
    diagnostics: TypeCheckDiagnostics,

    /// The fallback type for missing expressions.
    ///
    /// This is used only when constructing a cycle-recovery `UnpackResult`.
    cycle_recovery: Option<Type<'db>>,
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
    pub(crate) fn expression_type(&self, expr_id: impl Into<ExpressionNodeKey>) -> Type<'db> {
        self.try_expression_type(expr_id).expect(
            "expression should belong to this `UnpackResult` and \
            `Unpacker` should have inferred a type for it",
        )
    }

    pub(crate) fn try_expression_type(
        &self,
        expr: impl Into<ExpressionNodeKey>,
    ) -> Option<Type<'db>> {
        self.targets
            .get(&expr.into())
            .copied()
            .or(self.cycle_recovery)
    }

    /// Returns the diagnostics in this unpacking assignment.
    pub(crate) fn diagnostics(&self) -> &TypeCheckDiagnostics {
        &self.diagnostics
    }

    pub(crate) fn cycle_initial(cycle_recovery: Type<'db>) -> Self {
        Self {
            targets: FxHashMap::default(),
            diagnostics: TypeCheckDiagnostics::default(),
            cycle_recovery: Some(cycle_recovery),
        }
    }

    pub(crate) fn cycle_normalized(
        mut self,
        db: &'db dyn Db,
        previous_cycle_result: &UnpackResult<'db>,
        cycle: &salsa::Cycle,
    ) -> Self {
        for (expr, ty) in &mut self.targets {
            let previous_ty = previous_cycle_result.expression_type(*expr);
            *ty = ty.cycle_normalized(db, previous_ty, cycle);
        }

        self
    }
}
