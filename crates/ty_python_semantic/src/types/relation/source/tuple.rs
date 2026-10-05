//! Controlled tuple relations borrow the existing checker and canonical tuple payloads.

use std::ops::ControlFlow;
use std::slice;

use itertools::EitherOrBoth;
use salsa::execution_probe::{RunError, RunResult};

use super::{
    BorrowedPairs, PairChildren, PairEffects, RelationSourceEffects, RelationSourceOperation,
};
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::local_transfer::local_with_fixed_transfers_at;
use crate::types::relation::{TypeRelation, TypeRelationChecker, TypeVarEvaluation};
use crate::types::tuple::buffer::TupleBuffer;
use crate::types::tuple::relation::{
    self, Direction, NormalizedElements, NormalizedPart, NormalizedStep, TupleRelationEffects,
    TupleRelationFacts, check_boundaries_with, check_fixed_pair_with, check_variable_pair_with,
    next_normalized_pair_with, next_normalized_with,
};
use crate::types::tuple::{
    TupleLength, TupleSpec, TupleType, VariableLengthTuple, VariableSegment,
};
use crate::types::{BoundTypeVarInstance, Type};

#[cfg(test)]
mod tests;

/// Borrows the caller's relation state while tuple cursors and buffers await their children.
pub(super) struct BorrowedTuplePairs<'pairs, 'effects, 'run, 'db: 'run, 'a, 'c, E, P> {
    pub(super) pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
    pub(super) checker: &'pairs TypeRelationChecker<'a, 'c, 'db>,
}

impl<'run, 'db: 'run + 'c, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    BorrowedTuplePairs<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    /// Admits fixed callback transfers and result carriers before a finite tuple operation.
    async fn local<T, F>(&self, work: usize, operation: F) -> RunResult<T>
    where
        F: FnOnce() -> RunResult<T>,
    {
        local_with_fixed_transfers_at(self.pairs.endpoint, work, 0, operation).await?
    }

    /// Packs borrowed remainder elements through the original tuple construction and interner.
    async fn pack(
        &self,
        prefix: &[Type<'db>],
        variable: Option<VariableSegment<'db>>,
        suffix: &[Type<'db>],
    ) -> RunResult<Type<'db>> {
        let capacity = self
            .local(3, || {
                prefix
                    .len()
                    .checked_add(suffix.len())
                    .ok_or(RunError::Contract("tuple remainder length overflow"))
            })
            .await?;
        let mut buffer = TupleBuffer::new(self.pairs.endpoint, self.pairs.effects, capacity, || ()).await?;
        let mut prefix = self.elements(prefix).await?;
        while let Some(element) = self.next(&mut prefix, Direction::Forward).await? {
            buffer
                .push(self.pairs.endpoint, self.pairs.effects, element, |(), _| ())
                .await?;
        }
        if let Some(variable) = variable {
            buffer.start_variable(self.pairs.endpoint, variable).await?;
        }
        let mut suffix = self.elements(suffix).await?;
        while let Some(element) = self.next(&mut suffix, Direction::Forward).await? {
            buffer
                .push(self.pairs.endpoint, self.pairs.effects, element, |(), _| ())
                .await?;
        }
        let spec = buffer.finish(self.pairs.endpoint, self.pairs.effects, |()| ()).await?;
        let tuple = self
            .pairs
            .effects
            .tuple_from_spec(self.checker.env, &spec)
            .await?;
        self.local(1, || Ok(Type::tuple(tuple))).await
    }
}

impl<'run, 'db: 'run + 'c, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    TupleRelationEffects<'c, 'db> for BorrowedTuplePairs<'_, '_, 'run, 'db, '_, 'c, E, P>
{
    type Error = RunError;
    type Fold = ConstraintFold<'db, 'c>;
    type Buffer = TupleBuffer<'db>;

