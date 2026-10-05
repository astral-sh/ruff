//! Tuple relations preserve endpoint order, pack identity, and the caller's comparison resources.

use std::convert::Infallible;
use std::ops::ControlFlow;
use std::slice;

use itertools::EitherOrBoth;

use super::{Tuple, TupleLength, TupleSpec, TupleType, VariableLengthTuple, VariableSegment};
use crate::Db;
use crate::types::constraints::{ConstraintFold, ConstraintFoldKind, ConstraintSet};
use crate::types::relation::{TypeRelation, TypeRelationChecker, TypeVarEvaluation};
use crate::types::{BoundTypeVarInstance, ErrorContext, Type};

#[cfg(test)]
mod tests;

/// Selects which end of a borrowed tuple sequence is consumed next.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum Direction {
    Forward,
    Backward,
}

/// Selects the prefix or suffix view produced by tuple prenormalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum NormalizedPart {
    Prefix,
    Suffix,
}

/// Retains borrowed endpoints while equivalence decides whether a suffix element moves to the prefix.
#[derive(Debug)]
pub(in crate::types) struct NormalizedElements<'a, 'db> {
    prefix: slice::Iter<'a, Type<'db>>,
    suffix: slice::Iter<'a, Type<'db>>,
    variable: Type<'db>,
    part: NormalizedPart,
    scanning: bool,
    finished: bool,
}

/// Describes one admitted cursor advance before any required equivalence child.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum NormalizedStep<'db> {
    Yield(Type<'db>),
    Compare {
        element: Type<'db>,
        variable: Type<'db>,
    },
}

