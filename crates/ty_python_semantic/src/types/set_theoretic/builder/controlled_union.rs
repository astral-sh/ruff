//! Union insertion and finalization order shared by ordinary and controlled inference.

use std::convert::Infallible;

use smallvec::SmallVec;
use ty_mapping_probe_macros::shared_semantic_family;

use super::intersection_insertion::Elements;
use super::{
    IntersectionBuilder, IntersectionPolarity, IntersectionSimplification,
    MAX_NON_RECURSIVE_UNION_LITERALS, MAX_RECURSIVE_UNION_LITERALS, ReduceResult, UnionBuilder,
    UnionElement, intersection_assembly, simplify_intersection_pair,
};
use crate::FxOrderSet;
use crate::types::enums::EnumComplement;
use crate::types::literal::IntLiteralType;
use crate::types::visitor::any_over_type;
use crate::types::{
    BytesLiteralType, EnumLiteralType, IntersectionType, KnownClass, KnownInstanceType,
    LiteralValueType, LiteralValueTypeKind, NegativeIntersectionElements, NominalInstanceType,
    ProtocolInstanceType, RecursivelyDefined, StringLiteralType, Type, UnionType,
};

pub(in crate::types) struct UnionFacts;
pub(super) struct OrdinaryUnionEffects;

pub(in crate::types) struct ExclusionBuffer<'db> {
    elements: SmallVec<[Type<'db>; 2]>,
}

impl<'db> ExclusionBuffer<'db> {
    pub(in crate::types) fn new() -> Self {
        #[cfg(test)]
        exclusion_observations::entered();
        Self {
            elements: SmallVec::new(),
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn storage(&self) -> (usize, usize, bool) {
        (
            self.elements.len(),
            self.elements.capacity(),
            self.elements.spilled(),
        )
    }

    pub(in crate::types) fn len(&self) -> usize {
        self.elements.len()
    }

    pub(in crate::types) fn push(&mut self, ty: Type<'db>) {
        self.elements.push(ty);
    }

    pub(in crate::types) fn next(&self, cursor: &mut usize) -> Option<Type<'db>> {
        let next = self.elements.get(*cursor).copied();
        if next.is_some() {
            *cursor += 1;
        }
        next
    }
}

#[cfg(test)]
impl Drop for ExclusionBuffer<'_> {
    fn drop(&mut self) {
        exclusion_observations::dropped(self.storage());
    }
}

#[cfg(test)]
pub(in crate::types) mod exclusion_observations {
    use std::cell::{Cell, RefCell};
    use std::hash::{Hash, Hasher};

    use rustc_hash::FxHasher;

    use crate::Db;
    use crate::types::Type;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::types) enum Stage {
        AfterPush(usize),
        BeforeReconstruction,
    }

    #[derive(Clone, Default)]
    pub(in crate::types) struct Progress {
        pub live_buffers: usize,
        pub entered_buffers: usize,
        pub dropped_buffers: usize,
        pub spilled_buffers: usize,
        pub dropped_spilled_buffers: usize,
        pub dropped_capacity: usize,
        pub pushes: usize,
        pub peak_len: usize,
        pub peak_capacity: usize,
        pub partition_remaining: Option<usize>,
        pub push_remaining: Option<usize>,
        pub pairs: Vec<(u64, u64)>,
    }

    thread_local! {
        static PROGRESS: RefCell<Progress> = RefCell::default();
        static CANCEL: Cell<Option<Stage>> = const { Cell::new(None) };
    }

    pub(in crate::types) fn reset(cancel: Option<Stage>) {
        PROGRESS.with_borrow_mut(|progress| {
            assert_eq!(progress.live_buffers, 0);
            *progress = Progress::default();
        });
        CANCEL.set(cancel);
    }

    pub(in crate::types) fn progress() -> Progress {
        PROGRESS.with_borrow(Clone::clone)
    }

    pub(in crate::types) fn type_key(ty: Type<'_>) -> u64 {
        let mut hasher = FxHasher::default();
        ty.hash(&mut hasher);
        hasher.finish()
    }

    pub(super) fn entered() {
        PROGRESS.with_borrow_mut(|progress| {
            progress.live_buffers += 1;
            progress.entered_buffers += 1;
        });
    }

    pub(super) fn dropped((_, capacity, spilled): (usize, usize, bool)) {
        PROGRESS.with_borrow_mut(|progress| {
            progress.live_buffers -= 1;
            progress.dropped_buffers += 1;
            if spilled {
                progress.dropped_spilled_buffers += 1;
                progress.dropped_capacity += capacity;
            }
        });
    }

    fn cancel(db: &dyn Db, stage: Stage) {
        if CANCEL.get() == Some(stage) {
            CANCEL.set(None);
            db.cancellation_token().cancel();
        }
    }

    pub(in crate::types) fn pushed(db: &dyn Db, (len, capacity, spilled): (usize, usize, bool)) {
        let pushes = PROGRESS.with_borrow_mut(|progress| {
            progress.pushes += 1;
            progress.spilled_buffers += usize::from(spilled && len == 3);
            progress.peak_len = progress.peak_len.max(len);
            progress.peak_capacity = progress.peak_capacity.max(capacity);
            progress.push_remaining = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
            progress.pushes
        });
        cancel(db, Stage::AfterPush(pushes));
    }

    pub(in crate::types) fn partitioned(db: &dyn Db) {
        PROGRESS.with_borrow_mut(|progress| {
            if progress.partition_remaining.is_none() {
                progress.partition_remaining =
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
            }
        });
    }

    pub(in crate::types) fn reconstructing(db: &dyn Db) {
        if PROGRESS.with_borrow(|progress| progress.live_buffers != 0) {
            cancel(db, Stage::BeforeReconstruction);
        }
    }

    pub(in crate::types) fn pair(first: Type<'_>, second: Type<'_>) {
        PROGRESS
            .with_borrow_mut(|progress| progress.pairs.push((type_key(first), type_key(second))));
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) enum GroupedLiteral<'db> {
    String(StringLiteralType<'db>),
    Bytes(BytesLiteralType<'db>),
    Int(IntLiteralType),
    Enum(EnumLiteralType<'db>),
}

pub(in crate::types) struct UnionTypeInsertion<'db> {
    ty: Type<'db>,
    should_simplify_full: bool,
    ty_negated: Option<Type<'db>>,
    to_remove: SmallVec<[usize; 2]>,
}

impl<'db> UnionTypeInsertion<'db> {
    pub(in crate::types) fn ty(&self) -> Type<'db> {
        self.ty
    }

    pub(in crate::types) fn negation_cache(&mut self) -> &mut Option<Type<'db>> {
        &mut self.ty_negated
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn removals_storage(&self) -> (usize, usize, bool) {
        (
            self.to_remove.len(),
            self.to_remove.capacity(),
            self.to_remove.spilled(),
        )
    }

    #[cfg(feature = "experimental-analysis")]
    pub(in crate::types) fn reserve_removals(&mut self, additional: usize) {
        self.to_remove.reserve_exact(additional);
    }

    pub(in crate::types) fn defer_removal(&mut self, index: usize) {
        self.to_remove.push(index);
    }

    pub(in crate::types) fn set_type(&mut self, ty: Type<'db>) {
        self.ty = ty;
    }

    pub(in crate::types) fn take_removals(&mut self) -> smallvec::IntoIter<[usize; 2]> {
        std::mem::take(&mut self.to_remove).into_iter()
    }
}

