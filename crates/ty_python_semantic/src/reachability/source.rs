//! Reachability decisions shared by ordinary queries and controlled source inference.

use std::convert::Infallible;
use std::ops::ControlFlow;

use ruff_index::IndexSlice;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::Truthiness;
use ty_python_core::expression::Expression;
use ty_python_core::predicate::{
    CallableAndCallExpr, PatternPredicate, Predicate, PredicateNode, ScopedPredicateId,
    StarImportPlaceholderPredicate,
};
use ty_python_core::reachability_constraints::{
    InteriorNode, ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_core::scope::ScopeId;

use super::{ReachabilityCacheKey, ReachabilityEvaluationCache};
use crate::types::{
    CallableSignature, CallableType, CallableTypes, Signature, Type, TypeContext,
    infer_same_file_expression_type,
};
use crate::{Db, ProgramEnvironment};

pub(crate) fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

pub(crate) struct ReachabilityFacts;

pub(crate) struct PathCursor {
    id: ScopedReachabilityConstraintId,
    use_checkpoint: bool,
    visited: usize,
    result: Truthiness,
}

impl PathCursor {
    pub(crate) fn new(id: ScopedReachabilityConstraintId, use_checkpoint: bool) -> Self {
        Self {
            id,
            use_checkpoint,
            visited: 0,
            result: Truthiness::Ambiguous,
        }
    }

    pub(crate) fn next(&mut self, constraints: &ReachabilityConstraints) -> Option<InteriorNode> {
        if let Some(result) = super::terminal_reachability(self.id) {
            // Every exhausted cursor records the actual terminal before its result is read.
            self.result = result;
            None
        } else {
            Some(constraints.get_interior_node(self.id))
        }
    }

    pub(crate) fn advance(&mut self, id: ScopedReachabilityConstraintId) {
        self.id = id;
        self.use_checkpoint = true;
        self.visited += 1;
    }
}

#[cfg(feature = "experimental-analysis")]
pub(crate) fn is_checkpoint(
    call_predicates: Option<&[ScopedPredicateId]>,
    predicate: ScopedPredicateId,
    visited: usize,
) -> bool {
    super::is_reachability_checkpoint(call_predicates, predicate, visited)
}

shared_semantic_family! {
    #[synchronous(SynchronousReachabilityEffects)]
    pub(crate) trait ReachabilityEffects<'db> {
        type Error;
        #[operation(local)]
        async fn node(&self, constraints: &ReachabilityConstraints, id: ScopedReachabilityConstraintId) -> Result<InteriorNode, Self::Error>;
        #[operation(local)]
        async fn predicate(&self, predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>, id: ScopedPredicateId) -> Result<Predicate<'db>, Self::Error>;
        #[operation(source)]
        async fn predicate_scope(&self, predicate: &Predicate<'db>) -> Result<ScopeId<'db>, Self::Error>;
        #[operation(local)]
        async fn cache_key(&self, cache: &ReachabilityEvaluationCache<'db>, scope: ScopeId<'db>, constraints: &ReachabilityConstraints, id: ScopedReachabilityConstraintId) -> Result<ReachabilityCacheKey, Self::Error>;
        #[operation(local)]
        async fn cache_lookup(&self, cache: &ReachabilityEvaluationCache<'db>, key: ReachabilityCacheKey) -> Result<Option<Truthiness>, Self::Error>;
        #[operation(local)]
        async fn cache_insert(&self, cache: &ReachabilityEvaluationCache<'db>, key: ReachabilityCacheKey, result: Truthiness) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn evaluate_constraint(&self, scope: ScopeId<'db>, id: ScopedReachabilityConstraintId) -> Result<Truthiness, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_predicate(&self, predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>, cursor: &mut usize) -> Result<Option<Predicate<'db>>, Self::Error>;
        #[operation(source)]
        async fn large_call_prefix(&self, scope: ScopeId<'db>, root_predicate: ScopedPredicateId) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn call_prefix(&self, predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>, root_predicate: ScopedPredicateId) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn path(&self, scope: ScopeId<'db>, constraints: &ReachabilityConstraints, predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>, call_predicates: Option<&[ScopedPredicateId]>, id: ScopedReachabilityConstraintId, use_checkpoint: bool) -> Result<Truthiness, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_path_node(&self, constraints: &ReachabilityConstraints, cursor: &mut PathCursor) -> Result<Option<InteriorNode>, Self::Error>;
        #[operation(local)]
        async fn path_cursor(&self, id: ScopedReachabilityConstraintId, use_checkpoint: bool) -> Result<PathCursor, Self::Error>;
        #[operation(local)]
        async fn advance_path(&self, cursor: &mut PathCursor, id: ScopedReachabilityConstraintId) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn environment(&self, scope: ScopeId<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(local)]
        async fn is_checkpoint(&self, call_predicates: Option<&[ScopedPredicateId]>, predicate: ScopedPredicateId, visited: usize) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn checkpoint(&self, scope: ScopeId<'db>, id: ScopedReachabilityConstraintId) -> Result<Truthiness, Self::Error>;
        #[operation(source)]
        async fn analyze(&self, env: &ProgramEnvironment<'db>, predicate: &Predicate<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(child)]
        async fn non_terminal_call(&self, call: CallableAndCallExpr<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(source)]
        async fn truthiness(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(child)]
        async fn condition(&self, expression: Expression<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(child)]
        async fn comparison_condition(&self, env: &ProgramEnvironment<'db>, expression: Expression<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(child)]
        async fn context_manager_suppresses(&self, expression: Expression<'db>, is_async: bool) -> Result<Truthiness, Self::Error>;
        #[operation(child)]
        async fn finally_normal_path_impossible(&self, scope: ScopeId<'db>, continuation: ScopedReachabilityConstraintId) -> Result<Truthiness, Self::Error>;
        #[operation(child)]
        async fn pattern(&self, pattern: PatternPredicate<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(child)]
        async fn non_empty_iterable(&self, expression: Expression<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(source)]
        async fn star_import(&self, env: &ProgramEnvironment<'db>, star_import: StarImportPlaceholderPredicate<'db>) -> Result<Truthiness, Self::Error>;
        #[operation(source)]
        async fn callable(&self, call: CallableAndCallExpr<'db>) -> Result<Expression<'db>, Self::Error>;
        #[operation(source)]
        async fn expression_scope(&self, expression: Expression<'db>) -> Result<ScopeId<'db>, Self::Error>;
        #[operation(child)]
        async fn expression_type(&self, expression: Expression<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn is_await(&self, call: CallableAndCallExpr<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn classify_call(&self, env: &ProgramEnvironment<'db>, ty: Type<'db>, is_await: bool, call: CallableAndCallExpr<'db>) -> Result<Truthiness, Self::Error>;
    }

    #[finite_capability]
    impl ReachabilityFacts {
        fn terminal(&self, id: ScopedReachabilityConstraintId) -> Option<Truthiness> { super::terminal_reachability(id) }
        fn atom(&self, node: InteriorNode) -> ScopedPredicateId { node.atom() }
        fn is_call(&self, predicate: Predicate<'_>) -> bool { matches!(predicate.node, PredicateNode::IsNonTerminalCall(_)) }
        fn enough_calls(&self, count: usize) -> bool { count > super::NON_TERMINAL_CALL_CHUNK_SIZE }
        fn next_count(&self, count: usize) -> usize { count + 1 }
        fn use_checkpoint(&self, cursor: &PathCursor) -> bool { cursor.use_checkpoint }
        fn visited(&self, cursor: &PathCursor) -> usize { cursor.visited }
        fn current_id(&self, cursor: &PathCursor) -> ScopedReachabilityConstraintId { cursor.id }
        fn path_result(&self, cursor: PathCursor) -> Truthiness { cursor.result }
        fn edge(&self, node: InteriorNode, truthiness: Truthiness) -> ScopedReachabilityConstraintId {
            match truthiness {
                Truthiness::AlwaysTrue => node.if_true(),
                Truthiness::Ambiguous => node.if_ambiguous(),
                Truthiness::AlwaysFalse => node.if_false(),
            }
        }
        fn signed(&self, truthiness: Truthiness, positive: bool) -> Truthiness { truthiness.negate_if(!positive) }
    }

    #[synchronous(analyze_non_terminal_call_prefix_sync)]
    #[capabilities(effects = ReachabilityEffects, facts = ReachabilityFacts)]
    #[passive_values()]
    pub(crate) async fn analyze_non_terminal_call_prefix_with<'db, E: ReachabilityEffects<'db>>(
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        root_predicate: ScopedPredicateId,
        facts: ReachabilityFacts,
        effects: &E,
    ) -> Result<bool, E::Error> {
        let root = effects.predicate(predicates, root_predicate).await?;
        let scope = effects.predicate_scope(&root).await?;
        let mut cursor = 0;
        #[passive_state]
        let mut calls = 0;
        #[cursor_loop]
        while let Some(predicate) = effects.next_predicate(predicates, &mut cursor).await? {
            if facts.is_call(predicate) {
                calls = facts.next_count(calls);
                if facts.enough_calls(calls) {
                    return effects.large_call_prefix(scope, root_predicate).await;
                }
            }
        }
        Ok(false)
    }

    #[synchronous(evaluate_cached_reachability_sync)]
    #[capabilities(effects = ReachabilityEffects, facts = ReachabilityFacts)]
    #[passive_values()]
    pub(crate) async fn evaluate_cached_reachability_with<'db, E: ReachabilityEffects<'db>>(
        cache: &ReachabilityEvaluationCache<'db>,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        id: ScopedReachabilityConstraintId,
        facts: ReachabilityFacts,
        effects: &E,
    ) -> Result<Truthiness, E::Error> {
        if let Some(value) = facts.terminal(id) { return Ok(value); }
        let root = effects.node(constraints, id).await?;
        let predicate = effects.predicate(predicates, facts.atom(root)).await?;
        let scope = effects.predicate_scope(&predicate).await?;
        let key = effects.cache_key(cache, scope, constraints, id).await?;
        if let Some(result) = effects.cache_lookup(cache, key).await? { return Ok(result); }
        let result = effects.evaluate_constraint(scope, id).await?;
        effects.cache_insert(cache, key, result).await?;
        Ok(result)
    }

    #[synchronous(evaluate_reachability_sync)]
    #[capabilities(effects = ReachabilityEffects, facts = ReachabilityFacts)]
    #[passive_values()]
    pub(crate) async fn evaluate_reachability_with<'db, E: ReachabilityEffects<'db>>(
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        id: ScopedReachabilityConstraintId,
        facts: ReachabilityFacts,
        effects: &E,
    ) -> Result<Truthiness, E::Error> {
        if let Some(value) = facts.terminal(id) { return Ok(value); }
        let root = effects.node(constraints, id).await?;
        let root_predicate = facts.atom(root);
        effects.call_prefix(predicates, root_predicate).await?;
        let predicate = effects.predicate(predicates, root_predicate).await?;
        let scope = effects.predicate_scope(&predicate).await?;
        effects.path(scope, constraints, predicates, None, id, true).await
    }

    #[synchronous(evaluate_reachability_path_sync)]
    #[capabilities(effects = ReachabilityEffects, facts = ReachabilityFacts)]
    #[passive_values()]
    pub(crate) async fn evaluate_reachability_path_with<'db, E: ReachabilityEffects<'db>>(
        scope: ScopeId<'db>,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        call_predicates: Option<&[ScopedPredicateId]>,
        id: ScopedReachabilityConstraintId,
        use_checkpoint: bool,
        facts: ReachabilityFacts,
        effects: &E,
    ) -> Result<Truthiness, E::Error> {
        let env = effects.environment(scope).await?;
        let mut cursor = effects.path_cursor(id, use_checkpoint).await?;
        #[cursor_loop]
        while let Some(node) = effects.next_path_node(constraints, &mut cursor).await? {
            let atom = facts.atom(node);
            if facts.use_checkpoint(&cursor) && effects.is_checkpoint(call_predicates, atom, facts.visited(&cursor)).await? {
                return effects.checkpoint(scope, facts.current_id(&cursor)).await;
            }
            let predicate = effects.predicate(predicates, atom).await?;
            let truthiness = effects.analyze(&env, &predicate).await?;
            effects.advance_path(&mut cursor, facts.edge(node, truthiness)).await?;
        }
        Ok(facts.path_result(cursor))
    }

    #[synchronous(analyze_single_sync)]
    #[capabilities(effects = ReachabilityEffects, facts = ReachabilityFacts)]
    #[passive_values(Truthiness::Ambiguous)]
    pub(crate) async fn analyze_single_with<'db, E: ReachabilityEffects<'db>>(
        env: &ProgramEnvironment<'db>,
        predicate: &Predicate<'db>,
        facts: ReachabilityFacts,
        effects: &E,
    ) -> Result<Truthiness, E::Error> {
        match predicate.node {
            PredicateNode::Expression(expression) => {
                let ty = effects.expression_type(expression).await?;
                let truthiness = effects.truthiness(env, ty).await?;
                Ok(facts.signed(truthiness, predicate.is_positive))
            }
            PredicateNode::Condition(expression) => {
                let truthiness = effects.condition(expression).await?;
                Ok(facts.signed(truthiness, predicate.is_positive))
            }
            PredicateNode::ChainedComparisonCondition(expression) => {
                let truthiness = effects.comparison_condition(env, expression).await?;
                Ok(facts.signed(truthiness, predicate.is_positive))
            }
            PredicateNode::ContextManagerSuppresses { expression, is_async } => {
                let truthiness = effects.context_manager_suppresses(expression, is_async).await?;
                Ok(facts.signed(truthiness, predicate.is_positive))
            }
            PredicateNode::FinallyNormalPathImpossible { scope, continuation } => {
                let truthiness = effects.finally_normal_path_impossible(scope, continuation).await?;
                Ok(facts.signed(truthiness, predicate.is_positive))
            }
            PredicateNode::IsNonTerminalCall(call) => {
                let truthiness = effects.non_terminal_call(call).await?;
                Ok(facts.signed(truthiness, predicate.is_positive))
            }
            PredicateNode::Pattern(pattern) => effects.pattern(pattern).await,
            PredicateNode::SubjectElementPattern(subject_element) => effects.pattern(subject_element.pattern).await,
            PredicateNode::OrPatternAlternative(_) => Ok(Truthiness::Ambiguous),
            PredicateNode::IsNonEmptyIterable(expression) => {
                let truthiness = effects.non_empty_iterable(expression).await?;
                Ok(facts.signed(truthiness, predicate.is_positive))
            }
            PredicateNode::StarImportPlaceholder(star_import) => effects.star_import(env, star_import).await,
        }
    }

    #[synchronous(analyze_non_terminal_call_sync)]
    #[capabilities(effects = ReachabilityEffects)]
    #[passive_values()]
    pub(crate) async fn analyze_non_terminal_call_with<'db, E: ReachabilityEffects<'db>>(
        call: CallableAndCallExpr<'db>,
        effects: &E,
    ) -> Result<Truthiness, E::Error> {
        let callable = effects.callable(call).await?;
        let scope = effects.expression_scope(callable).await?;
        let env = effects.environment(scope).await?;
        // Inspecting the callee first avoids full argument inference and overload selection when
        // every signature already establishes whether the call can return.
        let ty = effects.expression_type(callable).await?;
        let is_await = effects.is_await(call).await?;
        effects.classify_call(&env, ty, is_await, call).await
    }
}

pub(crate) struct SignatureCursor<'call, 'db> {
    callables: std::slice::Iter<'call, CallableType<'db>>,
    overloads: std::slice::Iter<'db, Signature<'db>>,
}

impl<'call, 'db> SignatureCursor<'call, 'db> {
    pub(crate) fn new(callables: &'call CallableTypes<'db>) -> Self {
        Self {
            callables: callables.into_iter(),
            overloads: [].iter(),
        }
    }

    #[cfg(feature = "experimental-analysis")]
    pub(crate) fn remaining_callables(&self) -> usize {
        self.callables.len()
    }

    pub(crate) fn next(&mut self, db: &'db dyn Db) -> Option<&'db Signature<'db>> {
        loop {
            match self.next_retained() {
                ControlFlow::Break(signature) => return signature,
                ControlFlow::Continue(callable) => self.enter_signatures(callable.signatures(db)),
            }
        }
    }

    pub(crate) fn next_retained(
        &mut self,
    ) -> ControlFlow<Option<&'db Signature<'db>>, CallableType<'db>> {
        if let Some(signature) = self.overloads.next() {
            return ControlFlow::Break(Some(signature));
        }
        self.callables
            .next()
            .copied()
            .map_or(ControlFlow::Break(None), ControlFlow::Continue)
    }

    pub(crate) fn enter_signatures(&mut self, signatures: &'db CallableSignature<'db>) {
        self.overloads = signatures.overloads.iter();
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousTerminalCallEffects)]
    pub(crate) trait TerminalCallEffects<'db, C> {
        type Error;
        #[operation(source)]
        async fn callables(&self, ty: Type<'db>) -> Result<Option<CallableTypes<'db>>, Self::Error>;
        #[operation(local)]
        async fn signatures<'call>(&self, callables: &'call CallableTypes<'db>) -> Result<SignatureCursor<'call, 'db>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_signature(&self, signatures: &mut SignatureCursor<'_, 'db>) -> Result<Option<&'db Signature<'db>>, Self::Error>;
        #[operation(source)]
        async fn equivalent_to_never(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn has_typevar(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn call_type(&self, call: C) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn retire_callables(&self, callables: CallableTypes<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(is_non_terminal_call_sync)]
    #[capabilities(effects = TerminalCallEffects)]
    #[passive_values(Truthiness::AlwaysTrue, Truthiness::AlwaysFalse)]
    pub(crate) async fn is_non_terminal_call_with<'db, C, E: TerminalCallEffects<'db, C>>(
        ty: Type<'db>,
        is_await: bool,
        call: C,
        effects: &E,
    ) -> Result<Truthiness, E::Error> {
        // Dynamic calls cannot establish termination. Skipping callable conversion also avoids
        // contention on the interner for these common types: https://github.com/astral-sh/ty/issues/968.
        if let Type::Dynamic(_) = ty { return Ok(Truthiness::AlwaysTrue); }
        let Some(callables) = effects.callables(ty).await? else { return Ok(Truthiness::AlwaysTrue); };
        #[passive_state]
        let mut no_overloads_return_never = true;
        #[passive_state]
        let mut all_overloads_return_never = true;
        #[passive_state]
        let mut any_overload_is_generic = false;
        let mut signatures = effects.signatures(&callables).await?;
        #[cursor_loop]
        while let Some(overload) = effects.next_signature(&mut signatures).await? {
            let returns_never = effects.equivalent_to_never(overload.return_ty).await?;
            no_overloads_return_never = no_overloads_return_never && !returns_never;
            all_overloads_return_never = all_overloads_return_never && returns_never;
            let is_generic = effects.has_typevar(overload.return_ty).await?;
            any_overload_is_generic = any_overload_is_generic || is_generic;
        }
        let result = if no_overloads_return_never && !any_overload_is_generic && !is_await {
            Truthiness::AlwaysTrue
        } else if all_overloads_return_never {
            Truthiness::AlwaysFalse
        } else {
            let result_type = effects.call_type(call).await?;
            if effects.equivalent_to_never(result_type).await? { Truthiness::AlwaysFalse } else { Truthiness::AlwaysTrue }
        };
        effects.retire_callables(callables).await?;
        Ok(result)
    }
}

pub(crate) struct OrdinaryReachabilityEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> OrdinaryReachabilityEffects<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl<'db> SynchronousReachabilityEffects<'db> for OrdinaryReachabilityEffects<'db> {
    type Error = Infallible;
    fn node(
        &self,
        constraints: &ReachabilityConstraints,
        id: ScopedReachabilityConstraintId,
    ) -> Result<InteriorNode, Self::Error> {
        Ok(constraints.get_interior_node(id))
    }
    fn predicate(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        id: ScopedPredicateId,
    ) -> Result<Predicate<'db>, Self::Error> {
        Ok(predicates[id])
    }
    fn predicate_scope(&self, predicate: &Predicate<'db>) -> Result<ScopeId<'db>, Self::Error> {
        Ok(super::predicate_scope(self.db, predicate))
    }
    fn cache_key(
        &self,
        cache: &ReachabilityEvaluationCache<'db>,
        scope: ScopeId<'db>,
        constraints: &ReachabilityConstraints,
        id: ScopedReachabilityConstraintId,
    ) -> Result<ReachabilityCacheKey, Self::Error> {
        Ok(cache.key(scope, constraints, id))
    }
    fn cache_lookup(
        &self,
        cache: &ReachabilityEvaluationCache<'db>,
        key: ReachabilityCacheKey,
    ) -> Result<Option<Truthiness>, Self::Error> {
        Ok(cache.lookup(key))
    }
    fn cache_insert(
        &self,
        cache: &ReachabilityEvaluationCache<'db>,
        key: ReachabilityCacheKey,
        result: Truthiness,
    ) -> Result<(), Self::Error> {
        cache.insert(key, result);
        Ok(())
    }
    fn evaluate_constraint(
        &self,
        scope: ScopeId<'db>,
        id: ScopedReachabilityConstraintId,
    ) -> Result<Truthiness, Self::Error> {
        Ok(super::evaluate_reachability_constraint(self.db, scope, id))
    }
    fn next_predicate(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        cursor: &mut usize,
    ) -> Result<Option<Predicate<'db>>, Self::Error> {
        let next = predicates.iter().nth(*cursor).copied();
        *cursor += usize::from(next.is_some());
        Ok(next)
    }
    fn large_call_prefix(
        &self,
        scope: ScopeId<'db>,
        root_predicate: ScopedPredicateId,
    ) -> Result<bool, Self::Error> {
        Ok(super::analyze_large_non_terminal_call_prefix(
            self.db,
            scope,
            root_predicate,
        ))
    }
    fn call_prefix(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        root_predicate: ScopedPredicateId,
    ) -> Result<bool, Self::Error> {
        analyze_non_terminal_call_prefix_sync(predicates, root_predicate, ReachabilityFacts, self)
    }
    fn path(
        &self,
        scope: ScopeId<'db>,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        call_predicates: Option<&[ScopedPredicateId]>,
        id: ScopedReachabilityConstraintId,
        use_checkpoint: bool,
    ) -> Result<Truthiness, Self::Error> {
        evaluate_reachability_path_sync(
            scope,
            constraints,
            predicates,
            call_predicates,
            id,
            use_checkpoint,
            ReachabilityFacts,
            self,
        )
    }
    fn next_path_node(
        &self,
        constraints: &ReachabilityConstraints,
        cursor: &mut PathCursor,
    ) -> Result<Option<InteriorNode>, Self::Error> {
        Ok(cursor.next(constraints))
    }
    fn path_cursor(
        &self,
        id: ScopedReachabilityConstraintId,
        use_checkpoint: bool,
    ) -> Result<PathCursor, Self::Error> {
        Ok(PathCursor::new(id, use_checkpoint))
    }
    fn advance_path(
        &self,
        cursor: &mut PathCursor,
        id: ScopedReachabilityConstraintId,
    ) -> Result<(), Self::Error> {
        cursor.advance(id);
        Ok(())
    }
    fn environment(&self, scope: ScopeId<'db>) -> Result<ProgramEnvironment<'db>, Self::Error> {
        Ok(ProgramEnvironment::from_scope(scope))
    }
    fn is_checkpoint(
        &self,
        call_predicates: Option<&[ScopedPredicateId]>,
        predicate: ScopedPredicateId,
        visited: usize,
    ) -> Result<bool, Self::Error> {
        Ok(super::is_reachability_checkpoint(
            call_predicates,
            predicate,
            visited,
        ))
    }
    fn checkpoint(
        &self,
        scope: ScopeId<'db>,
        id: ScopedReachabilityConstraintId,
    ) -> Result<Truthiness, Self::Error> {
        Ok(super::evaluate_reachability_checkpoint(self.db, scope, id))
    }
    fn analyze(
        &self,
        env: &ProgramEnvironment<'db>,
        predicate: &Predicate<'db>,
    ) -> Result<Truthiness, Self::Error> {
        analyze_single_sync(env, predicate, ReachabilityFacts, self)
    }
    fn non_terminal_call(&self, call: CallableAndCallExpr<'db>) -> Result<Truthiness, Self::Error> {
        Ok(super::analyze_non_terminal_call(self.db, call))
    }
    fn truthiness(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Truthiness, Self::Error> {
        Ok(ty.bool(self.db, env))
    }
    fn condition(&self, expression: Expression<'db>) -> Result<Truthiness, Self::Error> {
        Ok(super::analyze_condition(self.db, expression))
    }
    fn comparison_condition(
        &self,
        env: &ProgramEnvironment<'db>,
        expression: Expression<'db>,
    ) -> Result<Truthiness, Self::Error> {
        let inference =
            crate::types::infer_expression_types(self.db, expression, TypeContext::default());
        let expression = expression.node_ref(self.db);
        Ok(inference
            .comparison_truthiness(expression)
            .unwrap_or_else(|| inference.expression_type(expression).bool(self.db, env)))
    }
    fn context_manager_suppresses(
        &self,
        expression: Expression<'db>,
        is_async: bool,
    ) -> Result<Truthiness, Self::Error> {
        Ok(Truthiness::from(if is_async {
            super::async_context_manager_suppresses(self.db, expression)
        } else {
            super::sync_context_manager_suppresses(self.db, expression)
        }))
    }
    fn finally_normal_path_impossible(
        &self,
        scope: ScopeId<'db>,
        continuation: ScopedReachabilityConstraintId,
    ) -> Result<Truthiness, Self::Error> {
        Ok(Truthiness::from(
            super::evaluate_finally_continuation(self.db, scope, continuation).is_always_false(),
        ))
    }
    fn pattern(&self, pattern: PatternPredicate<'db>) -> Result<Truthiness, Self::Error> {
        Ok(super::analyze_pattern_predicate(self.db, pattern))
    }
    fn non_empty_iterable(&self, expression: Expression<'db>) -> Result<Truthiness, Self::Error> {
        Ok(super::analyze_non_empty_iterable(self.db, expression))
    }
    fn star_import(
        &self,
        env: &ProgramEnvironment<'db>,
        star_import: StarImportPlaceholderPredicate<'db>,
    ) -> Result<Truthiness, Self::Error> {
        Ok(super::analyze_star_import_predicate(
            self.db,
            env,
            star_import,
        ))
    }
    fn callable(&self, call: CallableAndCallExpr<'db>) -> Result<Expression<'db>, Self::Error> {
        Ok(call.callable(self.db))
    }
    fn expression_scope(&self, expression: Expression<'db>) -> Result<ScopeId<'db>, Self::Error> {
        Ok(expression.scope(self.db))
    }
    fn expression_type(&self, expression: Expression<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(infer_same_file_expression_type(
            self.db,
            expression,
            TypeContext::default(),
        ))
    }
    fn is_await(&self, call: CallableAndCallExpr<'db>) -> Result<bool, Self::Error> {
        Ok(call.is_await(self.db))
    }
    fn classify_call(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        is_await: bool,
        call: CallableAndCallExpr<'db>,
    ) -> Result<Truthiness, Self::Error> {
        is_non_terminal_call_sync(
            ty,
            is_await,
            || {
                infer_same_file_expression_type(
                    self.db,
                    call.call_expr(self.db),
                    TypeContext::default(),
                )
            },
            &OrdinaryTerminalCallEffects::new(self.db, env),
        )
    }
}

pub(crate) struct OrdinaryTerminalCallEffects<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'env, 'db> OrdinaryTerminalCallEffects<'env, 'db> {
    pub(crate) fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>) -> Self {
        Self { db, env }
    }
}

impl<'db, C: FnOnce() -> Type<'db>> SynchronousTerminalCallEffects<'db, C>
    for OrdinaryTerminalCallEffects<'_, 'db>
{
    type Error = Infallible;
    fn callables(&self, ty: Type<'db>) -> Result<Option<CallableTypes<'db>>, Self::Error> {
        Ok(ty.try_upcast_to_callable(self.db, self.env))
    }
    fn signatures<'call>(
        &self,
        callables: &'call CallableTypes<'db>,
    ) -> Result<SignatureCursor<'call, 'db>, Self::Error> {
        Ok(SignatureCursor::new(callables))
    }
    fn next_signature(
        &self,
        signatures: &mut SignatureCursor<'_, 'db>,
    ) -> Result<Option<&'db Signature<'db>>, Self::Error> {
        Ok(signatures.next(self.db))
    }
    fn equivalent_to_never(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty.is_equivalent_to(self.db, self.env, Type::Never))
    }
    fn has_typevar(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty.has_typevar(self.db, self.env))
    }
    fn call_type(&self, call: C) -> Result<Type<'db>, Self::Error> {
        Ok(call())
    }
    fn retire_callables(&self, callables: CallableTypes<'db>) -> Result<(), Self::Error> {
        drop(callables);
        Ok(())
    }
}