/// Provides finite tuple shape decisions without invoking semantic children.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct TupleRelationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTupleRelationEffects)]
    pub(in crate::types) trait TupleRelationEffects<'c, 'db: 'c> {
        type Error;
        type Fold;
        type Buffer;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Self::Error>;
        #[operation(local)]
        async fn mode(&self) -> Result<(TypeRelation, TypeVarEvaluation), Self::Error>;
        #[operation(local)]
        async fn has_context(&self) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_length(
            &self,
            source_len: usize,
            target_len: TupleLength,
        ) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_element(
            &self,
            source: Type<'db>,
            target: Type<'db>,
            index: usize,
            count: usize,
        ) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn constant(&self, value: bool) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn is_never(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn pair(
            &self,
            source: Type<'db>,
            target: Type<'db>,
        ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn pair_without_context(
            &self,
            source: Type<'db>,
            target: Type<'db>,
        ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn conjoin(
            &self,
            left: ConstraintSet<'db, 'c>,
            right: ConstraintSet<'db, 'c>,
        ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn fold_start(&self) -> Result<Self::Fold, Self::Error>;
        #[operation(child)]
        async fn fold_push(
            &self,
            fold: &mut Self::Fold,
            next: ConstraintSet<'db, 'c>,
        ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Self::Error>;
        #[operation(child)]
        async fn fold_finish(
            &self,
            fold: &mut Self::Fold,
        ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(local)]
        async fn elements<'a>(
            &self,
            elements: &'a [Type<'db>],
        ) -> Result<slice::Iter<'a, Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next(
            &self,
            elements: &mut slice::Iter<'_, Type<'db>>,
            direction: Direction,
        ) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_zip(
            &self,
            source: &mut slice::Iter<'_, Type<'db>>,
            target: &mut slice::Iter<'_, Type<'db>>,
        ) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_longest(
            &self,
            source: &mut slice::Iter<'_, Type<'db>>,
            target: &mut slice::Iter<'_, Type<'db>>,
            direction: Direction,
        ) -> Result<Option<EitherOrBoth<Type<'db>, Type<'db>>>, Self::Error>;
        #[operation(child)]
        async fn same_pack(
            &self,
            source: BoundTypeVarInstance<'db>,
            target: BoundTypeVarInstance<'db>,
        ) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn inferable(&self, pack: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn gradual_element(
            &self,
            segment: VariableSegment<'db>,
        ) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn empty_protocol(&self) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn pack_fixed(&self, elements: &[Type<'db>]) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn pack_variable(
            &self,
            prefix: &[Type<'db>],
            variable: VariableSegment<'db>,
            suffix: &[Type<'db>],
        ) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn normalized_start<'a>(
            &self,
            tuple: &'a VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
            variable: Option<Type<'db>>,
            part: NormalizedPart,
        ) -> Result<NormalizedElements<'a, 'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn normalized_step(
            &self,
            elements: &mut NormalizedElements<'_, 'db>,
        ) -> Result<Option<NormalizedStep<'db>>, Self::Error>;
        #[operation(local)]
        async fn normalized_decision(
            &self,
            elements: &mut NormalizedElements<'_, 'db>,
            element: Type<'db>,
            equivalent: bool,
        ) -> Result<ControlFlow<Option<Type<'db>>>, Self::Error>;
        #[operation(child)]
        async fn equivalent(
            &self,
            element: Type<'db>,
            variable: Type<'db>,
        ) -> Result<bool, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_normalized(
            &self,
            elements: &mut NormalizedElements<'_, 'db>,
        ) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        #[progress]
        async fn next_normalized_pair(
            &self,
            source: &mut NormalizedElements<'_, 'db>,
            target: &mut NormalizedElements<'_, 'db>,
        ) -> Result<Option<EitherOrBoth<Type<'db>, Type<'db>>>, Self::Error>;
        #[operation(local)]
        async fn buffer_start(&self) -> Result<Self::Buffer, Self::Error>;
        #[operation(local)]
        async fn buffer_push(
            &self,
            buffer: &mut Self::Buffer,
            element: Type<'db>,
        ) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn buffer_elements<'a>(
            &self,
            buffer: &'a Self::Buffer,
        ) -> Result<slice::Iter<'a, Type<'db>>, Self::Error>
        where
            'db: 'a;
        #[operation(child)]
        async fn fixed_pair(
            &self,
            source: &[Type<'db>],
            target: &TupleSpec<'db>,
        ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn variable_pair(
            &self,
            source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
            target: &TupleSpec<'db>,
        ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
        #[operation(child)]
        async fn boundaries(
            &self,
            source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
            target: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
            source_variable: Type<'db>,
            target_variable: Type<'db>,
        ) -> Result<ConstraintSet<'db, 'c>, Self::Error>;
    }

    #[finite_capability]
    impl TupleRelationFacts {
        fn len(&self, elements: &[Type<'_>]) -> usize {
            elements.len()
        }
        fn all<'a, 'db>(&self, tuple: &'a super::FixedLengthTuple<Type<'db>>) -> &'a [Type<'db>] {
            tuple.all_elements()
        }
        fn prefix<'a, 'db>(
            &self,
            tuple: &'a VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        ) -> &'a [Type<'db>] {
            tuple.prefix_elements()
        }
        fn suffix<'a, 'db>(
            &self,
            tuple: &'a VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        ) -> &'a [Type<'db>] {
            tuple.suffix_elements()
        }
        fn variable<'db>(
            &self,
            tuple: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        ) -> VariableSegment<'db> {
            tuple.variable()
        }
        fn tuple_len(&self, tuple: &TupleSpec<'_>) -> TupleLength {
            tuple.len()
        }
        fn minimum(&self, tuple: &VariableLengthTuple<Type<'_>, VariableSegment<'_>>) -> usize {
            tuple.len().minimum()
        }
        fn eager(&self, mode: (TypeRelation, TypeVarEvaluation)) -> bool {
            mode.0.is_assignability() && mode.1 == TypeVarEvaluation::Eager
        }
        fn assignability(&self, mode: (TypeRelation, TypeVarEvaluation)) -> bool {
            mode.0.is_assignability()
        }
        fn lazy(&self, mode: (TypeRelation, TypeVarEvaluation)) -> bool {
            mode.1 == TypeVarEvaluation::Lazy
        }
        fn equal(&self, left: usize, right: usize) -> bool {
            left == right
        }
        fn less(&self, left: usize, right: usize) -> bool {
            left < right
        }
        fn more(&self, left: usize, right: usize) -> bool {
            left > right
        }
        fn subtract(&self, left: usize, right: usize) -> usize {
            left - right
        }
        fn increment(&self, index: usize) -> usize {
            index + 1
        }
        fn absent(&self, ty: Option<Type<'_>>) -> bool {
            ty.is_none()
        }
        fn before<'a, 'db>(&self, elements: &'a [Type<'db>], end: usize) -> &'a [Type<'db>] {
            &elements[..end]
        }
        fn after<'a, 'db>(&self, elements: &'a [Type<'db>], start: usize) -> &'a [Type<'db>] {
            &elements[start..]
        }
        fn remaining<'a, 'db>(&self, elements: &slice::Iter<'a, Type<'db>>) -> &'a [Type<'db>] {
            elements.as_slice()
        }
        fn element<'db>(&self, segment: VariableSegment<'db>) -> Type<'db> {
            match segment {
                VariableSegment::Homogeneous(ty) => ty,
                VariableSegment::TypeVarTuple(_) => Type::object(),
            }
        }
        fn pair<'db>(
            &self,
            source: Option<Type<'db>>,
            target: Option<Type<'db>>,
        ) -> Option<EitherOrBoth<Type<'db>, Type<'db>>> {
            match (source, target) {
                (Some(source), Some(target)) => Some(EitherOrBoth::Both(source, target)),
                (Some(source), None) => Some(EitherOrBoth::Left(source)),
                (None, Some(target)) => Some(EitherOrBoth::Right(target)),
                (None, None) => None,
            }
        }
    }

    /// Compares canonical exact tuples using the caller's relation and constraint builder.
    #[synchronous(check_tuple_pair_sync)]
    #[capabilities(effects = TupleRelationEffects, facts = TupleRelationFacts)]
    #[passive_values()]
    pub(in crate::types) async fn check_tuple_pair_with<
        'c,
        'db: 'c,
        E: TupleRelationEffects<'c, 'db>,
    >(
        source: TupleType<'db>,
        target: TupleType<'db>,
        effects: &E,
        facts: TupleRelationFacts,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        effects.checkpoint().await?;
        let source = effects.spec(source).await?;
        match source {
            Tuple::Fixed(source) => {
                let target = effects.spec(target).await?;
                effects.fixed_pair(facts.all(source), target).await
            }
            Tuple::Variable(source) => {
                let target = effects.spec(target).await?;
                effects.variable_pair(source, target).await
            }
        }
    }

    /// Compares fixed source elements with the target's fixed endpoints and remaining segment.
    #[synchronous(check_fixed_pair_sync)]
    #[capabilities(effects = TupleRelationEffects, facts = TupleRelationFacts)]
    #[passive_values(Direction::Forward, Direction::Backward, Type::TypeVar)]
    pub(in crate::types) async fn check_fixed_pair_with<
        'c,
        'db: 'c,
        E: TupleRelationEffects<'c, 'db>,
    >(
        source: &[Type<'db>],
        target: &TupleSpec<'db>,
        effects: &E,
        facts: TupleRelationFacts,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        effects.checkpoint().await?;
        match target {
            Tuple::Fixed(target_fixed) => {
                let target_elements = facts.all(target_fixed);
                let equal_length = facts.equal(facts.len(source), facts.len(target_elements));
                if effects.has_context().await? && !equal_length && facts.eager(effects.mode().await?) {
                    effects
                        .report_length(facts.len(source), facts.tuple_len(target))
                        .await?;
                }
                let length_constraints = effects.constant(equal_length).await?;
                if effects.is_never(length_constraints).await? {
                    return Ok(length_constraints);
                }
                let mut source_iter = effects.elements(source).await?;
                let mut target_iter = effects.elements(target_elements).await?;
                let mut fold = effects.fold_start().await?;
                #[passive_state]
                let mut index = 1;
                #[passive_state]
                let mut saturated = None;
                #[cursor_loop]
                while let Some(pair) = effects.next_zip(&mut source_iter, &mut target_iter).await? {
                    let (source_ty, target_ty) = pair;
                    let constraint_set = effects.pair(source_ty, target_ty).await?;
                    if effects.has_context().await?
                        && effects.is_never_satisfied(constraint_set).await?
                    {
                        effects
                            .report_element(source_ty, target_ty, index, facts.len(source))
                            .await?;
                    }
                    index = facts.increment(index);
                    if let ControlFlow::Break(result) =
                        effects.fold_push(&mut fold, constraint_set).await?
                    {
                        saturated = Some(result);
                        break;
                    }
                }
                let elements = match saturated {
                    Some(result) => result,
                    None => effects.fold_finish(&mut fold).await?,
                };
                effects.conjoin(length_constraints, elements).await
            }
            Tuple::Variable(target) => {
                // This tuple must have enough elements to match up with the other tuple's prefix
                // and suffix, and each of those elements must pairwise satisfy the relation.
                #[passive_state]
                let mut result = effects.constant(true).await?;
                let mut source_iter = effects.elements(source).await?;
                let mut prefix = effects.elements(facts.prefix(target)).await?;
                #[cursor_loop]
                while let Some(target_ty) = effects.next(&mut prefix, Direction::Forward).await? {
                    let Some(source_ty) = effects.next(&mut source_iter, Direction::Forward).await?
                    else {
                        return effects.constant(false).await;
                    };
                    let next = effects.pair(source_ty, target_ty).await?;
                    result = effects.conjoin(result, next).await?;
                    if effects.is_never(result).await? {
                        return Ok(result);
                    }
                }
                let mut suffix = effects.elements(facts.suffix(target)).await?;
                #[cursor_loop]
                while let Some(target_ty) = effects.next(&mut suffix, Direction::Backward).await? {
                    let Some(source_ty) = effects.next(&mut source_iter, Direction::Backward).await?
                    else {
                        return effects.constant(false).await;
                    };
                    let next = effects.pair(source_ty, target_ty).await?;
                    result = effects.conjoin(result, next).await?;
                    if effects.is_never(result).await? {
                        return Ok(result);
                    }
                }
                match facts.variable(target) {
                    VariableSegment::TypeVarTuple(pack) => {
                        let packed = effects.pack_fixed(facts.remaining(&source_iter)).await?;
                        if effects.is_never(result).await? {
                            return Ok(result);
                        }
                        let next = effects.pair(packed, Type::TypeVar(pack)).await?;
                        effects.conjoin(result, next).await
                    }
                    VariableSegment::Homogeneous(target_ty) => {
                        // In addition, any remaining elements in this tuple must satisfy the
                        // variable-length portion of the other tuple.
                        if effects.is_never(result).await? {
                            return Ok(result);
                        }
                        let mut fold = effects.fold_start().await?;
                        #[passive_state]
                        let mut saturated = None;
                        #[cursor_loop]
                        while let Some(source_ty) =
                            effects.next(&mut source_iter, Direction::Forward).await?
                        {
                            let next = effects.pair(source_ty, target_ty).await?;
                            if let ControlFlow::Break(value) =
                                effects.fold_push(&mut fold, next).await?
                            {
                                saturated = Some(value);
                                break;
                            }
                        }
                        let middle = match saturated {
                            Some(value) => value,
                            None => effects.fold_finish(&mut fold).await?,
                        };
                        effects.conjoin(result, middle).await
                    }
                }
            }
        }
    }

    /// Compares variable source tuples without changing gradual arity or symbolic-pack semantics.
    #[synchronous(check_variable_pair_sync)]
    #[capabilities(effects = TupleRelationEffects, facts = TupleRelationFacts)]
    #[passive_values(
        Direction::Forward,
        Direction::Backward,
        NormalizedPart::Prefix,
        NormalizedPart::Suffix,
        Type::TypeVar,
        Type::Never
    )]
    pub(in crate::types) async fn check_variable_pair_with<
        'c,
        'db: 'c,
        E: TupleRelationEffects<'c, 'db>,
    >(
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &TupleSpec<'db>,
        effects: &E,
        facts: TupleRelationFacts,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        effects.checkpoint().await?;
        match target {
            Tuple::Fixed(target) => {
                // The `...` length specifier of a variable-length tuple type is interpreted
                // differently depending on the type of the variable-length elements.
                //
                // It typically represents the _union_ of all possible lengths. That means that a
                // variable-length tuple type is not a subtype of _any_ fixed-length tuple type.
                //
                // However, as a special case, if the variable-length portion of the tuple is `Any`
                // (or any other dynamic type), then the `...` is the _gradual choice_ of all
                // possible lengths. This means that `tuple[Any, ...]` can match any tuple of any
                // length.
                //
                // Unlike a dynamic homogeneous segment, a symbolic type variable tuple ranges
                // over all specializations rather than making a gradual choice of length.
                if !facts.assignability(effects.mode().await?) {
                    return effects.constant(false).await;
                }
                let Some(source_element) = effects.gradual_element(facts.variable(source)).await?
                else {
                    return effects.constant(false).await;
                };
                // In addition, the other tuple must have enough elements to match up with this
                // tuple's prefix and suffix, and each of those elements must pairwise satisfy the
                // relation.
                #[passive_state]
                let mut result = effects.constant(true).await?;
                let mut target_iter = effects.elements(facts.all(target)).await?;
                let mut prefix = effects
                    .normalized_start(source, None, NormalizedPart::Prefix)
                    .await?;
                #[cursor_loop]
                while let Some(source_ty) = effects.next_normalized(&mut prefix).await? {
                    let Some(target_ty) = effects.next(&mut target_iter, Direction::Forward).await?
                    else {
                        return effects.constant(false).await;
                    };
                    let next = effects.pair(source_ty, target_ty).await?;
                    result = effects.conjoin(result, next).await?;
                    if effects.is_never(result).await? {
                        return Ok(result);
                    }
                }
                let mut suffix = effects
                    .normalized_start(source, None, NormalizedPart::Suffix)
                    .await?;
                let mut buffer = effects.buffer_start().await?;
                #[cursor_loop]
                while let Some(source_ty) = effects.next_normalized(&mut suffix).await? {
                    effects.buffer_push(&mut buffer, source_ty).await?;
                }
                let mut suffix = effects.buffer_elements(&buffer).await?;
                #[cursor_loop]
                while let Some(source_ty) = effects.next(&mut suffix, Direction::Backward).await? {
                    let Some(target_ty) = effects.next(&mut target_iter, Direction::Backward).await?
                    else {
                        return effects.constant(false).await;
                    };
                    let next = effects.pair(source_ty, target_ty).await?;
                    result = effects.conjoin(result, next).await?;
                    if effects.is_never(result).await? {
                        return Ok(result);
                    }
                }
                // The gradual segment supplies the remaining elements.
                if effects.is_never(result).await? {
                    return Ok(result);
                }
                let mut fold = effects.fold_start().await?;
                #[passive_state]
                let mut saturated = None;
                #[cursor_loop]
                while let Some(target_ty) = effects.next(&mut target_iter, Direction::Forward).await? {
                    let next = effects.pair(source_element, target_ty).await?;
                    if let ControlFlow::Break(value) = effects.fold_push(&mut fold, next).await? {
                        saturated = Some(value);
                        break;
                    }
                }
                let middle = match saturated {
                    Some(value) => value,
                    None => effects.fold_finish(&mut fold).await?,
                };
                effects.conjoin(result, middle).await
            }
            Tuple::Variable(target) => {
                if let (
                    VariableSegment::TypeVarTuple(source_pack),
                    VariableSegment::TypeVarTuple(target_pack),
                ) = (facts.variable(source), facts.variable(target))
                    && effects.same_pack(source_pack, target_pack).await?
                {
                    if !facts.equal(
                        facts.len(facts.prefix(source)),
                        facts.len(facts.prefix(target)),
                    ) || !facts.equal(
                        facts.len(facts.suffix(source)),
                        facts.len(facts.suffix(target)),
                    ) {
                        return effects.constant(false).await;
                    }
                    let mut source_prefix = effects.elements(facts.prefix(source)).await?;
                    let mut target_prefix = effects.elements(facts.prefix(target)).await?;
                    let mut source_suffix = effects.elements(facts.suffix(source)).await?;
                    let mut target_suffix = effects.elements(facts.suffix(target)).await?;
                    let mut fold = effects.fold_start().await?;
                    #[cursor_loop]
                    while let Some(pair) = effects
                        .next_zip(&mut source_prefix, &mut target_prefix)
                        .await?
                    {
                        let (source_ty, target_ty) = pair;
                        let next = effects.pair(source_ty, target_ty).await?;
                        if let ControlFlow::Break(value) = effects.fold_push(&mut fold, next).await? {
                            return Ok(value);
                        }
                    }
                    #[cursor_loop]
                    while let Some(pair) = effects
                        .next_zip(&mut source_suffix, &mut target_suffix)
                        .await?
                    {
                        let (source_ty, target_ty) = pair;
                        let next = effects.pair(source_ty, target_ty).await?;
                        if let ControlFlow::Break(value) = effects.fold_push(&mut fold, next).await? {
                            return Ok(value);
                        }
                    }
                    return effects.fold_finish(&mut fold).await;
                }
                // TODO: Extend lazy inference for mixed gradual tuples: let gradual segments
                // supply fixed target elements, and generate constraints for source packs that
                // overlap fixed target elements.
                if facts.lazy(effects.mode().await?)
                    && let VariableSegment::TypeVarTuple(pack) = facts.variable(target)
                {
                    let source_prefix = facts.prefix(source);
                    let source_suffix = facts.suffix(source);
                    let target_prefix = facts.prefix(target);
                    let target_suffix = facts.suffix(target);
                    if facts.less(facts.len(source_prefix), facts.len(target_prefix))
                        || facts.less(facts.len(source_suffix), facts.len(target_suffix))
                    {
                        return effects.constant(false).await;
                    }
                    let suffix_start =
                        facts.subtract(facts.len(source_suffix), facts.len(target_suffix));
                    let mut source_prefix_iter = effects.elements(source_prefix).await?;
                    let mut target_prefix_iter = effects.elements(target_prefix).await?;
                    let mut source_suffix_iter = effects
                        .elements(facts.after(source_suffix, suffix_start))
                        .await?;
                    let mut target_suffix_iter = effects.elements(target_suffix).await?;
                    let mut fold = effects.fold_start().await?;
                    #[passive_state]
                    let mut saturated = None;
                    #[cursor_loop]
                    while let Some(pair) = effects
                        .next_zip(&mut source_prefix_iter, &mut target_prefix_iter)
                        .await?
                    {
                        let (source_ty, target_ty) = pair;
                        let next = effects.pair(source_ty, target_ty).await?;
                        if let ControlFlow::Break(value) = effects.fold_push(&mut fold, next).await? {
                            saturated = Some(value);
                            break;
                        }
                    }
                    if let None = saturated {
                        #[cursor_loop]
                        while let Some(pair) = effects
                            .next_zip(&mut source_suffix_iter, &mut target_suffix_iter)
                            .await?
                        {
                            let (source_ty, target_ty) = pair;
                            let next = effects.pair(source_ty, target_ty).await?;
                            if let ControlFlow::Break(value) =
                                effects.fold_push(&mut fold, next).await?
                            {
                                saturated = Some(value);
                                break;
                            }
                        }
                    }
                    let boundaries = match saturated {
                        Some(value) => value,
                        None => effects.fold_finish(&mut fold).await?,
                    };
                    let packed = effects
                        .pack_variable(
                            facts.after(source_prefix, facts.len(target_prefix)),
                            facts.variable(source),
                            facts.before(source_suffix, suffix_start),
                        )
                        .await?;
                    if effects.is_never(boundaries).await? {
                        return Ok(boundaries);
                    }
                    let next = effects.pair(packed, Type::TypeVar(pack)).await?;
                    return effects.conjoin(boundaries, next).await;
                }
                // These checks must hold for every specialization of a non-inferable pack.
                // Lazy evaluation needs to retain pack constraints for later solving; its empty
                // `inferable` set does not imply universal quantification.
                if facts.eager(effects.mode().await?) {
                    if let VariableSegment::TypeVarTuple(target_pack) = facts.variable(target)
                        && !effects.inferable(target_pack).await?
                        && let Some(source_element) =
                            effects.gradual_element(facts.variable(source)).await?
                    {
                        // The pack may be empty, so the source cannot require more elements
                        // than the target's fixed ends. For longer packs, source endpoints
                        // extending into the pack must be assignable to every element type,
                        // which we check against `Never`. This also covers their overlap with
                        // the opposite fixed end when the pack is short.
                        if facts.more(facts.minimum(source), facts.minimum(target)) {
                            return effects.constant(false).await;
                        }
                        return effects
                            .boundaries(source, target, source_element, Type::Never)
                            .await;
                    }
                    if let VariableSegment::TypeVarTuple(source_pack) = facts.variable(source)
                        && !effects.inferable(source_pack).await?
                        && let Some(target_element) =
                            effects.gradual_element(facts.variable(target)).await?
                    {
                        // Conversely, the target's required elements must fit even with an
                        // empty source pack. A target endpoint extending into the pack must
                        // accept any possible element. An empty protocol expresses this without
                        // inheriting `object`'s permissive assignability to hash protocols.
                        if facts.less(facts.minimum(source), facts.minimum(target)) {
                            return effects.constant(false).await;
                        }
                        let source_element = effects.empty_protocol().await?;
                        return effects
                            .boundaries(source, target, source_element, target_element)
                            .await;
                    }
                }
                if let VariableSegment::TypeVarTuple(_) = facts.variable(target) {
                    // A fully gradual source imposes no length or element constraints, even
                    // when the target pack is inferable.
                    let gradual = match facts.variable(source) {
                        VariableSegment::Homogeneous(Type::Dynamic(_)) => true,
                        VariableSegment::Homogeneous(_) | VariableSegment::TypeVarTuple(_) => false,
                    };
                    return effects
                        .constant(
                            facts.eager(effects.mode().await?)
                                && facts.equal(facts.minimum(source), 0)
                                && gradual,
                        )
                        .await;
                }
                // When prenormalizing below, we assume that a dynamic variable-length portion of
                // one tuple materializes to the variable-length portion of the other tuple.
                let source_variable = facts.element(facts.variable(source));
                let target_variable = facts.element(facts.variable(target));
                let source_prenormalize_variable = match facts.variable(source) {
                    VariableSegment::Homogeneous(Type::Dynamic(_)) => Some(target_variable),
                    VariableSegment::Homogeneous(_) | VariableSegment::TypeVarTuple(_) => None,
                };
                let target_prenormalize_variable = match facts.variable(target) {
                    VariableSegment::Homogeneous(Type::Dynamic(_)) => Some(source_variable),
                    VariableSegment::Homogeneous(_) | VariableSegment::TypeVarTuple(_) => None,
                };
                // The overlapping parts of the prefixes and suffixes must satisfy the relation.
                // Any remaining parts must satisfy the relation with the other tuple's
                // variable-length part.
                #[passive_state]
                let mut result = effects.constant(true).await?;
                let mut source_prefix = effects
                    .normalized_start(source, source_prenormalize_variable, NormalizedPart::Prefix)
                    .await?;
                let mut target_prefix = effects
                    .normalized_start(target, target_prenormalize_variable, NormalizedPart::Prefix)
                    .await?;
                #[cursor_loop]
                while let Some(pair) = effects
                    .next_normalized_pair(&mut source_prefix, &mut target_prefix)
                    .await?
                {
                    let next = match pair {
                        EitherOrBoth::Both(source_ty, target_ty) => {
                            effects.pair(source_ty, target_ty).await?
                        }
                        EitherOrBoth::Left(source_ty) => {
                            effects.pair(source_ty, target_variable).await?
                        }
                        EitherOrBoth::Right(target_ty) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide, unless the lhs has a dynamic variable-length portion
                            // that can materialize to provide it (for assignability only),
                            // as in `tuple[Any, ...]` matching `tuple[int, int]`.
                            if !facts.assignability(effects.mode().await?)
                                || facts.absent(effects.gradual_element(facts.variable(source)).await?)
                            {
                                return effects.constant(false).await;
                            }
                            effects.pair(source_variable, target_ty).await?
                        }
                    };
                    result = effects.conjoin(result, next).await?;
                    if effects.is_never(result).await? {
                        return Ok(result);
                    }
                }
                let mut source_suffix = effects
                    .normalized_start(source, source_prenormalize_variable, NormalizedPart::Suffix)
                    .await?;
                let mut source_buffer = effects.buffer_start().await?;
                #[cursor_loop]
                while let Some(source_ty) = effects.next_normalized(&mut source_suffix).await? {
                    effects.buffer_push(&mut source_buffer, source_ty).await?;
                }
                let mut target_suffix = effects
                    .normalized_start(target, target_prenormalize_variable, NormalizedPart::Suffix)
                    .await?;
                let mut target_buffer = effects.buffer_start().await?;
                #[cursor_loop]
                while let Some(target_ty) = effects.next_normalized(&mut target_suffix).await? {
                    effects.buffer_push(&mut target_buffer, target_ty).await?;
                }
                let mut source_suffix = effects.buffer_elements(&source_buffer).await?;
                let mut target_suffix = effects.buffer_elements(&target_buffer).await?;
                #[cursor_loop]
                while let Some(pair) = effects
                    .next_longest(&mut source_suffix, &mut target_suffix, Direction::Backward)
                    .await?
                {
                    let next = match pair {
                        EitherOrBoth::Both(source_ty, target_ty) => {
                            effects.pair(source_ty, target_ty).await?
                        }
                        EitherOrBoth::Left(source_ty) => {
                            effects.pair(source_ty, target_variable).await?
                        }
                        EitherOrBoth::Right(target_ty) => {
                            // The rhs has a required element that the lhs is not guaranteed to
                            // provide, unless the lhs has a dynamic variable-length portion
                            // that can materialize to provide it (for assignability only),
                            // as in `tuple[Any, ...]` matching `tuple[int, int]`.
                            if !facts.assignability(effects.mode().await?)
                                || facts.absent(effects.gradual_element(facts.variable(source)).await?)
                            {
                                return effects.constant(false).await;
                            }
                            effects.pair(source_variable, target_ty).await?
                        }
                    };
                    result = effects.conjoin(result, next).await?;
                    if effects.is_never(result).await? {
                        return Ok(result);
                    }
                }
                // And lastly, the variable-length portions must satisfy the relation.
                if effects.is_never(result).await? {
                    return Ok(result);
                }
                let middle = effects.pair(source_variable, target_variable).await?;
                effects.conjoin(result, middle).await
            }
        }
    }

    /// Compare the fixed ends, pairing any overhanging elements with the provided variable types.
    /// Use raw endpoints: a symbolic pack cannot be prenormalized as a homogeneous `object` segment.
    #[synchronous(check_boundaries_sync)]
    #[capabilities(effects = TupleRelationEffects, facts = TupleRelationFacts)]
    #[passive_values(Direction::Forward, Direction::Backward)]
    pub(in crate::types) async fn check_boundaries_with<
        'c,
        'db: 'c,
        E: TupleRelationEffects<'c, 'db>,
    >(
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        source_variable: Type<'db>,
        target_variable: Type<'db>,
        effects: &E,
        facts: TupleRelationFacts,
    ) -> Result<ConstraintSet<'db, 'c>, E::Error> {
        effects.checkpoint().await?;
        let mut source_prefix = effects.elements(facts.prefix(source)).await?;
        let mut target_prefix = effects.elements(facts.prefix(target)).await?;
        let mut source_suffix = effects.elements(facts.suffix(source)).await?;
        let mut target_suffix = effects.elements(facts.suffix(target)).await?;
        let mut fold = effects.fold_start().await?;
        #[cursor_loop]
        while let Some(pair) = effects
            .next_longest(&mut source_prefix, &mut target_prefix, Direction::Forward)
            .await?
        {
            let next = match pair {
                EitherOrBoth::Right(target_ty)
                    if let VariableSegment::TypeVarTuple(_) = facts.variable(source) =>
                {
                    // The synthesized protocol stands for an arbitrary pack element. Diagnostics
                    // should describe the original tuple types, not this internal placeholder.
                    effects
                        .pair_without_context(source_variable, target_ty)
                        .await?
                }
                EitherOrBoth::Both(source_ty, target_ty) => effects.pair(source_ty, target_ty).await?,
                EitherOrBoth::Left(source_ty) => effects.pair(source_ty, target_variable).await?,
                EitherOrBoth::Right(target_ty) => effects.pair(source_variable, target_ty).await?,
            };
            if let ControlFlow::Break(value) = effects.fold_push(&mut fold, next).await? {
                return Ok(value);
            }
        }
        #[cursor_loop]
        while let Some(pair) = effects
            .next_longest(&mut source_suffix, &mut target_suffix, Direction::Backward)
            .await?
        {
            let next = match pair {
                EitherOrBoth::Right(target_ty)
                    if let VariableSegment::TypeVarTuple(_) = facts.variable(source) =>
                {
                    effects
                        .pair_without_context(source_variable, target_ty)
                        .await?
                }
                EitherOrBoth::Both(source_ty, target_ty) => effects.pair(source_ty, target_ty).await?,
                EitherOrBoth::Left(source_ty) => effects.pair(source_ty, target_variable).await?,
                EitherOrBoth::Right(target_ty) => effects.pair(source_variable, target_ty).await?,
            };
            if let ControlFlow::Break(value) = effects.fold_push(&mut fold, next).await? {
                return Ok(value);
            }
        }
        effects.fold_finish(&mut fold).await
    }

    /// Advances one prenormalized endpoint, retaining its cursor across equivalence children.
    ///
    /// This is used in our subtyping and equivalence checks to handle different tuple types
    /// that represent the same set of runtime tuple values. For instance, the following two tuple
    /// types both represent "a tuple of one or more `int`s":
    ///
    /// ```py
    /// tuple[int, *tuple[int, ...]]
    /// tuple[*tuple[int, ...], int]
    /// ```
    ///
    /// Prenormalization rewrites both types into the former form. We arbitrarily prefer the
    /// elements to appear in the prefix if they can, so we move elements from the beginning of the
    /// suffix, which are equivalent to the variable-length portion, to the end of the prefix.
    ///
    /// Complicating matters is that we don't always want to compare with _this_ tuple's
    /// variable-length portion. (When this tuple's variable-length portion is gradual —
    /// `tuple[Any, ...]` — we compare with the assumption that the `Any` materializes to the other
    /// tuple's variable-length portion.)
    #[synchronous(next_normalized_sync)]
    #[capabilities(effects = TupleRelationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn next_normalized_with<
        'c,
        'db: 'c,
        E: TupleRelationEffects<'c, 'db>,
    >(
        elements: &mut NormalizedElements<'_, 'db>,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        #[cursor_loop]
        while let Some(step) = effects.normalized_step(elements).await? {
            match step {
                NormalizedStep::Yield(element) => return Ok(Some(element)),
                NormalizedStep::Compare { element, variable } => {
                    let equivalent = effects.equivalent(element, variable).await?;
                    if let ControlFlow::Break(result) = effects
                        .normalized_decision(elements, element, equivalent)
                        .await?
                    {
                        return Ok(result);
                    }
                }
            }
        }
        Ok(None)
    }

    /// Advances source then target prenormalized prefixes, preserving longest-zip child order.
    #[synchronous(next_normalized_pair_sync)]
    #[capabilities(effects = TupleRelationEffects, facts = TupleRelationFacts)]
    #[passive_values()]
    pub(in crate::types) async fn next_normalized_pair_with<
        'c,
        'db: 'c,
        E: TupleRelationEffects<'c, 'db>,
    >(
        source: &mut NormalizedElements<'_, 'db>,
        target: &mut NormalizedElements<'_, 'db>,
        effects: &E,
        facts: TupleRelationFacts,
    ) -> Result<Option<EitherOrBoth<Type<'db>, Type<'db>>>, E::Error> {
        let source = effects.next_normalized(source).await?;
        let target = effects.next_normalized(target).await?;
        Ok(facts.pair(source, target))
    }
}

