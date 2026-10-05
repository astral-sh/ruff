//! Controlled unions retain the ordinary insertion, conversion, and normalization order.

use std::alloc::Layout;

use salsa::execution_probe::{RunError, RunResult};

use super::intersection::{buffer_push_quote, buffer_retirement};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::FxOrderSet;
use crate::types::enums::EnumComplement;
use crate::types::relation::source::subtyping_condition;
#[cfg(test)]
use crate::types::set_theoretic::builder::controlled_union::exclusion_observations;
use crate::types::set_theoretic::builder::controlled_union::{
    ExclusionBuffer, GroupedLiteral, UnionEffects, UnionElements, UnionFacts, UnionTypeInsertion,
    add_in_place_impl_with, add_literal_with, add_union_with, finish_insertion_with,
    merge_disjoint_exclusions_with, merge_intersection_exclusions_with,
    merge_truthiness_guarded_pair_with, normalize_enum_complement_unions_with,
    preserve_hashable_union_with, push_type_with, reduce_type_element_with,
    split_truthiness_guarded_intersection_with, try_reduce_with,
};
use crate::types::set_theoretic::builder::intersection_insertion::{Elements, InsertionEffects};
use crate::types::set_theoretic::builder::{
    IntersectionPolarity, IntersectionSimplification, ReduceResult, UnionElement,
};
use crate::types::visitor::runtime::has_alias_like_with;
use crate::types::{
    IntersectionBuilder, IntersectionType, KnownClass, LiteralValueType,
    NegativeIntersectionElements, NominalInstanceType, ProtocolInstanceType, RecursivelyDefined,
    Type, UnionBuilder, UnionType,
};

