//! Controlled reachability borrows the prepared graph and canonical predicate results.

use std::ops::ControlFlow;

use ruff_index::IndexSlice;
use ruff_python_ast::name::Name;
use ruff_text_size::{TextRange, TextSize};
use rustc_hash::FxHashSet;
use salsa::execution_probe::{ExecutionWork, RunError, RunResult};
use ty_python_core::expression::Expression;
use ty_python_core::predicate::{
    CallableAndCallExpr, PatternPredicate, Predicate, PredicateNode, ScopedPredicateId,
    StarImportPlaceholderPredicate, SubjectElementPatternPredicate,
};
use ty_python_core::reachability_constraints::{
    InteriorNode, ReachabilityConstraints, ScopedReachabilityConstraintId,
};
use ty_python_core::scope::{FileScopeId, Scope, ScopeId};
use ty_python_core::UseDefMap;
use ty_python_core::{ProgramFile, SemanticIndex, Truthiness};

use super::storage::{sequence_merge, slots, table_merge};
use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::place::{PlaceAndQualifiers, RequiresExplicitReExport};
use crate::reachability::range::{self, RangeCursor, RangeReachabilityEffects};
use crate::reachability::source::{
    self, PathCursor, ReachabilityEffects, ReachabilityFacts, SignatureCursor, TerminalCallEffects,
};
use crate::reachability::star_import::{StarImportEffects, analyze_star_import_with};
use crate::reachability::{ReachabilityCacheKey, ReachabilityEvaluationCache};
use crate::types::callable::CallableTypes;
use crate::types::infer::TypeInferenceBuilder;
use crate::types::local_transfer::generated_field_quote;
use crate::types::signatures::Signature;
use crate::types::{Type, TypeContext};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Quotes argument transfers into one scope's shared range scan.
    const fn range_scope_quote<I>(_: &RangeCursor<I>) -> (usize, usize) {
        (
            14,
            size_of::<[(&UseDefMap<'db>, TextRange, &mut RangeCursor<I>, &Self); 2]>(),
        )
    }

    /// Checks a diagnostic range against the prepared builder's scope and ancestor constraints.
    pub(in crate::types::infer::builder) async fn is_range_reachable_source(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        range: TextRange,
    ) -> RunResult<bool> {
        let file = self
            .local_with_fixed_transfers(2, 0, || builder.program_file())
            .await?;
        self.boxed_future_with_fixed_transfers(
            Ok((8, size_of::<[(&Self, ProgramFile<'db>); 2]>())),
            || self.check_file_program(file),
        )
        .await?
        .await?;
        let scope = self
            .local_with_fixed_transfers(2, 0, || builder.context.scope())
            .await?;
        let endpoint = self.access.endpoint();
        let quote = generated_field_quote(
            |scope: ScopeId<'db>, fields| scope.read_fields(fields),
            |scope: ScopeId<'db>, fields| scope.read_fields(fields).file_scope_id(),
        );
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                let request = scope
                    .read_fields(endpoint.field_request_context())
                    .file_scope_id();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        let file_scope = read.await;
        let index = self
            .local_with_fixed_transfers(1, 0, || builder.index)
            .await?;
        self.boxed_future_with_fixed_transfers(
            Ok((
                14,
                size_of::<[(&SemanticIndex<'db>, FileScopeId, TextRange, &Self); 2]>(),
            )),
            || range::is_range_reachable_with(index, file_scope, range, self),
        )
        .await?
        .await
    }

    pub(in crate::types::infer) async fn infer_non_terminal_call(
        &self,
        call: CallableAndCallExpr<'db>,
    ) -> RunResult<Truthiness> {
        let expression = self.field(call.read_fields(self.db()).callable()).await?;
        let file = self.expression_file(expression).await?;
        self.check_file_program(file).await?;
        source::analyze_non_terminal_call_with(call, self).await
    }

    pub(in crate::types::infer::builder) async fn evaluate_reachability(
        &self,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        constraint: ScopedReachabilityConstraintId,
    ) -> RunResult<Truthiness> {
        #[cfg(test)]
        super::observations::observe(
            self.db(),
            super::observations::Event::ReachabilityAllocationBefore,
        );
        self.allocate_future(|| {
            #[cfg(test)]
            super::observations::observe(
                self.db(),
                super::observations::Event::ReachabilityAllocationAdmitted,
            );
            source::evaluate_reachability_with(
                constraints,
                predicates,
                constraint,
                ReachabilityFacts,
                self,
            )
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RangeReachabilityEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn ancestor_cursor(&self, scope: FileScopeId) -> RunResult<Option<FileScopeId>> {
        self.local_with_fixed_transfers(3, 0, || Some(scope)).await
    }

    async fn next_ancestor<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        cursor: &mut Option<FileScopeId>,
    ) -> RunResult<Option<&'index UseDefMap<'db>>> {
        let bytes = size_of::<FileScopeId>()
            + size_of::<[Option<FileScopeId>; 2]>()
            + size_of::<&Scope>();
        self.local_with_fixed_transfers(14, bytes, || range::next_ancestor(index, cursor))
            .await
    }

    async fn scope_reachable(&self, use_def: &UseDefMap<'db>, range: TextRange) -> RunResult<bool> {
        let mut cursor = self
            .local_with_fixed_transfers(4, 0, || range::scope_ranges(use_def))
            .await?;
        // The concrete iterator remains in this frame while the child evaluates predicates.
        self.boxed_future_with_fixed_transfers(
            Ok(Self::range_scope_quote(&cursor)),
            || range::scope_ranges_reachable_with(use_def, range, &mut cursor, self),
        )
        .await?
        .await
    }

    async fn next_range<I>(
        &self,
        cursor: &mut RangeCursor<I>,
    ) -> RunResult<Option<(TextRange, ScopedReachabilityConstraintId)>>
    where
        I: Iterator<Item = (TextRange, ScopedReachabilityConstraintId)>,
    {
        let bytes = size_of::<I>() + size_of::<(TextRange, ScopedReachabilityConstraintId)>();
        self.local_with_fixed_transfers(12, bytes, || cursor.next())
            .await
    }

    async fn contains_range(&self, entry_range: TextRange, range: TextRange) -> RunResult<bool> {
        let bytes = size_of::<[TextSize; 4]>() + size_of::<[bool; 3]>();
        self.local_with_fixed_transfers(9, bytes, || entry_range.contains_range(range))
            .await
    }

    async fn constraint_reachable(
        &self,
        use_def: &UseDefMap<'db>,
        constraint: ScopedReachabilityConstraintId,
    ) -> RunResult<bool> {
        let (constraints, predicates) = self
            .local_with_fixed_transfers(9, 0, || {
                (use_def.reachability_constraints(), use_def.predicates())
            })
            .await?;
        let truthiness = self
            .boxed_future_with_fixed_transfers(
                Ok((
                    14,
                    size_of::<[
                        (
                            &Self,
                            &ReachabilityConstraints,
                            &IndexSlice<ScopedPredicateId, Predicate<'db>>,
                            ScopedReachabilityConstraintId,
                        );
                        2
                    ]>(),
                )),
                || self.evaluate_reachability(constraints, predicates, constraint),
            )
            .await?
            .await?;
        self.local_with_fixed_transfers(6, size_of::<[bool; 4]>(), || truthiness.may_be_true())
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ReachabilityEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn node(
        &self,
        constraints: &ReachabilityConstraints,
        id: ScopedReachabilityConstraintId,
    ) -> RunResult<InteriorNode> {
        self.local(2, 0, || constraints.get_interior_node(id)).await
    }

    async fn predicate(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        id: ScopedPredicateId,
    ) -> RunResult<Predicate<'db>> {
        self.local(1, 0, || predicates[id]).await
    }

    async fn predicate_scope(&self, predicate: &Predicate<'db>) -> RunResult<ScopeId<'db>> {
        self.work(2).await?;
        let fields = self.access.endpoint().field_request_context();
        let scope = match predicate.node {
            PredicateNode::Expression(expression)
            | PredicateNode::Condition(expression)
            | PredicateNode::ChainedComparisonCondition(expression)
            | PredicateNode::ContextManagerSuppresses { expression, .. }
            | PredicateNode::IsNonEmptyIterable(expression) => {
                SourceEffects::expression_scope(self, expression).await?
            }
            PredicateNode::IsNonTerminalCall(call) => {
                let expression = self.field(call.read_fields(fields).callable()).await?;
                SourceEffects::expression_scope(self, expression).await?
            }
            PredicateNode::Pattern(pattern)
            | PredicateNode::SubjectElementPattern(SubjectElementPatternPredicate {
                pattern,
                ..
            }) => {
                let file_scope = self.field(pattern.read_fields(fields).file_scope()).await?;
                let file = self
                    .field(pattern.read_fields(fields).program_file())
                    .await?;
                let index = self.access.semantic_index(file).await?;
                self.local(1, 0, || index.scope_id(file_scope)).await?
            }
            PredicateNode::FinallyNormalPathImpossible { scope, .. }
            | PredicateNode::OrPatternAlternative(scope) => scope,
            PredicateNode::StarImportPlaceholder(star_import) => {
                let file = self
                    .field(star_import.read_fields(fields).importing_file())
                    .await?;
                self.access.global_scope(file).await?
            }
        };
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        Ok(scope)
    }

    async fn cache_key(
        &self,
        cache: &ReachabilityEvaluationCache<'db>,
        scope: ScopeId<'db>,
        constraints: &ReachabilityConstraints,
        id: ScopedReachabilityConstraintId,
    ) -> RunResult<ReachabilityCacheKey> {
        self.local(2, 0, || cache.key(scope, constraints, id)).await
    }

    async fn cache_lookup(
        &self,
        cache: &ReachabilityEvaluationCache<'db>,
        key: ReachabilityCacheKey,
    ) -> RunResult<Option<Truthiness>> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                let work = match key {
                    ReachabilityCacheKey::Primary(_) => 1,
                    ReachabilityCacheKey::Other { .. } => {
                        Self::checked(slots(cache.storage(key).1))?
                    }
                };
                endpoint.admit_work(work)?;
                endpoint.check_completion()?;
                Ok(cache.lookup(key))
            })
            .await)
    }

    async fn cache_insert(
        &self,
        cache: &ReachabilityEvaluationCache<'db>,
        key: ReachabilityCacheKey,
        result: Truthiness,
    ) -> RunResult<()> {
        #[cfg(test)]
        super::observations::reachability_cache_ready(self.db(), key, cache.retained_storage());
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                // Predicate inference can re-enter this cache. Quote its current backing after
                // that dependency completes, and keep every borrow inside this local operation.
                let (length, capacity) = cache.storage(key);
                let (mut quote, retirement) = match key {
                    ReachabilityCacheKey::Primary(index) => {
                        let required = Self::checked(index.checked_add(1))?;
                        let incoming = required.saturating_sub(length);
                        let mut quote =
                            sequence_merge::<Option<Truthiness>>(length, capacity, incoming)
                                .ok_or(RunError::Contract(
                                    "reachability cache quotation overflow",
                                ))?;
                        if quote.bytes != 0 {
                            // Byte-sized vector elements can have an initial capacity of eight.
                            quote.bytes = quote.bytes.max(8 * size_of::<Option<Truthiness>>());
                        }
                        let backing = capacity.max(quote.bytes / size_of::<Option<Truthiness>>());
                        let retirement = Self::checked(
                            backing
                                .checked_add(length.max(required))
                                .and_then(|n| n.checked_add(4)),
                        )?;
                        (quote, retirement)
                    }
                    ReachabilityCacheKey::Other { .. } => {
                        let (quote, backing) = table_merge::<(
                            (usize, ScopedReachabilityConstraintId),
                            Truthiness,
                        )>(length, capacity, 1, 0)
                        .ok_or(RunError::Contract("reachability cache quotation overflow"))?;
                        let retirement = Self::checked(
                            backing.checked_add(length).and_then(|n| n.checked_add(5)),
                        )?;
                        (quote, retirement)
                    }
                };
                // The cache is also dropped when a later dependency refuses or cancels, before
                // the builder reaches finalization. Pay for that disposal before retaining data.
                quote.work = Self::checked(quote.work.checked_add(retirement))?;
                endpoint.admit_work(quote.work)?;
                if quote.bytes != 0 {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: quote.bytes,
                    })?;
                }
                endpoint.check_completion()?;
                cache.insert(key, result);
                #[cfg(test)]
                super::observations::reachability_cache_stored(
                    self.db(),
                    key,
                    cache.retained_storage(),
                );
                Ok(())
            })
            .await)
    }

    async fn evaluate_constraint(
        &self,
        scope: ScopeId<'db>,
        id: ScopedReachabilityConstraintId,
    ) -> RunResult<Truthiness> {
        let db = self.db();
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let source = self.access.prepare_existing(file).await?;
        if source.file != file {
            return Err(RunError::Contract("prepared reachability file is foreign"));
        }
        let file_scope = self.field(scope.read_fields(db).file_scope_id()).await?;
        let use_def = self
            .local(2, 0, || source.index.use_def_map(file_scope))
            .await?;
        self.evaluate_reachability(use_def.reachability_constraints(), use_def.predicates(), id)
            .await
    }

    async fn next_predicate(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        cursor: &mut usize,
    ) -> RunResult<Option<Predicate<'db>>> {
        self.local(1, 0, || {
            let next = predicates.iter().nth(*cursor).copied();
            *cursor += usize::from(next.is_some());
            next
        })
        .await
    }

    async fn large_call_prefix(
        &self,
        _scope: ScopeId<'db>,
        _root_predicate: ScopedPredicateId,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ReachabilityPrefix).await
    }

    async fn call_prefix(
        &self,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        root_predicate: ScopedPredicateId,
    ) -> RunResult<bool> {
        source::analyze_non_terminal_call_prefix_with(
            predicates,
            root_predicate,
            ReachabilityFacts,
            self,
        )
        .await
    }

    async fn path(
        &self,
        scope: ScopeId<'db>,
        constraints: &ReachabilityConstraints,
        predicates: &IndexSlice<ScopedPredicateId, Predicate<'db>>,
        call_predicates: Option<&[ScopedPredicateId]>,
        id: ScopedReachabilityConstraintId,
        use_checkpoint: bool,
    ) -> RunResult<Truthiness> {
        source::evaluate_reachability_path_with(
            scope,
            constraints,
            predicates,
            call_predicates,
            id,
            use_checkpoint,
            ReachabilityFacts,
            self,
        )
        .await
    }

    async fn next_path_node(
        &self,
        constraints: &ReachabilityConstraints,
        cursor: &mut PathCursor,
    ) -> RunResult<Option<InteriorNode>> {
        self.local(3, 0, || cursor.next(constraints)).await
    }

    async fn path_cursor(
        &self,
        id: ScopedReachabilityConstraintId,
        use_checkpoint: bool,
    ) -> RunResult<PathCursor> {
        self.local(1, 0, || PathCursor::new(id, use_checkpoint))
            .await
    }

    async fn advance_path(
        &self,
        cursor: &mut PathCursor,
        id: ScopedReachabilityConstraintId,
    ) -> RunResult<()> {
        self.local(1, 0, || cursor.advance(id)).await
    }

    async fn environment(&self, scope: ScopeId<'db>) -> RunResult<ProgramEnvironment<'db>> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        self.local(1, 0, || ProgramEnvironment::from_file(file))
            .await
    }

    async fn is_checkpoint(
        &self,
        call_predicates: Option<&[ScopedPredicateId]>,
        predicate: ScopedPredicateId,
        visited: usize,
    ) -> RunResult<bool> {
        let work = Self::checked(
            call_predicates
                .map_or(0, <[ScopedPredicateId]>::len)
                .checked_add(4),
        )?;
        self.local(work, 0, || {
            source::is_checkpoint(call_predicates, predicate, visited)
        })
        .await
    }

    async fn checkpoint(
        &self,
        _scope: ScopeId<'db>,
        _id: ScopedReachabilityConstraintId,
    ) -> RunResult<Truthiness> {
        self.unavailable(SourceOperation::ReachabilityCheckpoint)
            .await
    }

    async fn analyze(
        &self,
        env: &ProgramEnvironment<'db>,
        predicate: &Predicate<'db>,
    ) -> RunResult<Truthiness> {
        source::analyze_single_with(env, predicate, ReachabilityFacts, self).await
    }

    async fn non_terminal_call(&self, call: CallableAndCallExpr<'db>) -> RunResult<Truthiness> {
        self.access.non_terminal_call(call).await
    }

    async fn truthiness(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Truthiness> {
        self.type_truthiness(env, ty).await
    }

    async fn condition(&self, _expression: Expression<'db>) -> RunResult<Truthiness> {
        self.unavailable(SourceOperation::ReachabilityPredicate)
            .await
    }

    async fn comparison_condition(
        &self,
        env: &ProgramEnvironment<'db>,
        expression: Expression<'db>,
    ) -> RunResult<Truthiness> {
        let inference = self
            .canonical_expression(expression, TypeContext::default())
            .await?;
        let work = Self::checked(inference.expressions.iter().len().checked_add(3))?;
        let node_ref = self
            .field(expression.read_fields(self.db()).node_ref())
            .await?;
        if let Some(truthiness) = self
            .local(work, 0, || inference.comparison_truthiness(node_ref))
            .await?
        {
            return Ok(truthiness);
        }
        let node_ref = self
            .field(expression.read_fields(self.db()).node_ref())
            .await?;
        let ty = self
            .local(work, 0, || inference.expression_type(node_ref))
            .await?;
        self.type_truthiness(env, ty).await
    }

    async fn context_manager_suppresses(
        &self,
        _expression: Expression<'db>,
        _is_async: bool,
    ) -> RunResult<Truthiness> {
        self.unavailable(SourceOperation::ReachabilityPredicate)
            .await
    }

    async fn finally_normal_path_impossible(
        &self,
        _scope: ScopeId<'db>,
        _continuation: ScopedReachabilityConstraintId,
    ) -> RunResult<Truthiness> {
        self.unavailable(SourceOperation::ReachabilityPredicate)
            .await
    }

    async fn pattern(&self, _pattern: PatternPredicate<'db>) -> RunResult<Truthiness> {
        self.unavailable(SourceOperation::ReachabilityPredicate)
            .await
    }

    async fn non_empty_iterable(&self, _expression: Expression<'db>) -> RunResult<Truthiness> {
        self.unavailable(SourceOperation::ReachabilityPredicate)
            .await
    }

    async fn star_import(
        &self,
        env: &ProgramEnvironment<'db>,
        star_import: StarImportPlaceholderPredicate<'db>,
    ) -> RunResult<Truthiness> {
        self.allocate_future(|| analyze_star_import_with(env, star_import, self))
            .await?
            .await
    }

    async fn callable(&self, call: CallableAndCallExpr<'db>) -> RunResult<Expression<'db>> {
        let expression = self.field(call.read_fields(self.db()).callable()).await?;
        let file = self.expression_file(expression).await?;
        self.check_file_program(file).await?;
        Ok(expression)
    }

    async fn expression_scope(&self, expression: Expression<'db>) -> RunResult<ScopeId<'db>> {
        SourceEffects::expression_scope(self, expression).await
    }

    async fn expression_type(&self, expression: Expression<'db>) -> RunResult<Type<'db>> {
        let inference = self
            .canonical_expression(expression, TypeContext::default())
            .await?;
        let work = Self::checked(inference.expressions.iter().len().checked_add(2))?;
        let node_ref = self
            .field(expression.read_fields(self.db()).node_ref())
            .await?;
        self.local(work, 0, || inference.expression_type(node_ref))
            .await
    }

    async fn is_await(&self, call: CallableAndCallExpr<'db>) -> RunResult<bool> {
        self.field(call.read_fields(self.db()).is_await()).await
    }

    async fn classify_call(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        is_await: bool,
        call: CallableAndCallExpr<'db>,
    ) -> RunResult<Truthiness> {
        source::is_non_terminal_call_with(
            ty,
            is_await,
            call,
            &TerminalSourceEffects { source: self, env },
        )
        .await
    }
}

struct TerminalSourceEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    env: &'effects ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TerminalCallEffects<'db, CallableAndCallExpr<'db>>
    for TerminalSourceEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn callables(&self, ty: Type<'db>) -> RunResult<Option<CallableTypes<'db>>> {
        self.source.reachability_callables(self.env, ty).await
    }

    async fn signatures<'call>(
        &self,
        callables: &'call CallableTypes<'db>,
    ) -> RunResult<SignatureCursor<'call, 'db>> {
        self.source
            .local(1, 0, || SignatureCursor::new(callables))
            .await
    }

    async fn next_signature(
        &self,
        signatures: &mut SignatureCursor<'_, 'db>,
    ) -> RunResult<Option<&'db Signature<'db>>> {
        let work = SourceEffects::<A>::checked(signatures.remaining_callables().checked_add(2))?;
        self.source.work(work).await?;
        loop {
            match self
                .source
                .local(1, 0, || signatures.next_retained())
                .await?
            {
                ControlFlow::Break(signature) => return Ok(signature),
                ControlFlow::Continue(callable) => {
                    let signature = self
                        .source
                        .field(callable.field_requests(self.source.db()).signatures())
                        .await?;
                    self.source
                        .local(1, 0, || signatures.enter_signatures(signature))
                        .await?;
                }
            }
        }
    }

    async fn equivalent_to_never(&self, ty: Type<'db>) -> RunResult<bool> {
        crate::types::relation::source::equivalence_condition(
            self.source.db(),
            self.env,
            ty,
            Type::Never,
            self.source,
        )
        .await
    }

    async fn has_typevar(&self, ty: Type<'db>) -> RunResult<bool> {
        self.source.has_typevar_source(self.env, ty).await
    }

    async fn call_type(&self, call: CallableAndCallExpr<'db>) -> RunResult<Type<'db>> {
        let expression = self
            .source
            .field(call.read_fields(self.source.db()).call_expr())
            .await?;
        ReachabilityEffects::expression_type(self.source, expression).await
    }

    async fn retire_callables(&self, callables: CallableTypes<'db>) -> RunResult<()> {
        let work = SourceEffects::<A>::checked((&callables).into_iter().len().checked_add(2))?;
        self.source.local(work, 0, || drop(callables)).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StarImportEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn symbol_name(
        &self,
        predicate: StarImportPlaceholderPredicate<'db>,
    ) -> RunResult<&'db Name> {
        let file = self
            .field(predicate.read_fields(self.db()).importing_file())
            .await?;
        self.check_file_program(file).await?;
        self.access.prepare_existing(file).await?;
        let scope = self.access.global_scope(file).await?;
        let table = self.access.place_table(scope).await?;
        let symbol = self
            .field(predicate.read_fields(self.db()).symbol_id())
            .await?;
        self.local(2, 0, || table.symbol(symbol).name()).await
    }

    async fn referenced_file(
        &self,
        predicate: StarImportPlaceholderPredicate<'db>,
    ) -> RunResult<ProgramFile<'db>> {
        let file = self
            .field(predicate.read_fields(self.db()).referenced_file())
            .await?;
        self.check_file_program(file).await?;
        Ok(file)
    }

    async fn export_names(
        &self,
        file: ProgramFile<'db>,
    ) -> RunResult<Option<&'db FxHashSet<Name>>> {
        let names = self.access.dunder_all_names(file).await?;
        self.local(1, 0, || names.as_ref()).await
    }

    async fn contains_name(&self, names: &FxHashSet<Name>, name: &Name) -> RunResult<bool> {
        self.export_names_contains(names, name).await
    }

    async fn excluded(&self, file: ProgramFile<'db>, name: &Name) -> RunResult<()> {
        if self
            .local(1, 0, || tracing::enabled!(tracing::Level::TRACE))
            .await?
        {
            let file = self.physical_file(file).await?;
            let path = self.field(file.read_fields(self.db()).path()).await?;
            let work = self
                .local(3, 0, || {
                    name.len()
                        .checked_add(path.as_str().len())
                        .and_then(|length| length.checked_add(64))
                })
                .await?;
            self.local(Self::checked(work)?, 0, || {
                tracing::trace!(
                    "Symbol `{}` (via star import) not found in `__all__` of `{}`",
                    name,
                    path
                );
            })
            .await?;
        }
        Ok(())
    }

    async fn imported_symbol(
        &self,
        env: &ProgramEnvironment<'db>,
        file: ProgramFile<'db>,
        name: &Name,
        reexport: Option<RequiresExplicitReExport>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        SourceEffects::imported_symbol(self, self.db(), env, Some(file), name, reexport).await
    }
}