/// Advances one borrowed endpoint without invoking semantic operations.
pub(in crate::types) fn next_element<'db>(
    elements: &mut slice::Iter<'_, Type<'db>>,
    direction: Direction,
) -> Option<Type<'db>> {
    match direction {
        Direction::Forward => elements.next().copied(),
        Direction::Backward => elements.next_back().copied(),
    }
}

/// Advances a fixed-length pair in the ordinary left-to-right zip order.
pub(in crate::types) fn next_zip<'db>(
    source: &mut slice::Iter<'_, Type<'db>>,
    target: &mut slice::Iter<'_, Type<'db>>,
) -> Option<(Type<'db>, Type<'db>)> {
    let source = source.next().copied()?;
    let target = target.next().copied()?;
    Some((source, target))
}

/// Advances both borrowed endpoints and retains an unmatched element from either side.
pub(in crate::types) fn next_longest<'db>(
    source: &mut slice::Iter<'_, Type<'db>>,
    target: &mut slice::Iter<'_, Type<'db>>,
    direction: Direction,
) -> Option<EitherOrBoth<Type<'db>, Type<'db>>> {
    TupleRelationFacts.pair(
        next_element(source, direction),
        next_element(target, direction),
    )
}

/// Borrows the raw endpoints for one independent prefix or suffix normalization scan.
///
/// `Some(variable)` supplies the type used for suffix equivalence checks. `None` uses this
/// tuple's own variable element type.
pub(in crate::types) fn normalized_start<'a, 'db>(
    tuple: &'a VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
    variable: Option<Type<'db>>,
    part: NormalizedPart,
) -> NormalizedElements<'a, 'db> {
    NormalizedElements {
        prefix: tuple.prefix_elements().iter(),
        suffix: tuple.suffix_elements().iter(),
        variable: variable.unwrap_or_else(|| TupleRelationFacts.element(tuple.variable())),
        part,
        scanning: true,
        finished: false,
    }
}