pub(in crate::types) struct UnionElements<'db> {
    inner: std::vec::IntoIter<UnionElement<'db>>,
}

impl<'db> UnionElements<'db> {
    pub(super) fn new(elements: Vec<UnionElement<'db>>) -> Self {
        Self {
            inner: elements.into_iter(),
        }
    }

    pub(in crate::types) fn next(&mut self) -> Option<UnionElement<'db>> {
        self.inner.next()
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousUnionEffects)]
    pub(in crate::types) trait UnionEffects<'db> {
        type Error;
        #[operation(source)]
        async fn add_impl(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn expand_union(&self, builder: &mut UnionBuilder<'db>, union: UnionType<'db>, seen_aliases: &mut Vec<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_elements(&self, builder: &UnionBuilder<'db>, union: UnionType<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        async fn reserve_union_elements(&self, builder: &mut UnionBuilder<'db>, additional: usize) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn union_recursion(&self, builder: &UnionBuilder<'db>, union: UnionType<'db>) -> Result<RecursivelyDefined, Self::Error>;
        #[operation(local)]
        async fn merge_recursion(&self, builder: &mut UnionBuilder<'db>, recursion: RecursivelyDefined) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_literal_count(&self, builder: &UnionBuilder<'db>, cursor: &mut usize) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn widen_literals(&self, builder: &mut UnionBuilder<'db>, seen_aliases: &mut Vec<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn expand_alias(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn literal(&self, builder: &mut UnionBuilder<'db>, literal: LiteralValueType<'db>, seen_aliases: &mut Vec<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn merge_literal_recursion(&self, builder: &mut UnionBuilder<'db>, literal: LiteralValueType<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn grouped_literal(&self, builder: &mut UnionBuilder<'db>, literal: LiteralValueType<'db>, group: GroupedLiteral<'db>, seen_aliases: &mut Vec<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn collapse_to_object(&self, builder: &mut UnionBuilder<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn push_type(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element(&self, builder: &UnionBuilder<'db>, cursor: &mut usize) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn reduce_element(&self, builder: &mut UnionBuilder<'db>, index: usize, insertion: &mut UnionTypeInsertion<'db>, seen_aliases: &mut Vec<Type<'db>>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn plain_element(&self, builder: &UnionBuilder<'db>, index: usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn reduce_member(&self, builder: &mut UnionBuilder<'db>, index: usize, other: Type<'db>) -> Result<ReduceResult<'db>, Self::Error>;
        #[operation(source)]
        async fn reduce_literal_group(&self, builder: &mut UnionBuilder<'db>, index: usize, other: Type<'db>) -> Result<ReduceResult<'db>, Self::Error>;
        #[operation(local)]
        async fn same_type(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn preserve_hashable_union(&self, builder: &UnionBuilder<'db>, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn protocol_is_hashable(&self, builder: &UnionBuilder<'db>, protocol: ProtocolInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn nominal_is_final(&self, builder: &UnionBuilder<'db>, instance: NominalInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn known_instance(&self, builder: &UnionBuilder<'db>, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn merge_truthiness_guarded_pair(&self, builder: &UnionBuilder<'db>, first: Type<'db>, second: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn split_truthiness_guarded_intersection(&self, builder: &UnionBuilder<'db>, intersection: IntersectionType<'db>) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error>;
        #[operation(source)]
        async fn negate_guard(&self, builder: &UnionBuilder<'db>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn intersection_negatives(&self, builder: &UnionBuilder<'db>, intersection: IntersectionType<'db>) -> Result<&'db NegativeIntersectionElements<'db>, Self::Error>;
        #[operation(local)]
        async fn contains_truthiness_guard(&self, negative: &NegativeIntersectionElements<'db>, always_truthy: bool) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn new_guard_core(&self, builder: &UnionBuilder<'db>) -> Result<IntersectionBuilder<'db>, Self::Error>;
        #[operation(source)]
        async fn positive_guard_elements(&self, builder: &UnionBuilder<'db>, intersection: IntersectionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(local)]
        async fn negative_guard_elements(&self, negative: &'db NegativeIntersectionElements<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_guard_element(&self, elements: &mut Elements<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn add_guard_core_positive(&self, core: &mut IntersectionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn add_guard_core_negative(&self, core: &mut IntersectionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn build_guard_core(&self, core: &mut IntersectionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn merge_truthiness_guarded_cores(&self, builder: &UnionBuilder<'db>, first: Type<'db>, second: Type<'db>, first_parts: (Type<'db>, Type<'db>), second_parts: (Type<'db>, Type<'db>)) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn contains_nested_alias(&self, builder: &UnionBuilder<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn merge_disjoint_exclusions(&self, builder: &UnionBuilder<'db>, first: Type<'db>, second: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn merge_intersection_exclusions(&self, builder: &UnionBuilder<'db>, first: IntersectionType<'db>, second: IntersectionType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn intersection_positives(&self, builder: &UnionBuilder<'db>, intersection: IntersectionType<'db>) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn same_positive_sets(&self, first: &'db FxOrderSet<Type<'db>>, second: &'db FxOrderSet<Type<'db>>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn retained_positive_elements(&self, positive: &'db FxOrderSet<Type<'db>>) -> Result<Elements<'db>, Self::Error>;
        #[operation(local)]
        async fn contains_exclusion(&self, negative: &'db NegativeIntersectionElements<'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn has_all_exclusions(&self, buffer: &ExclusionBuffer<'db>, negative: &NegativeIntersectionElements<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn new_exclusion_buffer(&self) -> Result<ExclusionBuffer<'db>, Self::Error>;
        #[operation(local)]
        async fn push_exclusion(&self, buffer: &mut ExclusionBuffer<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn exclusion_buffer_is_empty(&self, buffer: &ExclusionBuffer<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_exclusion(&self, buffer: &ExclusionBuffer<'db>, cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn finish_exclusion_buffer(&self, buffer: ExclusionBuffer<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn simplify_exclusion_pair(&self, builder: &UnionBuilder<'db>, first: Type<'db>, second: Type<'db>) -> Result<IntersectionSimplification, Self::Error>;
        #[operation(source)]
        async fn redundant(&self, builder: &UnionBuilder<'db>, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn negation_subtype_cached(&self, builder: &UnionBuilder<'db>, insertion: &mut UnionTypeInsertion<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn defer_removal(&self, insertion: &mut UnionTypeInsertion<'db>, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn set_incoming(&self, insertion: &mut UnionTypeInsertion<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn take_removals(&self, insertion: &mut UnionTypeInsertion<'db>) -> Result<smallvec::IntoIter<[usize; 2]>, Self::Error>;
        #[operation(source)]
        async fn finish_insertion(&self, builder: &mut UnionBuilder<'db>, insertion: UnionTypeInsertion<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_removal(&self, removals: &mut smallvec::IntoIter<[usize; 2]>) -> Result<Option<usize>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_removal_back(&self, removals: &mut smallvec::IntoIter<[usize; 2]>) -> Result<Option<usize>, Self::Error>;
        #[operation(local)]
        async fn replace_type(&self, builder: &mut UnionBuilder<'db>, index: usize, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn remove_type(&self, builder: &mut UnionBuilder<'db>, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn append_type(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type_count(&self, builder: &UnionBuilder<'db>, cursor: &mut usize) -> Result<Option<usize>, Self::Error>;
        #[operation(local)]
        async fn allocate_types(&self, count: usize) -> Result<Vec<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn take_elements(&self, builder: &mut UnionBuilder<'db>) -> Result<UnionElements<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_owned_element(&self, elements: &mut UnionElements<'db>) -> Result<Option<UnionElement<'db>>, Self::Error>;
        #[operation(local)]
        async fn finish_elements(&self, elements: UnionElements<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn append_converted(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn convert_literals(&self, builder: &UnionBuilder<'db>, element: UnionElement<'db>, types: &mut Vec<Type<'db>>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn normalize(&self, builder: &UnionBuilder<'db>, types: &mut Vec<Type<'db>>) -> Result<bool, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_type(&self, types: &[Type<'db>], cursor: &mut usize) -> Result<Option<(usize, Type<'db>)>, Self::Error>;
        #[operation(source)]
        async fn normalize_complement(&self, builder: &UnionBuilder<'db>, types: &mut Vec<Type<'db>>, index: usize, complement: EnumComplement<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn rebuild(&self, builder: UnionBuilder<'db>, types: Vec<Type<'db>>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn intern(&self, builder: UnionBuilder<'db>, types: Vec<Type<'db>>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl UnionFacts {
        fn literal_kind<'db>(&self, literal: LiteralValueType<'db>) -> LiteralValueTypeKind<'db> {
            literal.kind()
        }
        fn assert_not_recursive_var(&self, ty: Type<'_>) {
            ty.assert_not_recursive_var();
        }
        fn empty_aliases<'db>(&self) -> Vec<Type<'db>> {
            Vec::new()
        }
        fn unpack_aliases(&self, builder: &UnionBuilder<'_>) -> bool {
            builder.unpack_aliases
        }
        fn cycle_recovery(&self, builder: &UnionBuilder<'_>) -> bool {
            builder.cycle_recovery
        }
        fn recursive_union(&self, builder: &UnionBuilder<'_>) -> bool {
            builder.recursively_defined.is_yes()
        }
        fn should_widen_union_literals(&self, builder: &UnionBuilder<'_>, literals: usize) -> bool {
            if builder.recursively_defined.is_yes() && builder.cycle_recovery {
                literals >= MAX_RECURSIVE_UNION_LITERALS
            } else {
                literals >= MAX_NON_RECURSIVE_UNION_LITERALS
            }
        }
        fn incoming<'db>(&self, insertion: &UnionTypeInsertion<'db>) -> Type<'db> {
            insertion.ty
        }
        fn should_simplify_full(&self, insertion: &UnionTypeInsertion<'_>) -> bool {
            insertion.should_simplify_full
        }
        fn object<'db>(&self) -> Type<'db> {
            Type::object()
        }
        fn is_alias_like(&self, ty: Type<'_>) -> bool {
            ty.is_alias_like()
        }
        fn is_typed_dict(&self, ty: Type<'_>) -> bool {
            ty.is_typed_dict()
        }
        fn opposite_ranges(&self, first: Type<'_>, second: Type<'_>) -> bool {
            matches!(
                (first, second),
                (
                    Type::KnownInstance(KnownInstanceType::Range { is_non_empty: left }),
                    Type::KnownInstance(KnownInstanceType::Range { is_non_empty: right }),
                ) if left != right
            )
        }
        fn opposite_bools(&self, first: Type<'_>, second: Type<'_>) -> bool {
            let second = second.as_literal_value_kind();
            let first = first.as_literal_value_kind().and_then(|kind| match kind {
                LiteralValueTypeKind::Bool(value) => Some(LiteralValueTypeKind::Bool(!value)),
                _ => None,
            });
            second.zip(first).is_some_and(|(second, opposite)| second == opposite)
        }
        fn collapse_object(&self, builder: &UnionBuilder<'_>, ty: Type<'_>) -> bool {
            ty.is_object() && !builder.cycle_recovery
        }
        fn insertion<'db>(&self, builder: &UnionBuilder<'db>, ty: Type<'db>) -> UnionTypeInsertion<'db> {
            // If an alias gets here, it means we aren't unpacking aliases, and we also
            // shouldn't try to simplify aliases out of the union, because that will require
            // unpacking them.
            UnionTypeInsertion {
                ty,
                should_simplify_full: !ty.is_alias_like() && !builder.cycle_recovery,
                ty_negated: None,
                to_remove: SmallVec::new(),
            }
        }
        fn removals<'db>(&self, insertion: UnionTypeInsertion<'db>) -> (Type<'db>, smallvec::IntoIter<[usize; 2]>) {
            (insertion.ty, insertion.to_remove.into_iter())
        }
        fn add_count(&self, count: usize, additional: usize) -> usize {
            count + additional
        }
        fn len(&self, types: &[Type<'_>]) -> usize {
            types.len()
        }
        fn first<'db>(&self, types: &[Type<'db>]) -> Type<'db> {
            types[0]
        }
    }

    #[synchronous(add_in_place_sync)]
    #[capabilities(effects = UnionEffects, facts = UnionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn add_in_place_with<'db, E: UnionEffects<'db>>(
        builder: &mut UnionBuilder<'db>, ty: Type<'db>, facts: UnionFacts, effects: &E,
    ) -> Result<(), E::Error> {
        facts.assert_not_recursive_var(ty);
        let mut seen_aliases = facts.empty_aliases();
        effects.add_impl(builder, ty, &mut seen_aliases).await
    }

    #[synchronous(add_in_place_impl_sync)]
    #[capabilities(effects = UnionEffects, facts = UnionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn add_in_place_impl_with<'db, E: UnionEffects<'db>>(
        builder: &mut UnionBuilder<'db>, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>, facts: UnionFacts, effects: &E,
    ) -> Result<(), E::Error> {
        match ty {
            Type::Union(union) => effects.expand_union(builder, union, seen_aliases).await,
            // Adding `Never` to a union is a no-op.
            Type::Never => Ok(()),
            Type::TypeAlias(_) => {
                if facts.unpack_aliases(builder) {
                    effects.expand_alias(builder, ty, seen_aliases).await
                } else {
                    effects.push_type(builder, ty, seen_aliases).await
                }
            }
            Type::LiteralValue(literal) => effects.literal(builder, literal, seen_aliases).await,
            _ => {
                // Adding `object` to a union results in `object`.
                if facts.collapse_object(builder, ty) {
                    effects.collapse_to_object(builder).await
                } else {
                    effects.push_type(builder, ty, seen_aliases).await
                }
            }
        }
    }

    #[synchronous(add_union_sync)]
    #[capabilities(effects = UnionEffects, facts = UnionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn add_union_with<'db, E: UnionEffects<'db>>(
        builder: &mut UnionBuilder<'db>, union: UnionType<'db>, seen_aliases: &mut Vec<Type<'db>>, facts: UnionFacts, effects: &E,
    ) -> Result<(), E::Error> {
        let elements = effects.union_elements(builder, union).await?;
        effects.reserve_union_elements(builder, facts.len(elements)).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(next) = effects.next_type(elements, &mut cursor).await? {
            let (_, element) = next;
            effects.add_impl(builder, element, seen_aliases).await?;
        }
        let recursion = effects.union_recursion(builder, union).await?;
        effects.merge_recursion(builder, recursion).await?;
        if facts.cycle_recovery(builder) && facts.recursive_union(builder) {
            #[passive_state]
            let mut literals = 0;
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(count) = effects.next_literal_count(builder, &mut cursor).await? {
                literals = facts.add_count(literals, count);
            }
            if facts.should_widen_union_literals(builder, literals) {
                effects.widen_literals(builder, seen_aliases).await?;
            }
        }
        Ok(())
    }

    #[synchronous(add_literal_sync)]
    #[capabilities(effects = UnionEffects, facts = UnionFacts)]
    #[passive_values(GroupedLiteral::String, GroupedLiteral::Bytes, GroupedLiteral::Int, GroupedLiteral::Enum, Type::LiteralValue)]
    pub(in crate::types) async fn add_literal_with<'db, E: UnionEffects<'db>>(
        builder: &mut UnionBuilder<'db>, literal: LiteralValueType<'db>, seen_aliases: &mut Vec<Type<'db>>, facts: UnionFacts, effects: &E,
    ) -> Result<(), E::Error> {
        effects.merge_literal_recursion(builder, literal).await?;
        let group = match facts.literal_kind(literal) {
            LiteralValueTypeKind::String(value) => GroupedLiteral::String(value),
            LiteralValueTypeKind::Bytes(value) => GroupedLiteral::Bytes(value),
            LiteralValueTypeKind::Int(value) => GroupedLiteral::Int(value),
            LiteralValueTypeKind::Enum(value) => GroupedLiteral::Enum(value),
            LiteralValueTypeKind::Bool(_) | LiteralValueTypeKind::LiteralString => {
                return effects.push_type(builder, Type::LiteralValue(literal), seen_aliases).await;
            }
        };
        effects.grouped_literal(builder, literal, group, seen_aliases).await
    }

    #[synchronous(push_type_sync)]
    #[capabilities(effects = UnionEffects, facts = UnionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn push_type_with<'db, E: UnionEffects<'db>>(
        builder: &mut UnionBuilder<'db>, ty: Type<'db>, seen_aliases: &mut Vec<Type<'db>>, facts: UnionFacts, effects: &E,
    ) -> Result<(), E::Error> {
        let mut insertion = facts.insertion(builder, ty);
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(index) = effects.next_element(builder, &mut cursor).await? {
            if !effects.reduce_element(builder, index, &mut insertion, seen_aliases).await? {
                return Ok(());
            }
        }
        effects.finish_insertion(builder, insertion).await
    }

    #[synchronous(try_reduce_sync)]
    #[capabilities(effects = UnionEffects)]
    #[passive_values(ReduceResult::Type)]
    pub(in crate::types) async fn try_reduce_with<'db, E: UnionEffects<'db>>(
        builder: &mut UnionBuilder<'db>, index: usize, other: Type<'db>, effects: &E,
    ) -> Result<ReduceResult<'db>, E::Error> {
        if let Some(existing) = effects.plain_element(builder, index).await? {
            return Ok(ReduceResult::Type(existing));
        }
        effects.reduce_literal_group(builder, index, other).await
    }

    /// Return `true` if union simplification should preserve this pair because one element is
    /// `Hashable` and the other is a non-final nominal instance.
    ///
    /// Hashability does not obey normal inheritance rules: subclasses of hashable classes can be
    /// unhashable. Keeping the non-final type allows downstream checks to consider it independently.
    #[synchronous(preserve_hashable_union_sync)]
    #[capabilities(effects = UnionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn preserve_hashable_union_with<'db, E: UnionEffects<'db>>(
        builder: &UnionBuilder<'db>, first: Type<'db>, second: Type<'db>, effects: &E,
    ) -> Result<bool, E::Error> {
        if let Type::ProtocolInstance(protocol) = first
            && effects.protocol_is_hashable(builder, protocol).await?
            && let Type::NominalInstance(instance) = second
            && !effects.nominal_is_final(builder, instance).await?
        {
            return Ok(true);
        }
        if let Type::ProtocolInstance(protocol) = second
            && effects.protocol_is_hashable(builder, protocol).await?
            && let Type::NominalInstance(instance) = first
            && !effects.nominal_is_final(builder, instance).await?
        {
            return Ok(true);
        }
        Ok(false)
    }

    /// Extract `(core, guard)` from truthiness-guarded intersections.
    ///
    /// e.g.
    /// - `A & ~AlwaysTruthy` -> `Some((A, ~AlwaysTruthy))`
    /// - `A & ~AlwaysFalsy` -> `Some((A, ~AlwaysFalsy))`
    /// - `A` -> `None`
    /// - `A & ~AlwaysTruthy & ~AlwaysFalsy` -> `None` (not a single-guard shape)
    ///
    /// This only recognizes the "single truthiness guard" forms used by truthiness narrowing.
    #[synchronous(split_truthiness_guarded_intersection_sync)]
    #[capabilities(effects = UnionEffects)]
    #[passive_values(Type::AlwaysTruthy, Type::AlwaysFalsy)]
    pub(in crate::types) async fn split_truthiness_guarded_intersection_with<'db, E: UnionEffects<'db>>(
        builder: &UnionBuilder<'db>, intersection: IntersectionType<'db>, effects: &E,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, E::Error> {
        let falsy = effects.negate_guard(builder, Type::AlwaysTruthy).await?;
        let truthy = effects.negate_guard(builder, Type::AlwaysFalsy).await?;

        let negative = effects.intersection_negatives(builder, intersection).await?;
        let has_not_truthy = effects.contains_truthiness_guard(negative, true).await?;
        let has_not_falsy = effects.contains_truthiness_guard(negative, false).await?;
        let guard = match (has_not_truthy, has_not_falsy) {
            (true, false) => falsy,
            (false, true) => truthy,
            _ => return Ok(None),
        };

        let mut core = effects.new_guard_core(builder).await?;
        let mut positive = effects.positive_guard_elements(builder, intersection).await?;
        #[cursor_loop]
        while let Some(positive) = effects.next_guard_element(&mut positive).await? {
            effects.add_guard_core_positive(&mut core, positive).await?;
        }
        let mut negative = effects.negative_guard_elements(negative).await?;
        #[cursor_loop]
        while let Some(negative) = effects.next_guard_element(&mut negative).await? {
            if (effects.same_type(guard, falsy).await?
                && effects.same_type(negative, Type::AlwaysTruthy).await?)
                || (effects.same_type(guard, truthy).await?
                    && effects.same_type(negative, Type::AlwaysFalsy).await?)
            {
                continue;
            }
            effects.add_guard_core_negative(&mut core, negative).await?;
        }
        Ok(Some((effects.build_guard_core(&mut core).await?, guard)))
    }

    /// Try to merge a complementary guarded pair into an unguarded core.
    ///
    /// e.g.
    /// - `(A & ~AlwaysTruthy, A & ~AlwaysFalsy)` -> `Some(A)`
    /// - `(A & ~AlwaysTruthy, B & ~AlwaysFalsy)` -> `Some(A | B)` if reconstruction is exact
    /// - `(A & ~AlwaysTruthy, C)` -> `None`
    ///
    /// Safety rule:
    /// The candidate merge is accepted only if adding each original guard back reconstructs
    /// exactly the original operands (`first` and `second`).
    ///
    /// TODO: This processing is specialized for `AlwaysTruthy/AlwaysFalsy`.
    /// It would be nice to generalize this in the future.
    /// Discussion: <https://github.com/astral-sh/ty/issues/224>
    #[synchronous(merge_truthiness_guarded_pair_sync)]
    #[capabilities(effects = UnionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn merge_truthiness_guarded_pair_with<'db, E: UnionEffects<'db>>(
        builder: &UnionBuilder<'db>, first: Type<'db>, second: Type<'db>, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let Type::Intersection(first_intersection) = first else {
            return Ok(None);
        };
        let Some(first_parts) = effects.split_truthiness_guarded_intersection(builder, first_intersection).await? else {
            return Ok(None);
        };
        let Type::Intersection(second_intersection) = second else {
            return Ok(None);
        };
        let Some(second_parts) = effects.split_truthiness_guarded_intersection(builder, second_intersection).await? else {
            return Ok(None);
        };
        let (_, first_guard) = first_parts;
        let (_, second_guard) = second_parts;
        if effects.same_type(first_guard, second_guard).await? {
            return Ok(None);
        }
        effects.merge_truthiness_guarded_cores(builder, first, second, first_parts, second_parts).await
    }

    /// Fold `(T & ~A) | (T & ~B)` to `T` when `A` and `B` are disjoint.
    ///
    /// The common part can itself contain exclusions. For example,
    /// `(Unknown & ~str & ~A) | (Unknown & ~str & ~B)` simplifies to `Unknown & ~str`.
    /// `A` and `B` can each be unions: all exclusions unique to one side must be disjoint
    /// from every exclusion unique to the other side.
    #[synchronous(merge_disjoint_exclusions_sync)]
    #[capabilities(effects = UnionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn merge_disjoint_exclusions_with<'db, E: UnionEffects<'db>>(
        builder: &UnionBuilder<'db>, first: Type<'db>, second: Type<'db>, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let (Type::Intersection(first), Type::Intersection(second)) = (first, second) else {
            return Ok(None);
        };
        effects.merge_intersection_exclusions(builder, first, second).await
    }

    #[synchronous(merge_intersection_exclusions_sync)]
    #[capabilities(effects = UnionEffects)]
    #[passive_values(IntersectionSimplification::Disjoint)]
    pub(in crate::types) async fn merge_intersection_exclusions_with<'db, E: UnionEffects<'db>>(
        builder: &UnionBuilder<'db>, left: IntersectionType<'db>, right: IntersectionType<'db>, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let left_positive = effects.intersection_positives(builder, left).await?;
        let left_negative = effects.intersection_negatives(builder, left).await?;
        let right_negative = effects.intersection_negatives(builder, right).await?;

        let right_positive = effects.intersection_positives(builder, right).await?;
        if !effects.same_positive_sets(left_positive, right_positive).await? {
            return Ok(None);
        }

        let mut common_negative = effects.new_exclusion_buffer().await?;
        let mut left_only = effects.new_exclusion_buffer().await?;
        let mut left_negatives = effects.negative_guard_elements(left_negative).await?;
        #[cursor_loop]
        while let Some(ty) = effects.next_guard_element(&mut left_negatives).await? {
            if effects.contains_exclusion(right_negative, ty).await? {
                effects.push_exclusion(&mut common_negative, ty).await?;
            } else {
                effects.push_exclusion(&mut left_only, ty).await?;
            }
        }

        // Leave trivially redundant operands to the usual union simplification, which preserves
        // their order. This only checks exact containment, not redundancy through subtyping.
        if effects.exclusion_buffer_is_empty(&left_only).await?
            || effects.has_all_exclusions(&common_negative, right_negative).await?
        {
            effects.finish_exclusion_buffer(common_negative).await?;
            effects.finish_exclusion_buffer(left_only).await?;
            return Ok(None);
        }

        let mut right_negatives = effects.negative_guard_elements(right_negative).await?;
        #[cursor_loop]
        while let Some(right_exclusion) = effects.next_guard_element(&mut right_negatives).await? {
            if effects.contains_exclusion(left_negative, right_exclusion).await? {
                continue;
            }
            let mut left_cursor = 0;
            #[cursor_loop]
            while let Some(left_exclusion) = effects.next_exclusion(&left_only, &mut left_cursor).await? {
                if !matches!(
                    effects.simplify_exclusion_pair(builder, left_exclusion, right_exclusion).await?,
                    IntersectionSimplification::Disjoint
                ) {
                    effects.finish_exclusion_buffer(common_negative).await?;
                    effects.finish_exclusion_buffer(left_only).await?;
                    return Ok(None);
                }
            }
        }

        let mut common = effects.new_guard_core(builder).await?;
        let mut positive = effects.retained_positive_elements(left_positive).await?;
        #[cursor_loop]
        while let Some(positive) = effects.next_guard_element(&mut positive).await? {
            effects.add_guard_core_positive(&mut common, positive).await?;
        }
        let mut common_cursor = 0;
        #[cursor_loop]
        while let Some(negative) = effects.next_exclusion(&common_negative, &mut common_cursor).await? {
            effects.add_guard_core_negative(&mut common, negative).await?;
        }
        let merged = effects.build_guard_core(&mut common).await?;
        effects.finish_exclusion_buffer(common_negative).await?;
        effects.finish_exclusion_buffer(left_only).await?;
        Ok(Some(merged))
    }

    #[synchronous(reduce_type_element_sync)]
    #[capabilities(effects = UnionEffects, facts = UnionFacts)]
    #[passive_values(KnownClass::Range, KnownClass::Bool)]
    pub(in crate::types) async fn reduce_type_element_with<'db, E: UnionEffects<'db>>(
        builder: &mut UnionBuilder<'db>, index: usize, insertion: &mut UnionTypeInsertion<'db>,
        seen_aliases: &mut Vec<Type<'db>>, facts: UnionFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        let ty = facts.incoming(insertion);
        let element_type = match effects.reduce_member(builder, index, ty).await? {
            ReduceResult::KeepIf(keep) => {
                if !keep {
                    effects.defer_removal(insertion, index).await?;
                }
                return Ok(true);
            }
            ReduceResult::Type(ty) => ty,
            ReduceResult::CollapseToObject => {
                effects.collapse_to_object(builder).await?;
                return Ok(false);
            }
            ReduceResult::Ignore => return Ok(false),
        };

        if effects.same_type(ty, element_type).await? {
            return Ok(false);
        }

        // `object` already contains every possible union element.
        if !facts.cycle_recovery(builder) && effects.same_type(element_type, facts.object()).await? {
            return Ok(false);
        }

        if !facts.cycle_recovery(builder)
            && effects.preserve_hashable_union(builder, ty, element_type).await?
        {
            return Ok(true);
        }

        // The empty and non-empty range refinements are disjoint, but together they cover
        // the ordinary `range` instance type.
        if facts.opposite_ranges(ty, element_type) {
            effects.defer_removal(insertion, index).await?;
            let range = effects.known_instance(builder, KnownClass::Range).await?;
            effects.set_incoming(insertion, range).await?;
            return Ok(true);
        }

        // Fold `(T & ~AlwaysTruthy) | (T & ~AlwaysFalsy)` to `T`.
        if !facts.cycle_recovery(builder)
            && let Some(merged_type) = effects.merge_truthiness_guarded_pair(builder, ty, element_type).await?
        {
            effects.defer_removal(insertion, index).await?;
            effects.set_incoming(insertion, merged_type).await?;
            return Ok(true);
        }

        if !facts.cycle_recovery(builder) && facts.opposite_bools(ty, element_type) {
            let boolean = effects.known_instance(builder, KnownClass::Bool).await?;
            effects.add_impl(builder, boolean, seen_aliases).await?;
            return Ok(false);
        }

        // Comparing `TypedDict`s for redundancy requires iterating over their fields, which is
        // problematic if some of those fields point to recursive `Union`s. To avoid cycles,
        // compare `TypedDict`s by name/identity instead of using the `has_relation_to`
        // machinery.
        if facts.is_typed_dict(element_type) && facts.is_typed_dict(ty) {
            return Ok(true);
        }

        if facts.should_simplify_full(insertion) && !facts.is_alias_like(element_type) {
            // Preserving aliases also excludes comparisons that expand aliases nested in
            // type arguments. A recursive alias can rebuild this union during specialization.
            if !facts.unpack_aliases(builder)
                && (effects.contains_nested_alias(builder, ty).await?
                    || effects.contains_nested_alias(builder, element_type).await?)
            {
                return Ok(true);
            }
            if let Some(merged) = effects.merge_disjoint_exclusions(builder, ty, element_type).await? {
                effects.defer_removal(insertion, index).await?;
                {
                    let mut removals = effects.take_removals(insertion).await?;
                    #[cursor_loop]
                    while let Some(index) = effects.next_removal_back(&mut removals).await? {
                        effects.remove_type(builder, index).await?;
                    }
                }
                // The common part can also subsume elements we already visited.
                effects.add_impl(builder, merged, seen_aliases).await?;
                return Ok(false);
            }
            if effects.redundant(builder, ty, element_type).await? {
                return Ok(false);
            }

            if effects.redundant(builder, element_type, ty).await? {
                effects.defer_removal(insertion, index).await?;
                return Ok(true);
            }

            if effects.negation_subtype_cached(builder, insertion, element_type).await? {
                // We add `ty` to the union. We just checked that `~ty` is a subtype of an
                // existing `element`. This also means that `~ty | ty` is a subtype of
                // `element | ty`, because both elements in the first union are subtypes of
                // the corresponding elements in the second union. But `~ty | ty` is just
                // `object`. Since `object` is a subtype of `element | ty`, we can only
                // conclude that `element | ty` must be `object` (object has no other
                // supertypes). This means we can simplify the whole union to just
                // `object`, since all other potential elements would also be subtypes of
                // `object`.
                effects.collapse_to_object(builder).await?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    #[synchronous(finish_insertion_sync)]
    #[capabilities(effects = UnionEffects, facts = UnionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn finish_insertion_with<'db, E: UnionEffects<'db>>(
        builder: &mut UnionBuilder<'db>, insertion: UnionTypeInsertion<'db>, facts: UnionFacts, effects: &E,
    ) -> Result<(), E::Error> {
        let (ty, mut removals) = facts.removals(insertion);
        if let Some(first) = effects.next_removal(&mut removals).await? {
            effects.replace_type(builder, first, ty).await?;
            // We iterate in descending order to keep remaining indices valid after `swap_remove`.
            #[cursor_loop]
            while let Some(index) = effects.next_removal_back(&mut removals).await? {
                effects.remove_type(builder, index).await?;
            }
        } else {
            effects.append_type(builder, ty).await?;
        }
        Ok(())
    }

    #[synchronous(try_build_sync)]
    #[capabilities(effects = UnionEffects, facts = UnionFacts)]
    #[passive_values()]
    pub(in crate::types) async fn try_build_with<'db, E: UnionEffects<'db>>(
        mut builder: UnionBuilder<'db>, facts: UnionFacts, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        #[passive_state]
        let mut count = 0;
        let mut count_cursor = 0;
        #[cursor_loop]
        while let Some(additional) = effects.next_type_count(&builder, &mut count_cursor).await? {
            count = facts.add_count(count, additional);
        }
        let mut types = effects.allocate_types(count).await?;
        let mut elements = effects.take_elements(&mut builder).await?;
        #[cursor_loop]
        while let Some(element) = effects.next_owned_element(&mut elements).await? {
            match element {
                UnionElement::Type(ty) => effects.append_converted(&mut types, ty).await?,
                element => effects.convert_literals(&builder, element, &mut types).await?,
            }
        }
        effects.finish_elements(elements).await?;
        if effects.normalize(&builder, &mut types).await? {
            return effects.rebuild(builder, types).await;
        }
        match facts.len(&types) {
            0 => Ok(None),
            1 => Ok(Some(facts.first(&types))),
            _ => Ok(Some(effects.intern(builder, types).await?)),
        }
    }

    #[synchronous(normalize_enum_complement_unions_sync)]
    #[capabilities(effects = UnionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn normalize_enum_complement_unions_with<'db, E: UnionEffects<'db>>(
        builder: &UnionBuilder<'db>, types: &mut Vec<Type<'db>>, effects: &E,
    ) -> Result<bool, E::Error> {
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(indexed_type) = effects.next_type(types, &mut cursor).await? {
            let (index, ty) = indexed_type;
            if let Type::EnumComplement(complement) = ty
                && effects.normalize_complement(builder, types, index, complement).await?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl<'db> SynchronousUnionEffects<'db> for OrdinaryUnionEffects {
    type Error = Infallible;

    fn add_impl(
        &self,
        builder: &mut UnionBuilder<'db>,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Result<(), Self::Error> {
        builder.add_in_place_impl(ty, seen_aliases);
        Ok(())
    }
    fn expand_union(
        &self,
        builder: &mut UnionBuilder<'db>,
        union: UnionType<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Result<(), Self::Error> {
        builder.add_union(union, seen_aliases);
        Ok(())
    }
    fn union_elements(
        &self,
        builder: &UnionBuilder<'db>,
        union: UnionType<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(union.elements(builder.db))
    }
    fn reserve_union_elements(
        &self,
        builder: &mut UnionBuilder<'db>,
        additional: usize,
    ) -> Result<(), Self::Error> {
        builder.elements.reserve(additional);
        Ok(())
    }
    fn union_recursion(
        &self,
        builder: &UnionBuilder<'db>,
        union: UnionType<'db>,
    ) -> Result<RecursivelyDefined, Self::Error> {
        Ok(union.recursively_defined(builder.db))
    }
    fn merge_recursion(
        &self,
        builder: &mut UnionBuilder<'db>,
        recursion: RecursivelyDefined,
    ) -> Result<(), Self::Error> {
        builder.merge_recursively_defined(recursion);
        Ok(())
    }
    fn next_literal_count(
        &self,
        builder: &UnionBuilder<'db>,
        cursor: &mut usize,
    ) -> Result<Option<usize>, Self::Error> {
        Ok(builder.next_literal_count(cursor))
    }
    fn widen_literals(
        &self,
        builder: &mut UnionBuilder<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Result<(), Self::Error> {
        builder.widen_literal_types(seen_aliases);
        Ok(())
    }
    fn expand_alias(
        &self,
        builder: &mut UnionBuilder<'db>,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Result<(), Self::Error> {
        builder.add_alias(ty, seen_aliases);
        Ok(())
    }
    fn literal(
        &self,
        builder: &mut UnionBuilder<'db>,
        literal: LiteralValueType<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Result<(), Self::Error> {
        builder.add_literal(literal, seen_aliases);
        Ok(())
    }
    fn merge_literal_recursion(
        &self,
        builder: &mut UnionBuilder<'db>,
        literal: LiteralValueType<'db>,
    ) -> Result<(), Self::Error> {
        builder.merge_literal_recursion(literal);
        Ok(())
    }
    fn grouped_literal(
        &self,
        builder: &mut UnionBuilder<'db>,
        literal: LiteralValueType<'db>,
        group: GroupedLiteral<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Result<(), Self::Error> {
        builder.add_grouped_literal(literal, group, seen_aliases);
        Ok(())
    }
    fn collapse_to_object(&self, builder: &mut UnionBuilder<'db>) -> Result<(), Self::Error> {
        builder.collapse_to_object();
        Ok(())
    }
    fn push_type(
        &self,
        builder: &mut UnionBuilder<'db>,
        ty: Type<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Result<(), Self::Error> {
        builder.push_type(ty, seen_aliases);
        Ok(())
    }
    fn next_element(
        &self,
        builder: &UnionBuilder<'db>,
        cursor: &mut usize,
    ) -> Result<Option<usize>, Self::Error> {
        Ok(builder.next_element(cursor))
    }
    fn reduce_element(
        &self,
        builder: &mut UnionBuilder<'db>,
        index: usize,
        insertion: &mut UnionTypeInsertion<'db>,
        seen_aliases: &mut Vec<Type<'db>>,
    ) -> Result<bool, Self::Error> {
        Ok(builder.reduce_type_element(index, insertion, seen_aliases))
    }
    fn plain_element(
        &self,
        builder: &UnionBuilder<'db>,
        index: usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(match builder.element(index) {
            Some(UnionElement::Type(ty)) => Some(*ty),
            _ => None,
        })
    }
    fn reduce_member(
        &self,
        builder: &mut UnionBuilder<'db>,
        index: usize,
        other: Type<'db>,
    ) -> Result<ReduceResult<'db>, Self::Error> {
        try_reduce_sync(builder, index, other, self)
    }
    fn reduce_literal_group(
        &self,
        builder: &mut UnionBuilder<'db>,
        index: usize,
        other: Type<'db>,
    ) -> Result<ReduceResult<'db>, Self::Error> {
        Ok(builder.elements[index].try_reduce(
            builder.db,
            &builder.env,
            other,
            builder.cycle_recovery,
        ))
    }
    fn same_type(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error> {
        Ok(first == second)
    }
    fn preserve_hashable_union(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<bool, Self::Error> {
        preserve_hashable_union_sync(builder, first, second, self)
    }
    fn protocol_is_hashable(
        &self,
        builder: &UnionBuilder<'db>,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(protocol.is_hashable(builder.db))
    }
    fn nominal_is_final(
        &self,
        builder: &UnionBuilder<'db>,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(instance
            .class(builder.db, builder.environment())
            .is_final(builder.db))
    }
    fn known_instance(
        &self,
        builder: &UnionBuilder<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(builder.db, builder.environment()))
    }
    fn merge_truthiness_guarded_pair(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        merge_truthiness_guarded_pair_sync(builder, first, second, self)
    }
    fn split_truthiness_guarded_intersection(
        &self,
        builder: &UnionBuilder<'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error> {
        split_truthiness_guarded_intersection_sync(builder, intersection, self)
    }
    fn negate_guard(
        &self,
        builder: &UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.negate(builder.db, builder.environment()))
    }
    fn intersection_negatives(
        &self,
        builder: &UnionBuilder<'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<&'db NegativeIntersectionElements<'db>, Self::Error> {
        Ok(intersection.negative(builder.db))
    }
    fn contains_truthiness_guard(
        &self,
        negative: &NegativeIntersectionElements<'db>,
        always_truthy: bool,
    ) -> Result<bool, Self::Error> {
        Ok(negative.contains(&if always_truthy {
            Type::AlwaysTruthy
        } else {
            Type::AlwaysFalsy
        }))
    }
    fn new_guard_core(
        &self,
        builder: &UnionBuilder<'db>,
    ) -> Result<IntersectionBuilder<'db>, Self::Error> {
        Ok(IntersectionBuilder::new(builder.db, builder.environment()))
    }
    fn positive_guard_elements(
        &self,
        builder: &UnionBuilder<'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<Elements<'db>, Self::Error> {
        Ok(Elements::Positive(intersection.positive(builder.db).iter()))
    }
    fn negative_guard_elements(
        &self,
        negative: &'db NegativeIntersectionElements<'db>,
    ) -> Result<Elements<'db>, Self::Error> {
        Ok(Elements::Negative(negative.iter()))
    }
    fn next_guard_element(
        &self,
        elements: &mut Elements<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(match elements {
            Elements::Positive(elements) => elements.next().copied(),
            Elements::Negative(elements) => elements.next().copied(),
        })
    }
    fn add_guard_core_positive(
        &self,
        core: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        core.add_positive_in_place(ty);
        Ok(())
    }
    fn add_guard_core_negative(
        &self,
        core: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        core.add_negative_in_place(ty);
        Ok(())
    }
    fn build_guard_core(
        &self,
        core: &mut IntersectionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(intersection_assembly::build(core))
    }
    fn merge_truthiness_guarded_cores(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
        first_parts: (Type<'db>, Type<'db>),
        second_parts: (Type<'db>, Type<'db>),
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(super::merge_truthiness_guarded_cores(
            builder.db,
            builder.environment(),
            first,
            second,
            first_parts,
            second_parts,
        ))
    }
    fn contains_nested_alias(
        &self,
        builder: &UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(any_over_type(
            builder.db,
            builder.environment(),
            ty,
            false,
            Type::is_alias_like,
        ))
    }
    fn merge_disjoint_exclusions(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        merge_disjoint_exclusions_sync(builder, first, second, self)
    }
    fn merge_intersection_exclusions(
        &self,
        builder: &UnionBuilder<'db>,
        first: IntersectionType<'db>,
        second: IntersectionType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        merge_intersection_exclusions_sync(builder, first, second, self)
    }
    fn intersection_positives(
        &self,
        builder: &UnionBuilder<'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error> {
        Ok(intersection.positive(builder.db))
    }
    fn same_positive_sets(
        &self,
        first: &'db FxOrderSet<Type<'db>>,
        second: &'db FxOrderSet<Type<'db>>,
    ) -> Result<bool, Self::Error> {
        Ok(first.set_eq(second))
    }
    fn retained_positive_elements(
        &self,
        positive: &'db FxOrderSet<Type<'db>>,
    ) -> Result<Elements<'db>, Self::Error> {
        Ok(Elements::Positive(positive.iter()))
    }
    fn contains_exclusion(
        &self,
        negative: &'db NegativeIntersectionElements<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(negative.contains(&ty))
    }
    fn has_all_exclusions(
        &self,
        buffer: &ExclusionBuffer<'db>,
        negative: &NegativeIntersectionElements<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(buffer.len() == negative.len())
    }
    fn new_exclusion_buffer(&self) -> Result<ExclusionBuffer<'db>, Self::Error> {
        Ok(ExclusionBuffer::new())
    }
    fn push_exclusion(
        &self,
        buffer: &mut ExclusionBuffer<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        buffer.push(ty);
        Ok(())
    }
    fn exclusion_buffer_is_empty(
        &self,
        buffer: &ExclusionBuffer<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(buffer.len() == 0)
    }
    fn next_exclusion(
        &self,
        buffer: &ExclusionBuffer<'db>,
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(buffer.next(cursor))
    }
    fn finish_exclusion_buffer(&self, buffer: ExclusionBuffer<'db>) -> Result<(), Self::Error> {
        drop(buffer);
        Ok(())
    }
    fn simplify_exclusion_pair(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<IntersectionSimplification, Self::Error> {
        Ok(simplify_intersection_pair(
            builder.db,
            builder.environment(),
            first,
            second,
            IntersectionPolarity::Positive,
        ))
    }
    fn redundant(
        &self,
        builder: &UnionBuilder<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(first.is_redundant_with(builder.db, builder.environment(), second))
    }
    fn negation_subtype_cached(
        &self,
        builder: &UnionBuilder<'db>,
        insertion: &mut UnionTypeInsertion<'db>,
        target: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(insertion.ty().negation_is_subtype_of_cached(
            builder.db,
            builder.environment(),
            target,
            insertion.negation_cache(),
        ))
    }
    fn defer_removal(
        &self,
        insertion: &mut UnionTypeInsertion<'db>,
        index: usize,
    ) -> Result<(), Self::Error> {
        insertion.defer_removal(index);
        Ok(())
    }
    fn set_incoming(
        &self,
        insertion: &mut UnionTypeInsertion<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        insertion.set_type(ty);
        Ok(())
    }
    fn take_removals(
        &self,
        insertion: &mut UnionTypeInsertion<'db>,
    ) -> Result<smallvec::IntoIter<[usize; 2]>, Self::Error> {
        Ok(insertion.take_removals())
    }
    fn finish_insertion(
        &self,
        builder: &mut UnionBuilder<'db>,
        insertion: UnionTypeInsertion<'db>,
    ) -> Result<(), Self::Error> {
        finish_insertion_sync(builder, insertion, UnionFacts, self)
    }
    fn next_removal(
        &self,
        removals: &mut smallvec::IntoIter<[usize; 2]>,
    ) -> Result<Option<usize>, Self::Error> {
        Ok(removals.next())
    }
    fn next_removal_back(
        &self,
        removals: &mut smallvec::IntoIter<[usize; 2]>,
    ) -> Result<Option<usize>, Self::Error> {
        Ok(removals.next_back())
    }
    fn replace_type(
        &self,
        builder: &mut UnionBuilder<'db>,
        index: usize,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.replace_type(index, ty);
        Ok(())
    }
    fn remove_type(
        &self,
        builder: &mut UnionBuilder<'db>,
        index: usize,
    ) -> Result<(), Self::Error> {
        builder.remove_type(index);
        Ok(())
    }
    fn append_type(
        &self,
        builder: &mut UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builder.append_type(ty);
        Ok(())
    }
    fn next_type_count(
        &self,
        builder: &UnionBuilder<'db>,
        cursor: &mut usize,
    ) -> Result<Option<usize>, Self::Error> {
        Ok(builder.next_type_count(cursor))
    }
    fn allocate_types(&self, count: usize) -> Result<Vec<Type<'db>>, Self::Error> {
        Ok(Vec::with_capacity(count))
    }
    fn take_elements(
        &self,
        builder: &mut UnionBuilder<'db>,
    ) -> Result<UnionElements<'db>, Self::Error> {
        Ok(builder.take_elements())
    }
    fn next_owned_element(
        &self,
        elements: &mut UnionElements<'db>,
    ) -> Result<Option<UnionElement<'db>>, Self::Error> {
        Ok(elements.next())
    }
    fn finish_elements(&self, elements: UnionElements<'db>) -> Result<(), Self::Error> {
        drop(elements);
        Ok(())
    }
    fn append_converted(
        &self,
        types: &mut Vec<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        types.push(ty);
        Ok(())
    }
    fn convert_literals(
        &self,
        builder: &UnionBuilder<'db>,
        element: UnionElement<'db>,
        types: &mut Vec<Type<'db>>,
    ) -> Result<(), Self::Error> {
        builder.convert_element(element, types);
        Ok(())
    }
    fn normalize(
        &self,
        builder: &UnionBuilder<'db>,
        types: &mut Vec<Type<'db>>,
    ) -> Result<bool, Self::Error> {
        normalize_enum_complement_unions_sync(builder, types, self)
    }
    fn next_type(
        &self,
        types: &[Type<'db>],
        cursor: &mut usize,
    ) -> Result<Option<(usize, Type<'db>)>, Self::Error> {
        let result = types.get(*cursor).copied().map(|ty| (*cursor, ty));
        if result.is_some() {
            *cursor += 1;
        }
        Ok(result)
    }
    fn normalize_complement(
        &self,
        builder: &UnionBuilder<'db>,
        types: &mut Vec<Type<'db>>,
        index: usize,
        complement: EnumComplement<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(super::normalize_enum_complement_union(
            builder.db,
            &builder.env,
            types,
            index,
            complement,
        ))
    }
    fn rebuild(
        &self,
        builder: UnionBuilder<'db>,
        types: Vec<Type<'db>>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let rebuilt = UnionBuilder::new(builder.db, &builder.env)
            .unpack_aliases(builder.unpack_aliases)
            .cycle_recovery(builder.cycle_recovery)
            .or_recursively_defined(builder.recursively_defined);
        Ok(types
            .into_iter()
            .fold(rebuilt, UnionBuilder::add)
            .try_build())
    }
    fn intern(
        &self,
        builder: UnionBuilder<'db>,
        types: Vec<Type<'db>>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(Type::Union(UnionType::new(
            builder.db,
            types.into_boxed_slice(),
            builder.recursion_state(),
        )))
    }
}