    async fn checkpoint(&self) -> RunResult<()> {
        // Each reached shared comparison entry pays this charge, including nested entries.
        // One branch reaches at most 22 non-loop facts. Allow two arguments and one receiver
        // per fact, two transfers of every carrier, and the branch's selectors and wrappers.
        // Cursor visits and their repeated facts are funded by the cursor effects instead.
        let argument_bytes = size_of::<VariableSegment<'db>>()
            .max(size_of::<&[Type<'db>]>())
            .max(size_of::<(TypeRelation, TypeVarEvaluation)>())
            .max(size_of::<usize>())
            .max(size_of::<
                &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
            >());
        let bytes = 2
            * (22 * size_of::<&TupleRelationFacts>()
                + 44 * argument_bytes
                + 10 * size_of::<VariableSegment<'db>>()
                + 8 * size_of::<&[Type<'db>]>()
                + 8 * size_of::<usize>()
                + 4 * size_of::<bool>()
                + 3 * size_of::<Type<'db>>()
                + 2 * size_of::<Option<Type<'db>>>()
                + 4 * size_of::<BoundTypeVarInstance<'db>>()
                + size_of::<TupleLength>());
        local_with_fixed_transfers_at(self.pairs.endpoint, 256, bytes, || ()).await
    }

    async fn spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        self.pairs.effects.tuple_spec(tuple).await
    }

    async fn mode(&self) -> RunResult<(TypeRelation, TypeVarEvaluation)> {
        self.local(2, || {
            Ok((self.checker.relation, self.checker.typevar_evaluation))
        })
        .await
    }

    async fn has_context(&self) -> RunResult<bool> {
        self.local(1, || Ok(self.checker.report_context().is_some()))
            .await
    }

    async fn report_length(&self, _source_len: usize, _target_len: TupleLength) -> RunResult<()> {
        self.pairs
            .unavailable(RelationSourceOperation::TupleContext)
            .await
    }

    async fn report_element(
        &self,
        _source: Type<'db>,
        _target: Type<'db>,
        _index: usize,
        _count: usize,
    ) -> RunResult<()> {
        self.pairs
            .unavailable(RelationSourceOperation::TupleContext)
            .await
    }

    async fn constant(&self, value: bool) -> RunResult<ConstraintSet<'db, 'c>> {
        self.local(2, || {
            Ok(ConstraintSet::from_bool(self.checker.constraints, value))
        })
        .await
    }

    async fn is_never(&self, value: ConstraintSet<'db, 'c>) -> RunResult<bool> {
        self.local(2, || {
            value.verify_builder(self.checker.constraints);
            Ok(value.is_trivially_never_satisfied())
        })
        .await
    }

    async fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> RunResult<bool> {
        self.pairs.satisfy(value, false).await
    }

    async fn pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .check_type_pair(self.checker, source, target)
            .await
    }

    async fn pair_without_context(
        &self,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .unavailable(RelationSourceOperation::TupleContext)
            .await
    }

    async fn conjoin(
        &self,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .combine_constraints(
                self.checker.constraints,
                ConstraintFoldKind::All,
                left,
                right,
            )
            .await
    }

    async fn fold_start(&self) -> RunResult<Self::Fold> {
        self.local(4, || {
            Ok(ConstraintFold::new(
                self.checker.constraints,
                ConstraintFoldKind::All,
            ))
        })
        .await
    }

    async fn fold_push(
        &self,
        fold: &mut Self::Fold,
        next: ConstraintSet<'db, 'c>,
    ) -> RunResult<ControlFlow<ConstraintSet<'db, 'c>>> {
        self.pairs.push_constraints(fold, next).await
    }

    async fn fold_finish(&self, fold: &mut Self::Fold) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs.finish_constraints(fold).await
    }