/// Selects a raw endpoint or an equivalence candidate without comparing its type.
pub(in crate::types) fn normalized_step<'db>(
    elements: &mut NormalizedElements<'_, 'db>,
) -> Option<NormalizedStep<'db>> {
    if elements.finished {
        return None;
    }
    if elements.part == NormalizedPart::Prefix
        && let Some(element) = elements.prefix.next().copied()
    {
        return Some(NormalizedStep::Yield(element));
    }
    let Some(element) = elements.suffix.next().copied() else {
        elements.finished = true;
        return None;
    };
    if elements.scanning {
        Some(NormalizedStep::Compare {
            element,
            variable: elements.variable,
        })
    } else {
        Some(NormalizedStep::Yield(element))
    }
}

/// Applies an equivalence result to the retained prefix take-while or suffix skip-while state.
pub(in crate::types) fn normalized_decision<'db>(
    elements: &mut NormalizedElements<'_, 'db>,
    element: Type<'db>,
    equivalent: bool,
) -> ControlFlow<Option<Type<'db>>> {
    match (elements.part, equivalent) {
        (NormalizedPart::Prefix, true) => ControlFlow::Break(Some(element)),
        (NormalizedPart::Prefix, false) => {
            elements.finished = true;
            ControlFlow::Break(None)
        }
        (NormalizedPart::Suffix, true) => ControlFlow::Continue(()),
        (NormalizedPart::Suffix, false) => {
            elements.scanning = false;
            ControlFlow::Break(Some(element))
        }
    }
}

