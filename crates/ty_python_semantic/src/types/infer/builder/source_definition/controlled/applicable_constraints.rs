//! Applicable constraints use prepared places and retain the ordinary reduction order.

use std::ops::ControlFlow;

use ruff_python_ast as ast;
use rustc_hash::FxHashMap;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::{PlaceExpr, PlaceExprCursor, PlaceExprRef, ScopedPlaceId};
use ty_python_core::predicate::Predicates;
use ty_python_core::reachability_constraints::{
    ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_core::scope::FileScopeId;
use ty_python_core::{
    ApplicableConstraints, BindingWithConstraints, BindingWithConstraintsIterator,
    NarrowingEvaluator, Truthiness,
};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::source_effects::{SourcePlaceEffects, reachability_with};
use crate::reachability::narrowing_entry::{NarrowingEntryFacts, narrow_type_by_constraint_with};
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::infer::builder::applicable_constraints::{
    ApplicableConstraintsEffects, ApplicableConstraintsFacts,
    narrow_place_with_applicable_constraints_with,
};
use crate::types::infer::builder::source_binding::SourceBindingEffects;
use crate::types::set_theoretic::builder::controlled_union::{
    UnionFacts, add_in_place_with, try_build_with,
};
use crate::types::{Type, UnionBuilder};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn construct_place(
        &self,
        expression: ast::ExprRef<'_>,
    ) -> RunResult<Option<PlaceExpr>> {
        let mut cursor = self
            .initialize_value(|| PlaceExprCursor::new(expression))
            .await?;
        loop {
            let step = cursor.prepare();
            let (work, bytes) = step
                .cost()
                .ok_or(RunError::Contract("place construction quotation overflow"))?;
            let result = self
                .local(work, bytes, || {
                    let result = step.advance();
                    #[cfg(test)]
                    super::observations::observe(self.db(), super::observations::Event::PlaceStep);
                    result
                })
                .await?;
            if let ControlFlow::Break(place) = result {
                return Ok(place);
            }
        }
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ApplicableConstraintsEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn place_expression(&self, target: ast::ExprRef<'_>) -> RunResult<Option<PlaceExpr>> {
        self.construct_place(target).await
    }

    async fn narrow_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expr: PlaceExprRef<'_>,
        ty: Type<'db>,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
    ) -> RunResult<Type<'db>> {
        narrow_place_with_applicable_constraints_with(
            builder,
            expr,
            ty,
            constraint_keys,
            ApplicableConstraintsFacts,
            self,
        )
        .await
    }

    async fn next_constraint(
        &self,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
        cursor: &mut usize,
    ) -> RunResult<Option<(FileScopeId, ConstraintKey)>> {
        self.local(2, 0, || {
            let next = constraint_keys.get(*cursor).copied();
            if next.is_some() {
                *cursor += 1;
            }
            next
        })
        .await
    }

    async fn applicable_constraints(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expr: PlaceExprRef<'_>,
        scope: FileScopeId,
        key: ConstraintKey,
    ) -> RunResult<(ScopedPlaceId, ApplicableConstraints<'db, 'db>)> {
        self.environment_program(builder.program_environment())
            .await?;
        let (use_def, table) = self
            .local(2, 0, || {
                (
                    builder.index.use_def_map(scope),
                    builder.index.place_table(scope),
                )
            })
            .await?;
        let lookup_work = Self::checked(table.lookup_work(expr))?;
        let place = self
            .local(lookup_work, 0, || table.place_id(expr))
            .await?
            .ok_or(RunError::Contract("applicable constraint place is missing"))?;
        let mut work = 4;
        if let ConstraintKey::NestedScope(nested_scope) = key {
            let mut ancestors = self
                .local(1, 0, || builder.index.ancestor_scopes(nested_scope))
                .await?;
            loop {
                // Admit both measurement and the same ancestor step during snapshot lookup.
                let reached_enclosing = self
                    .local(8, 0, || {
                        ancestors
                            .next()
                            .is_none_or(|(id, ancestor)| id == scope || !ancestor.is_eager())
                    })
                    .await?;
                if reached_enclosing {
                    break;
                }
            }
            work = Self::checked(
                lookup_work
                    .checked_add(Self::checked(
                        builder
                            .index
                            .enclosing_snapshot_lookup_work()
                            .checked_mul(8),
                    )?)
                    .and_then(|work| work.checked_add(8)),
            )?;
        }
        let constraints = self
            .local(work, 0, || {
                use_def.applicable_constraints(key, scope, expr, builder.index)
            })
            .await?;
        Ok((place, constraints))
    }

    async fn next_binding<'map>(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
    ) -> RunResult<Option<BindingWithConstraints<'map, 'db>>> {
        self.local(4, 0, || bindings.next()).await
    }

    async fn reachability(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        constraints: &ReachabilityConstraints,
        predicates: &Predicates<'db>,
        constraint: ScopedReachabilityConstraintId,
    ) -> RunResult<Truthiness> {
        let cache = SourceBindingEffects::reachability_cache(self, builder).await?;
        reachability_with(self, Some(cache), constraints, predicates, constraint).await
    }

    async fn is_discarded_dict_key_assignment(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        SourcePlaceEffects::is_discarded_dict_key_assignment(self, definition).await
    }

    async fn binding_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<Type<'db>> {
        SourcePlaceEffects::binding_type(self, definition).await
    }

    async fn is_loop_header(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        Ok(SourcePlaceEffects::definition_kind(self, definition)
            .await?
            .is_loop_header())
    }

    async fn loop_header_fallback_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _definition: Definition<'db>,
        _ty: Type<'db>,
        _fallbacks: &mut FxHashMap<Definition<'db>, Type<'db>>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::LoopHeader).await
    }

    async fn union_two(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn narrow(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        constraint: &NarrowingEvaluator<'_, 'db>,
        ty: Type<'db>,
        place: ScopedPlaceId,
    ) -> RunResult<Type<'db>> {
        narrow_type_by_constraint_with(
            builder.program_environment(),
            constraint,
            ty,
            place,
            NarrowingEntryFacts,
            self,
        )
        .await
    }

    async fn new_union(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<UnionBuilder<'db>> {
        let work = Self::checked(
            size_of::<UnionBuilder<'db>>()
                .checked_mul(2)
                .and_then(|work| work.checked_add(1)),
        )?;
        self.local(work, 0, || {
            UnionBuilder::new(self.db(), builder.program_environment())
        })
        .await
    }

    async fn union_add(&self, union: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        add_in_place_with(union, ty, UnionFacts, self).await
    }

    async fn union_build(&self, union: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        Ok(try_build_with(union, UnionFacts, self)
            .await?
            .unwrap_or(Type::Never))
    }
}
