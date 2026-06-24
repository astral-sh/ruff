use std::borrow::Cow;

use ruff_db::parsed::ParsedModuleRef;
use rustc_hash::FxHashMap;

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, AnyNodeRef};

use crate::Db;
use crate::types::infer::{ExpressionInference, FrozenMap};
use crate::types::tuple::{ResizeTupleError, Tuple, TupleLength, TupleSpec, TupleUnpacker};
use crate::types::{Type, TypeCheckDiagnostics, TypeContext, infer_expression_types};
use ty_python_core::ExpressionNodeKey;
use ty_python_core::scope::ScopeId;
use ty_python_core::unpack::{UnpackKind, UnpackValue};

use super::context::InferContext;
use super::diagnostic::INVALID_ASSIGNMENT;

/// Unpacks the value expression type to their respective targets.
pub(crate) struct Unpacker<'db, 'ast> {
    context: InferContext<'db, 'ast>,
    targets: FxHashMap<ExpressionNodeKey, Type<'db>>,
}

/// Records an `Unknown` type for every expression in a malformed unpack target subtree.
struct UnknownTargetCollector<'db, 'map> {
    targets: &'map mut FxHashMap<ExpressionNodeKey, Type<'db>>,
}

impl<'ast> Visitor<'ast> for UnknownTargetCollector<'_, '_> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        self.targets.insert(expr.into(), Type::unknown());
        visitor::walk_expr(self, expr);
    }
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

        let value_inference =
            infer_expression_types(self.db(), value.expression(), TypeContext::default());
        let value_expr = value.expression().node_ref(self.db()).node(self.module());

        if matches!(value.kind(), UnpackKind::Assign)
            && self.unpack_assignment_sequence_from_inference(target, value_expr, value_inference)
        {
            return;
        }

        let value_type = value_inference.expression_type(value_expr);

        let value_type = match value.kind() {
            UnpackKind::Assign => {
                if self.context.in_stub() && value_expr.is_ellipsis_literal_expr() {
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

        self.unpack_inner(target, value_expr.into(), value_type);
    }

    /// In regular tuple assignments like `a, b = 1, 2` {or even `a, (b, c) = 1, (2, 3)`}, map each
    /// expression on the left individually to the corresponding element type on the right, rather
    /// than trying to walk the tuple type of the entire RHS.
    ///
    /// We avoid infinitely growing types in cycle resolution by preserving only the
    /// topmost/outermost part of types that have `Divergent` components. For example, if the
    /// assignment `x = (0, x)` shows up in a loop, we need to avoid infinite looping on a
    /// never-ending type like `tuple[Literal[0], tuple[Literal[0], tuple[...]]]`. So when we see
    /// an intermediate result like `tuple[Literal[0], tuple[Literal[0], Divergent]]`, we simplify
    /// that to `tuple[Literal[0], Divergent]`.
    ///
    /// The problem here is that, when `Divergent` shows up on the RHS, we end up simplifying that
    /// tuple to e.g. `tuple[Divergent, Divergent]`. If we proceed by unpacking that type, we won't
    /// accumulate any information about the elements, and the user will end up seeing `Divergent`
    /// as the type of their variables.
    ///
    /// This function avoids that problem by walking the AST on the RHS and looking directly at the
    /// individual element types. That gives us one more level of structure for those types, which
    /// is enough to resolve a lot of common cycles.
    fn unpack_assignment_sequence_from_inference(
        &mut self,
        target: &ast::Expr,
        value_expr: &ast::Expr,
        value_inference: &ExpressionInference<'db>,
    ) -> bool {
        match target {
            ast::Expr::Name(_) | ast::Expr::Attribute(_) | ast::Expr::Subscript(_) => {
                self.targets
                    .insert(target.into(), value_inference.expression_type(value_expr));
                true
            }
            ast::Expr::List(ast::ExprList { elts, .. })
            | ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let Some(values) = sequence_elts(value_expr) else {
                    return false;
                };
                self.unpack_fixed_sequence_from_inference(elts, values, value_inference)
            }
            _ => false,
        }
    }

    fn unpack_fixed_sequence_from_inference(
        &mut self,
        targets: &[ast::Expr],
        values: &[ast::Expr],
        value_inference: &ExpressionInference<'db>,
    ) -> bool {
        if targets.len() != values.len()
            || targets.iter().any(ast::Expr::is_starred_expr)
            || values.iter().any(ast::Expr::is_starred_expr)
        {
            return false;
        }

        // Even `a, b = 1, 2` recurses through this helper. `.all()` short-circuits,
        // so in nested cases an earlier element may update `self.targets` before a
        // later element falls back to the general unpacking path. That's harmless
        // because the fallback recomputes the full unpacking and overwrites any
        // partial entries.
        targets.iter().zip(values).all(|(target, value_expr)| {
            self.unpack_assignment_sequence_from_inference(target, value_expr, value_inference)
        })
    }

    /// Records `Unknown` for a malformed unpack target and all of its descendant expressions.
    fn record_unknown_target_subtree(&mut self, target: &ast::Expr) {
        UnknownTargetCollector {
            targets: &mut self.targets,
        }
        .visit_expr(target);
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
            _ => {
                // Recovered syntax can still create assignment definitions for descendants of
                // malformed targets. Give the whole subtree an unknown type so later lookups
                // don't panic.
                self.record_unknown_target_subtree(target);
            }
        }
    }

    pub(crate) fn finish(self) -> UnpackResult<'db> {
        UnpackResult {
            diagnostics: self.context.finish(),
            targets: FrozenMap::from(self.targets),
            cycle_recovery: None,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct UnpackResult<'db> {
    targets: FrozenMap<ExpressionNodeKey, Type<'db>>,
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
            targets: FrozenMap::default(),
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

/// Extract the element slice from a list or tuple expression.
fn sequence_elts(expr: &ast::Expr) -> Option<&[ast::Expr]> {
    match expr {
        ast::Expr::List(list) => Some(&list.elts),
        ast::Expr::Tuple(tuple) => Some(&tuple.elts),
        _ => None,
    }
}