/// Runs tuple comparisons with the ordinary checker and its original diagnostic and guard state.
pub(super) struct OrdinaryTupleRelations<'check, 'a, 'c, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) checker: &'check TypeRelationChecker<'a, 'c, 'db>,
}

impl<'c, 'db: 'c> SynchronousTupleRelationEffects<'c, 'db>
    for OrdinaryTupleRelations<'_, '_, 'c, 'db>
{
    type Error = Infallible;
    type Fold = ConstraintFold<'db, 'c>;
    type Buffer = Vec<Type<'db>>;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn spec(&self, tuple: TupleType<'db>) -> Result<&'db TupleSpec<'db>, Infallible> {
        Ok(tuple.tuple(self.db))
    }
    fn mode(&self) -> Result<(TypeRelation, TypeVarEvaluation), Infallible> {
        Ok((self.checker.relation, self.checker.typevar_evaluation))
    }
    fn has_context(&self) -> Result<bool, Infallible> {
        Ok(self.checker.report_context().is_some())
    }
    fn report_length(&self, source_len: usize, target_len: TupleLength) -> Result<(), Infallible> {
        if let Some(context) = self.checker.report_context() {
            context.push(ErrorContext::TupleLengthMismatch {
                source_len,
                target_len,
            });
        }
        Ok(())
    }
    fn report_element(
        &self,
        source: Type<'db>,
        target: Type<'db>,
        index: usize,
        count: usize,
    ) -> Result<(), Infallible> {
        if let Some(context) = self.checker.report_context() {
            context.push(ErrorContext::TupleElementNotCompatible {
                source,
                target,
                element_index: index,
                element_count: count,
            });
        }
        Ok(())
    }
    fn constant(&self, value: bool) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(ConstraintSet::from_bool(self.checker.constraints, value))
    }
    fn is_never(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Infallible> {
        value.verify_builder(self.checker.constraints);
        Ok(value.is_trivially_never_satisfied())
    }
    fn is_never_satisfied(&self, value: ConstraintSet<'db, 'c>) -> Result<bool, Infallible> {
        Ok(value.is_never_satisfied(self.db, self.checker.env))
    }
    fn pair(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self.checker.check_type_pair(self.db, source, target))
    }
    fn pair_without_context(
        &self,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(self
            .checker
            .without_context_collection(|| self.checker.check_type_pair(self.db, source, target)))
    }
    fn conjoin(
        &self,
        mut left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(left.intersect(self.db, self.checker.constraints, right))
    }
    fn fold_start(&self) -> Result<Self::Fold, Infallible> {
        Ok(ConstraintFold::new(
            self.checker.constraints,
            ConstraintFoldKind::All,
        ))
    }
    fn fold_push(
        &self,
        fold: &mut Self::Fold,
        next: ConstraintSet<'db, 'c>,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, Infallible> {
        Ok(fold.push(next))
    }
    fn fold_finish(&self, fold: &mut Self::Fold) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        Ok(fold.finish_borrowed())
    }
    fn elements<'a>(
        &self,
        elements: &'a [Type<'db>],
    ) -> Result<slice::Iter<'a, Type<'db>>, Infallible> {
        Ok(elements.iter())
    }
    fn next(
        &self,
        elements: &mut slice::Iter<'_, Type<'db>>,
        direction: Direction,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(next_element(elements, direction))
    }
    fn next_zip(
        &self,
        source: &mut slice::Iter<'_, Type<'db>>,
        target: &mut slice::Iter<'_, Type<'db>>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Infallible> {
        Ok(next_zip(source, target))
    }
    fn next_longest(
        &self,
        source: &mut slice::Iter<'_, Type<'db>>,
        target: &mut slice::Iter<'_, Type<'db>>,
        direction: Direction,
    ) -> Result<Option<EitherOrBoth<Type<'db>, Type<'db>>>, Infallible> {
        Ok(next_longest(source, target, direction))
    }
    fn same_pack(
        &self,
        source: BoundTypeVarInstance<'db>,
        target: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        Ok(source.is_same_typevar_as(self.db, target))
    }
    fn inferable(&self, pack: BoundTypeVarInstance<'db>) -> Result<bool, Infallible> {
        Ok(pack.is_inferable(self.db, self.checker.inferable))
    }
    fn gradual_element(
        &self,
        segment: VariableSegment<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(segment.gradual_element_type(self.db, self.checker.env))
    }
    fn empty_protocol(&self) -> Result<Type<'db>, Infallible> {
        Ok(Type::protocol_with_methods(self.db, self.checker.env, []))
    }
    fn pack_fixed(&self, elements: &[Type<'db>]) -> Result<Type<'db>, Infallible> {
        Ok(Type::heterogeneous_tuple(
            self.db,
            self.checker.env,
            elements.iter().copied(),
        ))
    }
    fn pack_variable(
        &self,
        prefix: &[Type<'db>],
        variable: VariableSegment<'db>,
        suffix: &[Type<'db>],
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::tuple(TupleType::new(
            self.db,
            self.checker.env,
            &VariableLengthTuple::mixed(prefix.iter().copied(), variable, suffix.iter().copied()),
        )))
    }
    fn normalized_start<'a>(
        &self,
        tuple: &'a VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        variable: Option<Type<'db>>,
        part: NormalizedPart,
    ) -> Result<NormalizedElements<'a, 'db>, Infallible> {
        Ok(normalized_start(tuple, variable, part))
    }
    fn normalized_step(
        &self,
        elements: &mut NormalizedElements<'_, 'db>,
    ) -> Result<Option<NormalizedStep<'db>>, Infallible> {
        Ok(normalized_step(elements))
    }
    fn normalized_decision(
        &self,
        elements: &mut NormalizedElements<'_, 'db>,
        element: Type<'db>,
        equivalent: bool,
    ) -> Result<ControlFlow<Option<Type<'db>>>, Infallible> {
        Ok(normalized_decision(elements, element, equivalent))
    }
    fn equivalent(&self, element: Type<'db>, variable: Type<'db>) -> Result<bool, Infallible> {
        // Nested element comparisons must retain the outer recursive comparison's guards.
        Ok(self
            .checker
            .as_equivalence_checker()
            .check_type_pair(self.db, element, variable)
            .is_always_satisfied(self.db, self.checker.env))
    }
    fn next_normalized(
        &self,
        elements: &mut NormalizedElements<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        next_normalized_sync(elements, self)
    }
    fn next_normalized_pair(
        &self,
        source: &mut NormalizedElements<'_, 'db>,
        target: &mut NormalizedElements<'_, 'db>,
    ) -> Result<Option<EitherOrBoth<Type<'db>, Type<'db>>>, Infallible> {
        next_normalized_pair_sync(source, target, self, TupleRelationFacts)
    }
    fn buffer_start(&self) -> Result<Self::Buffer, Infallible> {
        Ok(Vec::new())
    }
    fn buffer_push(&self, buffer: &mut Self::Buffer, element: Type<'db>) -> Result<(), Infallible> {
        buffer.push(element);
        Ok(())
    }
    fn buffer_elements<'a>(
        &self,
        buffer: &'a Self::Buffer,
    ) -> Result<slice::Iter<'a, Type<'db>>, Infallible>
    where
        'db: 'a,
    {
        Ok(buffer.iter())
    }
    fn fixed_pair(
        &self,
        source: &[Type<'db>],
        target: &TupleSpec<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        check_fixed_pair_sync(source, target, self, TupleRelationFacts)
    }
    fn variable_pair(
        &self,
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &TupleSpec<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        check_variable_pair_sync(source, target, self, TupleRelationFacts)
    }
    fn boundaries(
        &self,
        source: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        target: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        source_variable: Type<'db>,
        target_variable: Type<'db>,
    ) -> Result<ConstraintSet<'db, 'c>, Infallible> {
        check_boundaries_sync(
            source,
            target,
            source_variable,
            target_variable,
            self,
            TupleRelationFacts,
        )
    }
}