    async fn elements<'a>(
        &self,
        elements: &'a [Type<'db>],
    ) -> RunResult<slice::Iter<'a, Type<'db>>> {
        self.local(1, || Ok(elements.iter())).await
    }

    async fn next(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
        direction: Direction,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(4, || Ok(relation::next_element(elements, direction)))
            .await
    }

    async fn next_zip(
        &self,
        source: &mut slice::Iter<'_, Type<'db>>,
        target: &mut slice::Iter<'_, Type<'db>>,
    ) -> RunResult<Option<(Type<'db>, Type<'db>)>> {
        // The fixed-pair loop may read its length and advance its diagnostic index on each visit.
        let bytes = 2
            * (2 * size_of::<&TupleRelationFacts>()
                + size_of::<&[Type<'db>]>()
                + 3 * size_of::<usize>());
        local_with_fixed_transfers_at(self.pairs.endpoint, 23, bytes, || {
            relation::next_zip(source, target)
        })
        .await
    }

    async fn next_longest(
        &self,
        source: &mut slice::Iter<'_, Type<'db>>,
        target: &mut slice::Iter<'_, Type<'db>>,
        direction: Direction,
    ) -> RunResult<Option<EitherOrBoth<Type<'db>, Type<'db>>>> {
        local_with_fixed_transfers_at(self.pairs.endpoint, 33, overhang_fact_bytes(), || {
            relation::next_longest(source, target, direction)
        })
        .await
    }

    async fn same_pack(
        &self,
        source: BoundTypeVarInstance<'db>,
        target: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let source = self.pairs.effects.tuple_pack_identity(source).await?;
        let target = self.pairs.effects.tuple_pack_identity(target).await?;
        self.local(1, || Ok(source == target)).await
    }

    async fn inferable(&self, pack: BoundTypeVarInstance<'db>) -> RunResult<bool> {
        self.pairs.typevar_is_inferable(self.checker, pack).await
    }

    async fn gradual_element(&self, segment: VariableSegment<'db>) -> RunResult<Option<Type<'db>>> {
        let homogeneous = self
            .local(1, || {
                Ok(match segment {
                    VariableSegment::Homogeneous(element) => Some(element),
                    VariableSegment::TypeVarTuple(_) => None,
                })
            })
            .await?;
        match homogeneous {
            None => Ok(None),
            Some(_) => {
                self.pairs
                    .unavailable(RelationSourceOperation::TupleGradualArity)
                    .await
            }
        }
    }

    async fn empty_protocol(&self) -> RunResult<Type<'db>> {
        self.pairs
            .unavailable(RelationSourceOperation::TupleProtocol)
            .await
    }

    async fn pack_fixed(&self, elements: &[Type<'db>]) -> RunResult<Type<'db>> {
        self.pack(elements, None, &[]).await
    }

    async fn pack_variable(
        &self,
        prefix: &[Type<'db>],
        variable: VariableSegment<'db>,
        suffix: &[Type<'db>],
    ) -> RunResult<Type<'db>> {
        self.pack(prefix, Some(variable), suffix).await
    }

    async fn normalized_start<'a>(
        &self,
        tuple: &'a VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        variable: Option<Type<'db>>,
        part: NormalizedPart,
    ) -> RunResult<NormalizedElements<'a, 'db>> {
        self.local(9, || Ok(relation::normalized_start(tuple, variable, part)))
            .await
    }

    async fn normalized_step(
        &self,
        elements: &mut NormalizedElements<'_, 'db>,
    ) -> RunResult<Option<NormalizedStep<'db>>> {
        self.local(10, || Ok(relation::normalized_step(elements)))
            .await
    }

    async fn normalized_decision(
        &self,
        elements: &mut NormalizedElements<'_, 'db>,
        element: Type<'db>,
        equivalent: bool,
    ) -> RunResult<ControlFlow<Option<Type<'db>>>> {
        self.local(3, || {
            Ok(relation::normalized_decision(elements, element, equivalent))
        })
        .await
    }

    async fn equivalent(&self, _element: Type<'db>, _variable: Type<'db>) -> RunResult<bool> {
        self.pairs
            .unavailable(RelationSourceOperation::TuplePrenormalization)
            .await
    }

    async fn next_normalized(
        &self,
        elements: &mut NormalizedElements<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        next_normalized_with(elements, self).await
    }

    async fn next_normalized_pair(
        &self,
        source: &mut NormalizedElements<'_, 'db>,
        target: &mut NormalizedElements<'_, 'db>,
    ) -> RunResult<Option<EitherOrBoth<Type<'db>, Type<'db>>>> {
        // Fund each pair construction and its fixed return carriers before either child advances.
        local_with_fixed_transfers_at(
            self.pairs.endpoint,
            31,
            size_of::<Option<EitherOrBoth<Type<'db>, Type<'db>>>>()
                + size_of::<RunResult<Option<EitherOrBoth<Type<'db>, Type<'db>>>>>() * 2
                + 2 * (size_of::<&TupleRelationFacts>() + 2 * size_of::<Option<Type<'db>>>())
                + overhang_fact_bytes(),
            || (),
        )
        .await?;
        next_normalized_pair_with(source, target, self, TupleRelationFacts).await
    }

    async fn buffer_start(&self) -> RunResult<Self::Buffer> {
        TupleBuffer::new(self.pairs.endpoint, self.pairs.effects, 0, || ()).await
    }

    async fn buffer_push(&self, buffer: &mut Self::Buffer, element: Type<'db>) -> RunResult<()> {
        buffer.push(self.pairs.endpoint, self.pairs.effects, element, |(), _| ()).await
    }

    async fn buffer_elements<'a>(
        &self,
        buffer: &'a Self::Buffer,
    ) -> RunResult<slice::Iter<'a, Type<'db>>>
    where
        'db: 'a,
    {
        self.local(2, || Ok(buffer.elements().iter())).await
    }

    async fn fixed_pair(
        &self,
        source: &[Type<'db>],
        target: &TupleSpec<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_fixed_pair_with(source, target, self, TupleRelationFacts).await
    }

    async fn variable_pair(
        &self,
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &TupleSpec<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_variable_pair_with(source, target, self, TupleRelationFacts).await
    }

    async fn boundaries(
        &self,
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        source_variable: Type<'db>,
        target_variable: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_boundaries_with(
            source,
            target,
            source_variable,
            target_variable,
            self,
            TupleRelationFacts,
        )
        .await
    }
}

/// Quotes repeated mode and source-segment facts for one overhanging endpoint.
const fn overhang_fact_bytes() -> usize {
    2 * (3 * size_of::<&TupleRelationFacts>()
        + size_of::<(TypeRelation, TypeVarEvaluation)>()
        + size_of::<&VariableLengthTuple<Type<'_>, VariableSegment<'_>>>()
        + size_of::<Option<Type<'_>>>()
        + size_of::<VariableSegment<'_>>()
        + 2 * size_of::<bool>())
}