/// Quotes backing-storage relocation and allocation, including its eventual disposal.
/// Work counts entries and slots; requested bytes describe the allocation independently.
fn buffer_quote<T>(old_capacity: usize, capacity: usize) -> RunResult<(usize, usize)> {
    let bytes = Layout::array::<T>(capacity)
        .map_err(|_| RunError::Contract("union buffer allocation overflow"))?
        .size();
    let work = capacity
        .checked_mul(2)
        .and_then(|work| work.checked_add(old_capacity))
        .and_then(|work| work.checked_add(4))
        .ok_or(RunError::Contract("union buffer work overflow"))?;
    Ok((work, bytes))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Admits a union action's logical work, storage, and callback/result carriers before execution.
    async fn union_local<T, F: FnOnce() -> T>(
        &self,
        work: usize,
        requested_bytes: usize,
        action: F,
    ) -> RunResult<T> {
        self.union_local_quoted(Ok((work, requested_bytes)), action)
            .await
    }

    /// Runs a quoted union action, retaining its captures while quotation failures drain children.
    async fn union_local_quoted<T, F: FnOnce() -> T>(
        &self,
        quote: RunResult<(usize, usize)>,
        action: F,
    ) -> RunResult<T> {
        self.local_quoted_with_fixed_transfers(quote, action).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> UnionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn add_impl(
        &self,
        builder: &mut UnionBuilder<'db>,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> RunResult<()> {
        self.allocate_future(|| add_in_place_impl_with(builder, ty, seen_aliases, UnionFacts, self))
            .await?
            .await
    }

    async fn expand_union(
        &self,
        builder: &mut UnionBuilder<'db>,
        union: UnionType<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> RunResult<()> {
        add_union_with(builder, union, seen_aliases, UnionFacts, self).await
    }

    async fn union_elements(
        &self,
        _builder: &UnionBuilder<'db>,
        union: UnionType<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        self.union_elements_source(union).await
    }

    async fn reserve_union_elements(
        &self,
        builder: &mut UnionBuilder<'db>,
        additional: usize,
    ) -> RunResult<()> {
        let (len, capacity) = self.union_local(2, 0, || builder.elements_storage()).await?;
        let required = Self::checked(len.checked_add(additional))?;
        if required > capacity {
            let (work, bytes) = buffer_quote::<UnionElement<'db>>(capacity, required)?;
            self.union_local(work, bytes, || builder.reserve_elements(additional))
                .await?;
        }
        Ok(())
    }

    async fn union_recursion(
        &self,
        _builder: &UnionBuilder<'db>,
        union: UnionType<'db>,
    ) -> RunResult<RecursivelyDefined> {
        self.union_recursion_source(union).await
    }

    async fn merge_recursion(
        &self,
        builder: &mut UnionBuilder<'db>,
        recursion: RecursivelyDefined,
    ) -> RunResult<()> {
        self.local(2, 0, || builder.merge_recursively_defined(recursion))
            .await
    }

    async fn next_literal_count(
        &self,
        builder: &UnionBuilder<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<usize>> {
        self.local(4, 0, || builder.next_literal_count(cursor))
            .await
    }

    async fn widen_literals(
        &self,
        _builder: &mut UnionBuilder<'db>,
        _seen_aliases: &mut Vec<Type<'db>>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn expand_alias(
        &self,
        _builder: &mut UnionBuilder<'db>,
        _ty: Type<'db>,
        _seen_aliases: &mut Vec<Type<'db>>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn literal(
        &self,
        builder: &mut UnionBuilder<'db>,
        literal: LiteralValueType<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> RunResult<()> {
        add_literal_with(builder, literal, seen_aliases, UnionFacts, self).await
    }

    async fn merge_literal_recursion(
        &self,
        builder: &mut UnionBuilder<'db>,
        literal: LiteralValueType<'db>,
    ) -> RunResult<()> {
        self.local(2, 0, || builder.merge_literal_recursion(literal))
            .await
    }

    async fn grouped_literal(
        &self,
        _builder: &mut UnionBuilder<'db>,
        _literal: LiteralValueType<'db>,
        _group: GroupedLiteral<'db>,
        _seen_aliases: &mut Vec<Type<'db>>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn collapse_to_object(&self, builder: &mut UnionBuilder<'db>) -> RunResult<()> {
        let (len, capacity) = self.union_local(2, 0, || builder.elements_storage()).await?;
        let plain = self
            .union_local(Self::checked(len.checked_add(1))?, 0, || {
                (0..len).all(|index| matches!(builder.element(index), Some(UnionElement::Type(_))))
            })
            .await?;
        if !plain {
            return self.unavailable(SourceOperation::Union).await;
        }
        if capacity == 0 {
            let (work, bytes) = buffer_quote::<UnionElement<'db>>(0, 1)?;
            self.union_local(work, bytes, || builder.reserve_elements(1))
                .await?;
        }
        let work = Self::checked(len.checked_add(4))?;
        self.union_local(work, size_of::<UnionElement<'db>>(), || {
            builder.collapse_to_object()
        })
        .await
    }

    async fn push_type(
        &self,
        builder: &mut UnionBuilder<'db>,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> RunResult<()> {
        push_type_with(builder, ty, seen_aliases, UnionFacts, self).await
    }

    async fn next_element(
        &self,
        builder: &UnionBuilder<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<usize>> {
        self.local(2, 0, || builder.next_element(cursor)).await
    }

    async fn reduce_element(
        &self,
        builder: &mut UnionBuilder<'db>,
        index: usize,
        insertion: &mut UnionTypeInsertion<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> RunResult<bool> {
        reduce_type_element_with(builder, index, insertion, seen_aliases, UnionFacts, self).await
    }

    async fn plain_element(
        &self,
        builder: &UnionBuilder<'db>,
        index: usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.union_local(4, 0, || match builder.element(index) {
            Some(UnionElement::Type(ty)) => Some(*ty),
            _ => None,
        })
        .await
    }

    async fn reduce_member(
        &self,
        builder: &mut UnionBuilder<'db>,
        index: usize,
        other: Type<'db>,
    ) -> RunResult<ReduceResult<'db>> {
        try_reduce_with(builder, index, other, self).await
    }

    async fn reduce_literal_group(
        &self,
        _builder: &mut UnionBuilder<'db>,
        _index: usize,
        _other: Type<'db>,
    ) -> RunResult<ReduceResult<'db>> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn same_type(&self, first: Type<'db>, second: Type<'db>) -> RunResult<bool> {
        // Debug todo labels are compared as strings; their byte elements require a variable
        // comparison bound even though Type itself has a fixed representation.
        let work = self
            .union_local(2, 0, || {
                first
                    .inline_payload_bytes()
                    .checked_add(second.inline_payload_bytes())
                    .and_then(|work| work.checked_add(2))
            })
            .await?;
        self.union_local(Self::checked(work)?, 0, || first == second)
            .await
    }

    async fn preserve_hashable_union(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<bool> {
        preserve_hashable_union_with(builder, first, second, self).await
    }

    async fn protocol_is_hashable(
        &self,
        _builder: &UnionBuilder<'db>,
        _protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn nominal_is_final(
        &self,
        _builder: &UnionBuilder<'db>,
        _instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn known_instance(
        &self,
        builder: &UnionBuilder<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(builder.environment()).await?;
        self.access.known_class_instance(program, class).await
    }

    async fn merge_truthiness_guarded_pair(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        merge_truthiness_guarded_pair_with(builder, first, second, self).await
    }

    async fn split_truthiness_guarded_intersection(
        &self,
        builder: &UnionBuilder<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Option<(Type<'db>, Type<'db>)>> {
        split_truthiness_guarded_intersection_with(builder, intersection, self).await
    }

    async fn negate_guard(
        &self,
        builder: &UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.negate_type(builder.environment(), ty).await
    }

    async fn intersection_negatives(
        &self,
        _builder: &UnionBuilder<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<&'db NegativeIntersectionElements<'db>> {
        self.field(
            intersection
                .field_requests(self.access.endpoint().field_request_context())
                .negative(),
        )
        .await
    }

    async fn contains_truthiness_guard(
        &self,
        negative: &NegativeIntersectionElements<'db>,
        always_truthy: bool,
    ) -> RunResult<bool> {
        let len = self.local(1, 0, || negative.len()).await?;
        let work = Self::checked(
            len.checked_mul(size_of::<Type<'db>>() + 1)
                .and_then(|work| work.checked_add(3)),
        )?;
        // These fixed markers need only discriminant comparisons. Scanning avoids relying
        // on the retained capacity of the negative elements' hash table.
        self.local(work, 0, || {
            negative.iter().any(|ty| {
                matches!(
                    (always_truthy, ty),
                    (true, Type::AlwaysTruthy) | (false, Type::AlwaysFalsy)
                )
            })
        })
        .await
    }

    async fn new_guard_core(
        &self,
        builder: &UnionBuilder<'db>,
    ) -> RunResult<IntersectionBuilder<'db>> {
        #[cfg(test)]
        exclusion_observations::reconstructing(self.db());
        self.new_intersection(builder.environment()).await
    }

    async fn positive_guard_elements(
        &self,
        _builder: &UnionBuilder<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        InsertionEffects::positive_elements(self, intersection).await
    }

    async fn negative_guard_elements(
        &self,
        negative: &'db NegativeIntersectionElements<'db>,
    ) -> RunResult<Elements<'db>> {
        self.local(size_of::<Elements<'db>>() + 1, 0, || {
            Elements::Negative(negative.iter())
        })
        .await
    }

    async fn next_guard_element(
        &self,
        elements: &mut Elements<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        InsertionEffects::next_element(self, elements).await
    }

    async fn add_guard_core_positive(
        &self,
        core: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.intersection_add_positive(core, ty).await
    }

    async fn add_guard_core_negative(
        &self,
        core: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.intersection_add_negative(core, ty).await
    }

    async fn build_guard_core(&self, core: &mut IntersectionBuilder<'db>) -> RunResult<Type<'db>> {
        self.intersection_build(core).await
    }

    async fn merge_truthiness_guarded_cores(
        &self,
        _builder: &UnionBuilder<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
        _first_parts: (Type<'db>, Type<'db>),
        _second_parts: (Type<'db>, Type<'db>),
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn contains_nested_alias(
        &self,
        builder: &UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.environment_program(builder.environment()).await?;
        has_alias_like_with(self.db(), self.access.endpoint(), ty, self).await
    }

    async fn merge_disjoint_exclusions(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        merge_disjoint_exclusions_with(builder, first, second, self).await
    }

    async fn merge_intersection_exclusions(
        &self,
        builder: &UnionBuilder<'db>,
        first: IntersectionType<'db>,
        second: IntersectionType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        merge_intersection_exclusions_with(builder, first, second, self).await
    }

    async fn intersection_positives(
        &self,
        _builder: &UnionBuilder<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<&'db FxOrderSet<Type<'db>>> {
        self.field(
            intersection
                .field_requests(self.access.endpoint().field_request_context())
                .positive(),
        )
        .await
    }

    async fn same_positive_sets(
        &self,
        first: &'db FxOrderSet<Type<'db>>,
        second: &'db FxOrderSet<Type<'db>>,
    ) -> RunResult<bool> {
        if !self.local(2, 0, || first.len() == second.len()).await? {
            return Ok(false);
        }
        let mut first_elements = UnionEffects::retained_positive_elements(self, first).await?;
        while let Some(first) = UnionEffects::next_guard_element(self, &mut first_elements).await? {
            let mut second_elements =
                UnionEffects::retained_positive_elements(self, second).await?;
            let mut found = false;
            while let Some(second) =
                UnionEffects::next_guard_element(self, &mut second_elements).await?
            {
                if UnionEffects::same_type(self, first, second).await? {
                    found = true;
                    break;
                }
            }
            if !found {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn retained_positive_elements(
        &self,
        positive: &'db FxOrderSet<Type<'db>>,
    ) -> RunResult<Elements<'db>> {
        self.local(size_of::<Elements<'db>>() + 1, 0, || {
            Elements::Positive(positive.iter())
        })
        .await
    }

    async fn contains_exclusion(
        &self,
        negative: &'db NegativeIntersectionElements<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        // Dense iteration does not depend on the retained hash-table capacity. Each exact
        // comparison admits both types' inline payloads, including unsuccessful searches.
        let mut negative = UnionEffects::negative_guard_elements(self, negative).await?;
        while let Some(exclusion) = UnionEffects::next_guard_element(self, &mut negative).await? {
            if UnionEffects::same_type(self, ty, exclusion).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn has_all_exclusions(
        &self,
        buffer: &ExclusionBuffer<'db>,
        negative: &NegativeIntersectionElements<'db>,
    ) -> RunResult<bool> {
        self.local(2, 0, || buffer.len() == negative.len()).await
    }

    async fn new_exclusion_buffer(&self) -> RunResult<ExclusionBuffer<'db>> {
        self.local(
            size_of::<ExclusionBuffer<'db>>() * 2 + 1,
            0,
            ExclusionBuffer::new,
        )
        .await
    }

    async fn push_exclusion(
        &self,
        buffer: &mut ExclusionBuffer<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let storage = self.local(1, 0, || buffer.storage()).await?;
        let quote = buffer_push_quote::<Type<'db>>(storage).ok_or(RunError::Contract(
            "union exclusion buffer quotation overflow",
        ))?;
        self.local(quote.work, quote.bytes, || buffer.push(ty))
            .await?;
        #[cfg(test)]
        exclusion_observations::pushed(self.db(), buffer.storage());
        Ok(())
    }

    async fn exclusion_buffer_is_empty(&self, buffer: &ExclusionBuffer<'db>) -> RunResult<bool> {
        #[cfg(test)]
        exclusion_observations::partitioned(self.db());
        self.local(1, 0, || buffer.len() == 0).await
    }

    async fn next_exclusion(
        &self,
        buffer: &ExclusionBuffer<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(size_of::<Type<'db>>() + 2, 0, || buffer.next(cursor))
            .await
    }

    async fn finish_exclusion_buffer(&self, buffer: ExclusionBuffer<'db>) -> RunResult<()> {
        let storage = self.local(1, 0, || buffer.storage()).await?;
        self.work(Self::checked(buffer_retirement::<Type<'db>>(storage))?)
            .await?;
        // Keep the owner outside the admission closure. Each push prepaid cleanup if a
        // semantic child or the disposal admission refuses after a partition has spilled.
        drop(buffer);
        Ok(())
    }

    async fn simplify_exclusion_pair(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<IntersectionSimplification> {
        self.environment_program(builder.environment()).await?;
        #[cfg(test)]
        exclusion_observations::pair(first, second);
        InsertionEffects::simplify_pair(self, first, second, IntersectionPolarity::Positive).await
    }

    async fn redundant(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<bool> {
        self.environment_program(builder.environment()).await?;
        self.access.is_redundant_with(first, second).await
    }

    async fn negation_subtype_cached(
        &self,
        builder: &UnionBuilder<'db>,
        insertion: &mut UnionTypeInsertion<'db>,
        target: Type<'db>,
    ) -> RunResult<bool> {
        let ty = self
            .union_local(2, 0, || insertion.ty())
            .await?;
        if matches!(ty, Type::Intersection(_)) {
            return self.unavailable(SourceOperation::Union).await;
        }
        let cached = self
            .union_local(2, 0, || *insertion.negation_cache())
            .await?;
        let negated = if let Some(cached) = cached {
            cached
        } else {
            let negated = self.negate_type(builder.environment(), ty).await?;
            self.union_local(2, size_of::<Option<Type<'db>>>(), || {
                *insertion.negation_cache() = Some(negated)
            })
            .await?;
            negated
        };
        subtyping_condition(self.db(), builder.environment(), negated, target, self).await
    }

    async fn defer_removal(
        &self,
        insertion: &mut UnionTypeInsertion<'db>,
        index: usize,
    ) -> RunResult<()> {
        let (len, capacity, _) = self
            .union_local(3, 0, || insertion.removals_storage())
            .await?;
        if len == capacity {
            let new_capacity = Self::checked(len.checked_add(1))?;
            let (work, bytes) = buffer_quote::<usize>(len, new_capacity)?;
            self.union_local(work, bytes, || {
                insertion.reserve_removals(new_capacity - len)
            })
            .await?;
        }
        self.union_local(3, size_of::<usize>(), || insertion.defer_removal(index))
            .await
    }

    async fn set_incoming(
        &self,
        insertion: &mut UnionTypeInsertion<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.union_local(2, size_of::<Type<'db>>(), || insertion.set_type(ty))
            .await
    }

    async fn take_removals(
        &self,
        insertion: &mut UnionTypeInsertion<'db>,
    ) -> RunResult<smallvec::IntoIter<[usize; 2]>> {
        self.union_local(4, size_of::<smallvec::SmallVec<[usize; 2]>>(), || {
            insertion.take_removals()
        })
        .await
    }

    async fn finish_insertion(
        &self,
        builder: &mut UnionBuilder<'db>,
        insertion: UnionTypeInsertion<'db>,
    ) -> RunResult<()> {
        finish_insertion_with(builder, insertion, UnionFacts, self).await
    }

    async fn next_removal(
        &self,
        removals: &mut smallvec::IntoIter<[usize; 2]>,
    ) -> RunResult<Option<usize>> {
        self.union_local(3, 0, || removals.next()).await
    }

    async fn next_removal_back(
        &self,
        removals: &mut smallvec::IntoIter<[usize; 2]>,
    ) -> RunResult<Option<usize>> {
        self.union_local(3, 0, || removals.next_back()).await
    }

    async fn replace_type(
        &self,
        builder: &mut UnionBuilder<'db>,
        index: usize,
        ty: Type<'db>,
    ) -> RunResult<()> {
        if UnionEffects::plain_element(self, builder, index)
            .await?
            .is_none()
        {
            return self.unavailable(SourceOperation::Union).await;
        }
        self.union_local(3, size_of::<UnionElement<'db>>(), || {
            builder.replace_type(index, ty)
        })
        .await
    }

    async fn remove_type(&self, builder: &mut UnionBuilder<'db>, index: usize) -> RunResult<()> {
        if UnionEffects::plain_element(self, builder, index)
            .await?
            .is_none()
        {
            return self.unavailable(SourceOperation::Union).await;
        }
        self.union_local(4, 2 * size_of::<UnionElement<'db>>(), || {
            builder.remove_type(index)
        })
        .await
    }

    async fn append_type(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        let (len, capacity) = self.union_local(2, 0, || builder.elements_storage()).await?;
        if len == capacity {
            let new_capacity = Self::checked(len.checked_add(1))?;
            // Growth admits copying retained entries and disposal of the new allocation before
            // reserve can change the owner. Only plain types can be inserted by this provider.
            let (work, bytes) = buffer_quote::<UnionElement<'db>>(len, new_capacity)?;
            self.union_local(work, bytes, || builder.reserve_elements(new_capacity - len))
                .await?;
        }
        self.union_local(3, size_of::<UnionElement<'db>>(), || {
            builder.append_type(ty);
        })
        .await
    }

    async fn next_type_count(
        &self,
        builder: &UnionBuilder<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<usize>> {
        self.local(2, 0, || builder.next_type_count(cursor)).await
    }

    async fn allocate_types(&self, count: usize) -> RunResult<Vec<Type<'db>>> {
        // The capacity also pays for dropping converted entries at any later suspension point.
        let (work, bytes) = buffer_quote::<Type<'db>>(0, count)?;
        self.union_local(work, bytes, || Vec::with_capacity(count))
            .await
    }

    async fn take_elements(
        &self,
        builder: &mut UnionBuilder<'db>,
    ) -> RunResult<UnionElements<'db>> {
        self.union_local(4, size_of::<Vec<UnionElement<'db>>>(), || {
            builder.take_elements()
        })
        .await
    }

    async fn next_owned_element(
        &self,
        elements: &mut UnionElements<'db>,
    ) -> RunResult<Option<UnionElement<'db>>> {
        self.union_local(3, 0, || elements.next()).await
    }

    async fn finish_elements(&self, elements: UnionElements<'db>) -> RunResult<()> {
        // Allocation and insertion prepaid disposal. Retain the iterator in the local action
        // until admission completes, including while pending children drain after a refusal.
        self.union_local(1, 0, || drop(elements)).await
    }

    async fn append_converted(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()> {
        self.union_local(3, size_of::<Type<'db>>(), || {
            if types.len() == types.capacity() {
                return Err(RunError::Contract("union conversion exceeded its type count"));
            }
            types.push(ty);
            Ok(())
        })
        .await?
    }

    async fn convert_literals(
        &self,
        _builder: &UnionBuilder<'db>,
        _element: UnionElement<'db>,
        _types: &mut Vec<Type<'db>>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn normalize(
        &self,
        builder: &UnionBuilder<'db>,
        types: &mut Vec<Type<'db>>,
    ) -> RunResult<bool> {
        normalize_enum_complement_unions_with(builder, types, self).await
    }

    async fn next_type(
        &self,
        types: &[Type<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<(usize, Type<'db>)>> {
        self.local(2, 0, || {
            let result = types.get(*cursor).copied().map(|ty| (*cursor, ty));
            if result.is_some() {
                *cursor += 1;
            }
            result
        })
        .await
    }

    async fn normalize_complement(
        &self,
        _builder: &UnionBuilder<'db>,
        _types: &mut Vec<Type<'db>>,
        _index: usize,
        _complement: EnumComplement<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn rebuild(
        &self,
        _builder: UnionBuilder<'db>,
        _types: Vec<Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::Union).await
    }

    async fn intern(
        &self,
        builder: UnionBuilder<'db>,
        types: Vec<Type<'db>>,
    ) -> RunResult<Type<'db>> {
        let recursively_defined = self.local(1, 0, || builder.recursion_state()).await?;
        let quote = buffer_quote::<Type<'db>>(types.capacity(), types.len());
        let elements = self
            .union_local_quoted(quote, || types.into_boxed_slice())
            .await?;
        Ok(Type::Union(
            self.access
                .intern_union(elements, recursively_defined)
                .await?,
        ))
    }
}
