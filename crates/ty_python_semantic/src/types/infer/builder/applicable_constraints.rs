//! Narrow places through their enclosing scopes and the bindings visible at each constraint.

use std::convert::Infallible;

use ruff_python_ast as ast;
use rustc_hash::FxHashMap;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::{PlaceExpr, PlaceExprRef, ScopedPlaceId};
use ty_python_core::predicate::Predicates;
use ty_python_core::reachability_constraints::{
    ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_core::scope::FileScopeId;
use ty_python_core::{
    ApplicableConstraints, BindingWithConstraints, BindingWithConstraintsIterator,
    NarrowingEvaluator, Truthiness,
};

use super::TypeInferenceBuilder;
use crate::reachability::evaluate_reachability_with_cache;
use crate::types::narrow::NarrowingEvaluatorExtension;
use crate::types::{Type, UnionBuilder, UnionType, binding_type, is_discarded_dict_key_assignment};

pub(in crate::types::infer) struct ApplicableConstraintsFacts;
pub(super) struct OrdinaryApplicableConstraintsEffects;

shared_semantic_family! {
    #[synchronous(SynchronousApplicableConstraintsEffects)]
    pub(in crate::types::infer) trait ApplicableConstraintsEffects<'db, 'ast> {
        type Error;
        #[operation(local)]
        async fn place_expression(&self, target: ast::ExprRef<'_>) -> Result<Option<PlaceExpr>, Self::Error>;
        #[operation(source)]
        async fn narrow_place(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expr: PlaceExprRef<'_>, ty: Type<'db>, constraint_keys: &[(FileScopeId, ConstraintKey)]) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_constraint(&self, constraint_keys: &[(FileScopeId, ConstraintKey)], cursor: &mut usize) -> Result<Option<(FileScopeId, ConstraintKey)>, Self::Error>;
        #[operation(local)]
        async fn applicable_constraints(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expr: PlaceExprRef<'_>, scope: FileScopeId, key: ConstraintKey) -> Result<(ScopedPlaceId, ApplicableConstraints<'db, 'db>), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_binding<'map>(&self, bindings: &mut BindingWithConstraintsIterator<'map, 'db>) -> Result<Option<BindingWithConstraints<'map, 'db>>, Self::Error>;
        #[operation(source)]
        async fn reachability(&self, builder: &TypeInferenceBuilder<'db, 'ast>, constraints: &ReachabilityConstraints, predicates: &Predicates<'db>, constraint: ScopedReachabilityConstraintId) -> Result<Truthiness, Self::Error>;
        #[operation(child)]
        async fn is_discarded_dict_key_assignment(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn binding_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn is_loop_header(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn loop_header_fallback_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, ty: Type<'db>, fallbacks: &mut FxHashMap<Definition<'db>, Type<'db>>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn union_two(&self, builder: &TypeInferenceBuilder<'db, 'ast>, first: Type<'db>, second: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn narrow(&self, builder: &TypeInferenceBuilder<'db, 'ast>, constraint: &NarrowingEvaluator<'_, 'db>, ty: Type<'db>, place: ScopedPlaceId) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn new_union(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<UnionBuilder<'db>, Self::Error>;
        #[operation(source)]
        async fn union_add(&self, union: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_build(&self, union: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ApplicableConstraintsFacts {
        fn place_ref<'expr>(&self, expr: &'expr PlaceExpr) -> PlaceExprRef<'expr> {
            PlaceExprRef::from(expr)
        }
        fn reachability_constraints<'map, 'db>(&self, bindings: &BindingWithConstraintsIterator<'map, 'db>) -> &'map ReachabilityConstraints {
            bindings.reachability_constraints()
        }
        fn predicates<'map, 'db>(&self, bindings: &BindingWithConstraintsIterator<'map, 'db>) -> &'map Predicates<'db> {
            bindings.predicates()
        }
        fn is_always_false(&self, reachability: Truthiness) -> bool {
            reachability.is_always_false()
        }
        fn empty_loop_header_fallbacks<'db>(&self) -> FxHashMap<Definition<'db>, Type<'db>> {
            FxHashMap::default()
        }
    }

    #[synchronous(narrow_expr_with_applicable_constraints_sync)]
    #[capabilities(effects = ApplicableConstraintsEffects, facts = ApplicableConstraintsFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn narrow_expr_with_applicable_constraints_with<'db, 'ast, E: ApplicableConstraintsEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>,
        target: ast::ExprRef<'_>,
        target_ty: Type<'db>,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
        facts: ApplicableConstraintsFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        if let Some(place_expr) = effects.place_expression(target).await? {
            effects.narrow_place(builder, facts.place_ref(&place_expr), target_ty, constraint_keys).await
        } else {
            Ok(target_ty)
        }
    }

    #[synchronous(narrow_place_with_applicable_constraints_sync)]
    #[capabilities(effects = ApplicableConstraintsEffects, facts = ApplicableConstraintsFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn narrow_place_with_applicable_constraints_with<'db, 'ast, E: ApplicableConstraintsEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expr: PlaceExprRef<'_>,
        target_ty: Type<'db>,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
        facts: ApplicableConstraintsFacts,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        #[passive_state]
        let mut ty = target_ty;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(scope_constraint) = effects.next_constraint(constraint_keys, &mut cursor).await? {
            let (enclosing_scope_file_id, constraint_key) = scope_constraint;
            let (place, constraints) = effects.applicable_constraints(builder, expr, enclosing_scope_file_id, constraint_key).await?;
            match constraints {
                ApplicableConstraints::UnboundBinding(constraint) => {
                    ty = effects.narrow(builder, &constraint, ty, place).await?;
                }
                // Performs narrowing based on constrained bindings.
                // This handling must be performed even if narrowing is attempted and failed using `infer_place_load`.
                // The result of `infer_place_load` can be applied as is only when its boundness is `Bound`.
                // For example, this handling is required in the following case:
                // ```python
                // class C:
                //     x: int | None = None
                // c = C()
                // # c.x: int | None = <unbound>
                // if c.x is None:
                //     c.x = 1
                // # else: c.x: int = <unbound>
                // # `c.x` is not definitely bound here
                // reveal_type(c.x)  # revealed: int
                // ```
                ApplicableConstraints::ConstrainedBindings(mut bindings) => {
                    let reachability_constraints = facts.reachability_constraints(&bindings);
                    let predicates = facts.predicates(&bindings);
                    let mut union = effects.new_union(builder).await?;
                    let mut loop_header_fallbacks = facts.empty_loop_header_fallbacks();
                    #[cursor_loop]
                    while let Some(binding) = effects.next_binding(&mut bindings).await? {
                        let static_reachability = effects.reachability(
                            builder,
                            reachability_constraints,
                            predicates,
                            binding.reachability_constraint,
                        ).await?;
                        if facts.is_always_false(static_reachability) {
                            continue;
                        }
                        let binding_ty = match binding.binding {
                            DefinitionState::Defined(definition) => {
                                if effects.is_discarded_dict_key_assignment(builder, definition).await? {
                                    ty
                                } else {
                                    #[passive_state]
                                    let mut binding_ty = effects.binding_type(builder, definition).await?;
                                    if effects.is_loop_header(builder, definition).await? {
                                        let fallback_ty = effects.loop_header_fallback_type(
                                            builder,
                                            definition,
                                            ty,
                                            &mut loop_header_fallbacks,
                                        ).await?;
                                        binding_ty = effects.union_two(builder, binding_ty, fallback_ty).await?;
                                    }
                                    binding_ty
                                }
                            }
                            DefinitionState::Undefined | DefinitionState::Deleted => ty,
                        };
                        let narrowed_ty = effects.narrow(builder, &binding.narrowing_constraint, binding_ty, place).await?;
                        effects.union_add(&mut union, narrowed_ty).await?;
                    }
                    // If there are no visible bindings, the union becomes `Never`.
                    // Since an unbound binding is recorded even for an undefined place,
                    // this can only happen if the code is unreachable
                    // and therefore it is correct to set the result to `Never`.
                    ty = effects.union_build(union).await?;
                }
            }
        }
        Ok(ty)
    }
}

impl<'db, 'ast> SynchronousApplicableConstraintsEffects<'db, 'ast>
    for OrdinaryApplicableConstraintsEffects
{
    type Error = Infallible;

    fn place_expression(&self, target: ast::ExprRef<'_>) -> Result<Option<PlaceExpr>, Self::Error> {
        Ok(PlaceExpr::try_from_expr(target))
    }

    fn narrow_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expr: PlaceExprRef<'_>,
        ty: Type<'db>,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.narrow_place_with_applicable_constraints(expr, ty, constraint_keys))
    }

    fn next_constraint(
        &self,
        constraint_keys: &[(FileScopeId, ConstraintKey)],
        cursor: &mut usize,
    ) -> Result<Option<(FileScopeId, ConstraintKey)>, Self::Error> {
        let next = constraint_keys.get(*cursor).copied();
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }

    fn applicable_constraints(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expr: PlaceExprRef<'_>,
        scope: FileScopeId,
        key: ConstraintKey,
    ) -> Result<(ScopedPlaceId, ApplicableConstraints<'db, 'db>), Self::Error> {
        let use_def = builder.index.use_def_map(scope);
        let place_table = builder.index.place_table(scope);
        let place = place_table.place_id(expr).unwrap();
        Ok((
            place,
            use_def.applicable_constraints(key, scope, expr, builder.index),
        ))
    }

    fn next_binding<'map>(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<Option<BindingWithConstraints<'map, 'db>>, Self::Error> {
        Ok(bindings.next())
    }

    fn reachability(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        constraints: &ReachabilityConstraints,
        predicates: &Predicates<'db>,
        constraint: ScopedReachabilityConstraintId,
    ) -> Result<Truthiness, Self::Error> {
        Ok(evaluate_reachability_with_cache(
            builder.db(),
            Some(builder.reachability_cache()),
            constraints,
            predicates,
            constraint,
        ))
    }

    fn is_discarded_dict_key_assignment(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(is_discarded_dict_key_assignment(builder.db(), definition))
    }

    fn binding_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(binding_type(builder.db(), definition))
    }

    fn is_loop_header(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(definition.kind(builder.db()).is_loop_header())
    }

    fn loop_header_fallback_type(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        ty: Type<'db>,
        fallbacks: &mut FxHashMap<Definition<'db>, Type<'db>>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.loop_header_fallback_type(definition, ty, fallbacks))
    }

    fn union_two(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_elements(
            builder.db(),
            builder.program_environment(),
            [first, second],
        ))
    }

    fn narrow(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        constraint: &NarrowingEvaluator<'_, 'db>,
        ty: Type<'db>,
        place: ScopedPlaceId,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(constraint.narrow(builder.db(), builder.program_environment(), ty, place))
    }

    fn new_union(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<UnionBuilder<'db>, Self::Error> {
        Ok(UnionBuilder::new(
            builder.db(),
            builder.program_environment(),
        ))
    }

    fn union_add(&self, union: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error> {
        union.add_in_place(ty);
        Ok(())
    }

    fn union_build(&self, union: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(union.build())
    }
}
