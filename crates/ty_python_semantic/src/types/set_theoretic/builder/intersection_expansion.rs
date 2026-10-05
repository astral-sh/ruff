//! Signed intersection expansion with a flat continuation stack.

use std::convert::Infallible;
use std::marker::PhantomData;
use std::ops::ControlFlow;

use smallvec::{SmallVec, smallvec};
use ty_mapping_probe_macros::shared_semantic_family;

#[cfg(any(test, feature = "experimental-analysis"))]
use super::InnerIntersectionBuilder;
#[cfg(any(test, feature = "experimental-analysis"))]
use super::intersection_assembly::IntersectionBranches;
use super::intersection_distribution::{self, DistributionFacts, OrdinaryDistributionEffects};
use super::intersection_distribution_storage::DistributionSet;
use super::{IntersectionBuilder, IntersectionLimits};
#[cfg(any(test, feature = "experimental-analysis"))]
use crate::ProgramEnvironment;
use crate::types::enums::EnumComplement;
use crate::types::set_theoretic::NegativeIntersectionElementsIterator;
use crate::types::{IntersectionType, Type, UnionType};

#[derive(Clone, Copy)]
pub(in crate::types) enum Sign {
    Positive,
    Negative,
}

enum Builder<'a, 'db> {
    Borrowed(&'a mut IntersectionBuilder<'db>),
    Owned(IntersectionBuilder<'db>),
}

impl<'db> Builder<'_, 'db> {
    fn get(&self) -> &IntersectionBuilder<'db> {
        match self {
            Self::Borrowed(builder) => builder,
            Self::Owned(builder) => builder,
        }
    }

    fn get_mut(&mut self) -> &mut IntersectionBuilder<'db> {
        match self {
            Self::Borrowed(builder) => builder,
            Self::Owned(builder) => builder,
        }
    }
}

enum Aliases<'a, 'db> {
    Borrowed(&'a mut Vec<Type<'db>>),
    Owned(Vec<Type<'db>>),
}

impl<'db> Aliases<'_, 'db> {
    fn get(&self) -> &Vec<Type<'db>> {
        match self {
            Self::Borrowed(aliases) => aliases,
            Self::Owned(aliases) => aliases,
        }
    }

    fn get_mut(&mut self) -> &mut Vec<Type<'db>> {
        match self {
            Self::Borrowed(aliases) => aliases,
            Self::Owned(aliases) => aliases,
        }
    }
}

pub(in crate::types) enum Elements<'db> {
    Union(std::slice::Iter<'db, Type<'db>>),
    Positive(ordermap::set::Iter<'db, Type<'db>>),
    Negative(NegativeIntersectionElementsIterator<'db, 'db>),
}

pub(in crate::types) struct Distribution<'db> {
    elements: Elements<'db>,
    next_negative: Option<IntersectionType<'db>>,
    sign: Sign,
    distributed: DistributionSet<'db>,
    check_budget: bool,
    has_disjunction: bool,
    clone_aliases: bool,
}

/// A suspended parent owns no child or continuation. Shared alias history stays with the child.
pub(in crate::types) struct Parent<'a, 'db> {
    builder: Builder<'a, 'db>,
    aliases: Option<Aliases<'a, 'db>>,
    #[cfg(test)]
    _lifetime: expansion_observations::ParentLifetime,
}

impl<'db> Parent<'_, 'db> {
    pub(in crate::types) fn builder(&self) -> &IntersectionBuilder<'db> {
        self.builder.get()
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn owned_builder(&self) -> Option<&IntersectionBuilder<'db>> {
        match &self.builder {
            Builder::Owned(builder) => Some(builder),
            Builder::Borrowed(_) => None,
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn owned_aliases_storage(&self) -> Option<(&[Type<'db>], usize)> {
        match &self.aliases {
            Some(Aliases::Owned(aliases)) => Some((aliases, aliases.capacity())),
            _ => None,
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn restores_aliases(&self) -> bool {
        self.aliases.is_some()
    }
}

pub(in crate::types) enum Frame<'a, 'db> {
    Add(Type<'db>, Sign),
    Sequence {
        elements: Elements<'db>,
        sign: Sign,
        next_negative: Option<IntersectionType<'db>>,
    },
    Distribute(Distribution<'db>),
    Extend {
        distribution: Distribution<'db>,
        parent: Parent<'a, 'db>,
    },
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<'a, 'db> Frame<'a, 'db> {
    pub(in crate::types) fn parent(&self) -> Option<&Parent<'a, 'db>> {
        match self {
            Self::Extend { parent, .. } => Some(parent),
            _ => None,
        }
    }

    pub(in crate::types) fn distribution(&self) -> Option<&DistributionSet<'db>> {
        match self {
            Self::Distribute(distribution) | Self::Extend { distribution, .. } => {
                Some(&distribution.distributed)
            }
            _ => None,
        }
    }
}

pub(in crate::types) struct Expansion<'a, 'db> {
    builder: Builder<'a, 'db>,
    aliases: Aliases<'a, 'db>,
    frames: SmallVec<[Frame<'a, 'db>; 2]>,
    #[cfg(test)]
    _lifetime: expansion_observations::ExpansionLifetime,
}

impl<'a, 'db> Expansion<'a, 'db> {
    pub(in crate::types) fn new(
        builder: &'a mut IntersectionBuilder<'db>,
        aliases: &'a mut Vec<Type<'db>>,
        initial: Frame<'a, 'db>,
    ) -> Self {
        Self {
            builder: Builder::Borrowed(builder),
            aliases: Aliases::Borrowed(aliases),
            frames: smallvec![initial],
            #[cfg(test)]
            _lifetime: expansion_observations::entered_expansion(),
        }
    }

    pub(in crate::types) fn next_frame(&mut self) -> Option<Frame<'a, 'db>> {
        self.frames.pop()
    }

    pub(in crate::types) fn push_frame(&mut self, frame: Frame<'a, 'db>) {
        self.frames.push(frame);
        #[cfg(test)]
        expansion_observations::frames(self.frames.len(), self.frames.spilled());
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn frames_storage(&self) -> (usize, usize, bool) {
        (
            self.frames.len(),
            self.frames.capacity(),
            self.frames.spilled(),
        )
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn frame(&self, index: usize) -> Option<&Frame<'a, 'db>> {
        self.frames.get(index)
    }

    pub(in crate::types) fn builder(&self) -> &IntersectionBuilder<'db> {
        self.builder.get()
    }

    pub(in crate::types) fn builder_mut(&mut self) -> &mut IntersectionBuilder<'db> {
        self.builder.get_mut()
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn owned_builder(&self) -> Option<&IntersectionBuilder<'db>> {
        match &self.builder {
            Builder::Owned(builder) => Some(builder),
            Builder::Borrowed(_) => None,
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn aliases_storage(&self) -> (&[Type<'db>], usize) {
        let aliases = self.aliases.get();
        (aliases, aliases.capacity())
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn owned_aliases_storage(&self) -> Option<(&[Type<'db>], usize)> {
        match &self.aliases {
            Aliases::Owned(aliases) => Some((aliases, aliases.capacity())),
            Aliases::Borrowed(_) => None,
        }
    }

    pub(in crate::types) fn seen_alias(&self, ty: Type<'db>) -> bool {
        self.aliases.get().contains(&ty)
    }

    pub(in crate::types) fn remember_alias(&mut self, ty: Type<'db>) {
        self.aliases.get_mut().push(ty);
    }

    pub(in crate::types) fn branch(&mut self, clone_aliases: bool) -> Parent<'a, 'db> {
        let branch = self.builder.get().clone();
        let builder = std::mem::replace(&mut self.builder, Builder::Owned(branch));
        let aliases = if clone_aliases {
            let aliases = self.aliases.get().clone();
            Some(std::mem::replace(
                &mut self.aliases,
                Aliases::Owned(aliases),
            ))
        } else {
            None
        };
        Parent {
            builder,
            aliases,
            #[cfg(test)]
            _lifetime: expansion_observations::entered_parent(),
        }
    }

    pub(in crate::types) fn restore_aliases(&mut self, parent: &mut Parent<'a, 'db>) {
        if let Some(aliases) = parent.aliases.take() {
            self.aliases = aliases;
        }
    }

    pub(in crate::types) fn restore(&mut self, parent: Parent<'a, 'db>) {
        self.builder = parent.builder;
        if let Some(aliases) = parent.aliases {
            self.aliases = aliases;
        }
    }

    pub(in crate::types) fn install(
        &mut self,
        distributed: DistributionSet<'db>,
        has_disjunction: bool,
    ) {
        let builder = self.builder.get_mut();
        builder.intersections = distributed.into_intersections();
        builder.has_disjunction = has_disjunction;
    }

    pub(in crate::types) fn next_inner(&self, cursor: &mut usize) -> Option<usize> {
        if *cursor < self.builder.get().intersections.len() {
            let index = *cursor;
            *cursor += 1;
            Some(index)
        } else {
            None
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn inner_with_environment(
        &mut self,
        index: usize,
    ) -> Option<(&ProgramEnvironment<'db>, &mut InnerIntersectionBuilder<'db>)> {
        self.builder.get_mut().inner_with_environment(index)
    }
}

impl<'db> IntersectionBuilder<'db> {
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn branches_storage(&self) -> (&[InnerIntersectionBuilder<'db>], usize) {
        (&self.intersections, self.intersections.capacity())
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn environment(&self) -> &ProgramEnvironment<'db> {
        &self.env
    }

    pub(in crate::types) fn has_disjunction(&self) -> bool {
        self.has_disjunction
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn inner_with_environment(
        &mut self,
        index: usize,
    ) -> Option<(&ProgramEnvironment<'db>, &mut InnerIntersectionBuilder<'db>)> {
        let env = &self.env;
        self.intersections.get_mut(index).map(|inner| (env, inner))
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn take_branches(&mut self) -> IntersectionBranches<'db> {
        IntersectionBranches::take(self)
    }
}

#[cfg(test)]
pub(in crate::types) mod expansion_observations {
    use std::cell::Cell;

    #[derive(Clone, Copy, Default)]
    pub(in crate::types) struct Progress {
        pub live_expansions: usize,
        pub entered_expansions: usize,
        pub live_parents: usize,
        pub entered_parents: usize,
        pub peak_parents: usize,
        pub peak_frames: usize,
        pub spilled_frames: bool,
    }

    thread_local! {
        static PROGRESS: Cell<Progress> = const { Cell::new(Progress {
            live_expansions: 0,
            entered_expansions: 0,
            live_parents: 0,
            entered_parents: 0,
            peak_parents: 0,
            peak_frames: 0,
            spilled_frames: false,
        }) };
    }

    pub(in crate::types) fn reset() {
        assert_eq!(PROGRESS.get().live_expansions, 0);
        assert_eq!(PROGRESS.get().live_parents, 0);
        PROGRESS.set(Progress::default());
    }

    pub(in crate::types) fn progress() -> Progress {
        PROGRESS.get()
    }

    pub(super) struct ExpansionLifetime;
    pub(super) struct ParentLifetime;

    impl Drop for ExpansionLifetime {
        fn drop(&mut self) {
            let mut progress = PROGRESS.get();
            progress.live_expansions -= 1;
            PROGRESS.set(progress);
        }
    }

    impl Drop for ParentLifetime {
        fn drop(&mut self) {
            let mut progress = PROGRESS.get();
            progress.live_parents -= 1;
            PROGRESS.set(progress);
        }
    }

    pub(super) fn entered_expansion() -> ExpansionLifetime {
        let mut progress = PROGRESS.get();
        progress.live_expansions += 1;
        progress.entered_expansions += 1;
        progress.peak_frames = progress.peak_frames.max(1);
        PROGRESS.set(progress);
        ExpansionLifetime
    }

    pub(super) fn entered_parent() -> ParentLifetime {
        let mut progress = PROGRESS.get();
        progress.live_parents += 1;
        progress.entered_parents += 1;
        progress.peak_parents = progress.peak_parents.max(progress.live_parents);
        PROGRESS.set(progress);
        ParentLifetime
    }

    pub(super) fn frames(len: usize, spilled: bool) {
        let mut progress = PROGRESS.get();
        progress.peak_frames = progress.peak_frames.max(len);
        progress.spilled_frames |= spilled;
        PROGRESS.set(progress);
    }
}

pub(super) struct OrdinaryExpansionEffects<L>(PhantomData<L>);

pub(in crate::types) struct ExpansionFacts;

impl<L> OrdinaryExpansionEffects<L> {
    pub(super) fn new() -> Self {
        Self(PhantomData)
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousExpansionEffects)]
    // Owned arguments stay in the effect's future until admission succeeds. A controlled
    // implementation borrows those owners from its admission callback and only then takes
    // their payloads. Allocation and cloning must also prepay automatic disposal on refusal.
    pub(in crate::types) trait ExpansionEffects<'db> {
        type Error;
        type Break;

        #[operation(local)]
        async fn start<'a>(&self, builder: &'a mut IntersectionBuilder<'db>, aliases: &'a mut Vec<Type<'db>>, initial: Frame<'a, 'db>) -> Result<Expansion<'a, 'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next<'a>(&self, expansion: &mut Expansion<'a, 'db>) -> Result<Option<Frame<'a, 'db>>, Self::Error>;
        #[operation(local)]
        async fn push<'a>(&self, expansion: &mut Expansion<'a, 'db>, frame: Frame<'a, 'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish(&self, expansion: Expansion<'_, 'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_failed(&self, expansion: Expansion<'_, 'db>, parent: Parent<'_, 'db>, distributed: DistributionSet<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn seen_alias(&self, expansion: &Expansion<'_, 'db>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn remember_alias(&self, expansion: &mut Expansion<'_, 'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn resolve_alias(&self, expansion: &Expansion<'_, 'db>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn union_elements(&self, expansion: &Expansion<'_, 'db>, union: UnionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(source)]
        async fn positive_elements(&self, expansion: &Expansion<'_, 'db>, intersection: IntersectionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(source)]
        async fn negative_elements(&self, expansion: &Expansion<'_, 'db>, intersection: IntersectionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(source)]
        async fn signed_element_count(&self, expansion: &Expansion<'_, 'db>, intersection: IntersectionType<'db>) -> Result<usize, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element(&self, elements: &mut Elements<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn enum_intersection(&self, expansion: &Expansion<'_, 'db>, complement: EnumComplement<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn new_distribution(&self) -> Result<DistributionSet<'db>, Self::Error>;
        #[operation(local)]
        async fn has_disjunction(&self, expansion: &Expansion<'_, 'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn branch<'a>(&self, expansion: &mut Expansion<'a, 'db>, clone_aliases: bool) -> Result<Parent<'a, 'db>, Self::Error>;
        #[operation(local)]
        async fn restore_aliases<'a>(&self, expansion: &mut Expansion<'a, 'db>, parent: &mut Parent<'a, 'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn extend(&self, expansion: &mut Expansion<'_, 'db>, parent: &Parent<'_, 'db>, distributed: &mut DistributionSet<'db>, check_budget: bool) -> Result<ControlFlow<Self::Break>, Self::Error>;
        #[operation(local)]
        async fn restore<'a>(&self, expansion: &mut Expansion<'a, 'db>, parent: Parent<'a, 'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn install(&self, expansion: &mut Expansion<'_, 'db>, distributed: DistributionSet<'db>, has_disjunction: bool) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_inner(&self, expansion: &Expansion<'_, 'db>, cursor: &mut usize) -> Result<Option<usize>, Self::Error>;
        #[operation(local)]
        async fn insert_recursive(&self, expansion: &mut Expansion<'_, 'db>, index: usize, ty: Type<'db>, sign: Sign) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn add_inner(&self, expansion: &mut Expansion<'_, 'db>, index: usize, ty: Type<'db>, sign: Sign) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ExpansionFacts {
        fn multiple_branches(&self, branches: usize) -> bool {
            branches > 1
        }
    }

    #[synchronous(add_sync)]
    #[capabilities(effects = ExpansionEffects, facts = ExpansionFacts)]
    #[passive_values(Frame::Add, Frame::Sequence, Frame::Distribute, Frame::Extend, Distribution, Sign::Positive, Sign::Negative, ControlFlow::Continue, ControlFlow::Break)]
    pub(in crate::types) async fn add_with<'a, 'db, E: ExpansionEffects<'db>>(
        builder: &'a mut IntersectionBuilder<'db>,
        ty: Type<'db>,
        sign: Sign,
        seen_aliases: &'a mut Vec<Type<'db>>,
        facts: ExpansionFacts,
        effects: &E,
    ) -> Result<ControlFlow<E::Break>, E::Error> {
        let mut expansion = effects.start(builder, seen_aliases, Frame::Add(ty, sign)).await?;
        #[cursor_loop]
        while let Some(frame) = effects.next(&mut expansion).await? {
            match frame {
                Frame::Add(ty, sign) => match ty {
                    Type::TypeAlias(_) | Type::Recursive(_) => {
                        if effects.seen_alias(&expansion, ty).await? {
                            // Recursive alias, add it without expanding to avoid infinite recursion.
                            let mut cursor = 0;
                            #[cursor_loop]
                            while let Some(index) = effects.next_inner(&expansion, &mut cursor).await? {
                                effects.insert_recursive(&mut expansion, index, ty, sign).await?;
                            }
                        } else {
                            effects.remember_alias(&mut expansion, ty).await?;
                            let value_type = effects.resolve_alias(&expansion, ty).await?;
                            effects.push(&mut expansion, Frame::Add(value_type, sign)).await?;
                        }
                    }
                    Type::Union(union) => match sign {
                        Sign::Positive => {
                            // Distribute ourself over this union: for each union element, clone ourself and
                            // intersect with that union element, then create a new union-of-intersections with all
                            // of those sub-intersections in it. E.g. if `self` is a simple intersection `T1 & T2`
                            // and we add `T3 | T4` to the intersection, we don't get `T1 & T2 & (T3 | T4)` (that's
                            // not in DNF), we distribute the union and get `(T1 & T3) | (T2 & T3) | (T1 & T4) |
                            // (T2 & T4)`. If `self` is already a union-of-intersections `(T1 & T2) | (T3 & T4)`
                            // and we add `T5 | T6` to it, that flattens all the way out to `(T1 & T2 & T5) | (T1 &
                            // T2 & T6) | (T3 & T4 & T5) ...` -- you get the idea.
                            let distributed = effects.new_distribution().await?;
                            let elements = effects.union_elements(&expansion, union).await?;
                            let check_budget = effects.has_disjunction(&expansion).await?;
                            effects.push(&mut expansion, Frame::Distribute(Distribution {
                                elements, next_negative: None, sign, distributed, check_budget,
                                has_disjunction: true, clone_aliases: false,
                            })).await?;
                        }
                        Sign::Negative => {
                            let elements = effects.union_elements(&expansion, union).await?;
                            effects.push(&mut expansion, Frame::Sequence { elements, sign, next_negative: None }).await?;
                        }
                    },
                    Type::Intersection(intersection) => match sign {
                        // `(A & B & ~C) & (D & E & ~F)` -> `A & B & D & E & ~C & ~F`
                        Sign::Positive => {
                            let elements = effects.positive_elements(&expansion, intersection).await?;
                            effects.push(&mut expansion, Frame::Sequence { elements, sign, next_negative: Some(intersection) }).await?;
                        }
                        Sign::Negative => {
                            // (A | B) & ~(C & ~D)
                            // -> (A | B) & (~C | D)
                            // -> ((A | B) & ~C) | ((A | B) & D)
                            // i.e. if we have an intersection of positive constraints C
                            // and negative constraints D, then our new intersection
                            // is (existing & ~C) | (existing & D)
                            let distributed = effects.new_distribution().await?;
                            // A single negative element can encode double negation. It only introduces a
                            // disjunction if expanding that element does, for example `~~Alias` for a union.
                            let branches = effects.signed_element_count(&expansion, intersection).await?;
                            let was_disjunction = effects.has_disjunction(&expansion).await?;
                            let check_budget = was_disjunction && facts.multiple_branches(branches);
                            let has_disjunction = was_disjunction || facts.multiple_branches(branches);
                            // We negate all the positive constraints while distributing.
                            let elements = effects.positive_elements(&expansion, intersection).await?;
                            effects.push(&mut expansion, Frame::Distribute(Distribution {
                                elements, next_negative: Some(intersection), sign, distributed,
                                check_budget, has_disjunction, clone_aliases: true,
                            })).await?;
                        }
                    },
                    Type::EnumComplement(complement) => {
                        let intersection = effects.enum_intersection(&expansion, complement).await?;
                        effects.push(&mut expansion, Frame::Add(intersection, sign)).await?;
                    }
                    _ => {
                        // If we are already a union-of-intersections, distribute the new intersected element
                        // across all of those intersections.
                        let mut cursor = 0;
                        #[cursor_loop]
                        while let Some(index) = effects.next_inner(&expansion, &mut cursor).await? {
                            effects.add_inner(&mut expansion, index, ty, sign).await?;
                        }
                    }
                },
                Frame::Sequence { mut elements, sign, next_negative } => {
                    if let Some(ty) = effects.next_element(&mut elements).await? {
                        effects.push(&mut expansion, Frame::Sequence { elements, sign, next_negative }).await?;
                        effects.push(&mut expansion, Frame::Add(ty, sign)).await?;
                    } else if let Some(intersection) = next_negative {
                        let elements = effects.negative_elements(&expansion, intersection).await?;
                        effects.push(&mut expansion, Frame::Sequence { elements, sign: Sign::Negative, next_negative: None }).await?;
                    }
                }
                Frame::Distribute(distribution) => {
                    let Distribution { mut elements, next_negative, sign, distributed, check_budget, has_disjunction, clone_aliases } = distribution;
                    if let Some(ty) = effects.next_element(&mut elements).await? {
                        let parent = effects.branch(&mut expansion, clone_aliases).await?;
                        effects.push(&mut expansion, Frame::Extend {
                            distribution: Distribution { elements, next_negative, sign, distributed, check_budget, has_disjunction, clone_aliases }, parent,
                        }).await?;
                        effects.push(&mut expansion, Frame::Add(ty, sign)).await?;
                    } else if let Some(intersection) = next_negative {
                        // All negative constraints end up becoming positive constraints.
                        let elements = effects.negative_elements(&expansion, intersection).await?;
                        effects.push(&mut expansion, Frame::Distribute(Distribution {
                            elements, next_negative: None, sign: Sign::Positive, distributed,
                            check_budget, has_disjunction, clone_aliases,
                        })).await?;
                    } else {
                        effects.install(&mut expansion, distributed, has_disjunction).await?;
                    }
                }
                Frame::Extend { distribution, mut parent } => {
                    let Distribution { elements, next_negative, sign, mut distributed, check_budget, has_disjunction, clone_aliases } = distribution;
                    let has_disjunction = if clone_aliases {
                        effects.restore_aliases(&mut expansion, &mut parent).await?;
                        let branch_has_disjunction = effects.has_disjunction(&expansion).await?;
                        has_disjunction || branch_has_disjunction
                    } else {
                        has_disjunction
                    };
                    if let ControlFlow::Break(value) = effects.extend(&mut expansion, &parent, &mut distributed, check_budget).await? {
                        effects.finish_failed(expansion, parent, distributed).await?;
                        return Ok(ControlFlow::Break(value));
                    }
                    effects.restore(&mut expansion, parent).await?;
                    effects.push(&mut expansion, Frame::Distribute(Distribution {
                        elements, next_negative, sign, distributed, check_budget, has_disjunction, clone_aliases,
                    })).await?;
                }
            }
        }
        effects.finish(expansion).await?;
        Ok(ControlFlow::Continue(()))
    }
}

impl<'db, L: IntersectionLimits> SynchronousExpansionEffects<'db> for OrdinaryExpansionEffects<L> {
    type Error = Infallible;
    type Break = L::Break;

    fn start<'a>(
        &self,
        builder: &'a mut IntersectionBuilder<'db>,
        aliases: &'a mut Vec<Type<'db>>,
        initial: Frame<'a, 'db>,
    ) -> Result<Expansion<'a, 'db>, Self::Error> {
        Ok(Expansion::new(builder, aliases, initial))
    }

    fn next<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
    ) -> Result<Option<Frame<'a, 'db>>, Self::Error> {
        Ok(expansion.next_frame())
    }

    fn push<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
        frame: Frame<'a, 'db>,
    ) -> Result<(), Self::Error> {
        expansion.push_frame(frame);
        Ok(())
    }

    fn finish(&self, expansion: Expansion<'_, 'db>) -> Result<(), Self::Error> {
        drop(expansion);
        Ok(())
    }

    fn finish_failed(
        &self,
        expansion: Expansion<'_, 'db>,
        parent: Parent<'_, 'db>,
        distributed: DistributionSet<'db>,
    ) -> Result<(), Self::Error> {
        drop(distributed);
        drop(parent);
        drop(expansion);
        Ok(())
    }

    fn seen_alias(
        &self,
        expansion: &Expansion<'_, 'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(expansion.seen_alias(ty))
    }

    fn remember_alias(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        expansion.remember_alias(ty);
        Ok(())
    }

    fn resolve_alias(
        &self,
        expansion: &Expansion<'_, 'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(expansion.builder.get().db))
    }

    fn union_elements(
        &self,
        expansion: &Expansion<'_, 'db>,
        union: UnionType<'db>,
    ) -> Result<Elements<'db>, Self::Error> {
        Ok(Elements::Union(
            union.elements(expansion.builder.get().db).iter(),
        ))
    }

    fn positive_elements(
        &self,
        expansion: &Expansion<'_, 'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<Elements<'db>, Self::Error> {
        Ok(Elements::Positive(
            intersection.positive(expansion.builder.get().db).iter(),
        ))
    }

    fn negative_elements(
        &self,
        expansion: &Expansion<'_, 'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<Elements<'db>, Self::Error> {
        Ok(Elements::Negative(
            intersection.negative(expansion.builder.get().db).iter(),
        ))
    }

    fn signed_element_count(
        &self,
        expansion: &Expansion<'_, 'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<usize, Self::Error> {
        let db = expansion.builder.get().db;
        Ok(intersection.positive(db).len() + intersection.negative(db).len())
    }

    fn next_element(&self, elements: &mut Elements<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(match elements {
            Elements::Union(elements) => elements.next().copied(),
            Elements::Positive(elements) => elements.next().copied(),
            Elements::Negative(elements) => elements.next().copied(),
        })
    }

    fn enum_intersection(
        &self,
        expansion: &Expansion<'_, 'db>,
        complement: EnumComplement<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        let builder = expansion.builder.get();
        Ok(complement.to_intersection(builder.db, &builder.env))
    }

    fn new_distribution(&self) -> Result<DistributionSet<'db>, Self::Error> {
        Ok(DistributionSet::default())
    }

    fn has_disjunction(&self, expansion: &Expansion<'_, 'db>) -> Result<bool, Self::Error> {
        Ok(expansion.builder().has_disjunction())
    }

    fn branch<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
        clone_aliases: bool,
    ) -> Result<Parent<'a, 'db>, Self::Error> {
        Ok(expansion.branch(clone_aliases))
    }

    fn extend(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        parent: &Parent<'_, 'db>,
        distributed: &mut DistributionSet<'db>,
        check_budget: bool,
    ) -> Result<ControlFlow<Self::Break>, Self::Error> {
        let parent = parent.builder();
        intersection_distribution::extend_sync(
            expansion.builder_mut(),
            distributed,
            check_budget,
            DistributionFacts,
            &OrdinaryDistributionEffects::<L>::new(parent.db, &parent.env),
        )
    }

    fn restore_aliases<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
        parent: &mut Parent<'a, 'db>,
    ) -> Result<(), Self::Error> {
        expansion.restore_aliases(parent);
        Ok(())
    }

    fn restore<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
        parent: Parent<'a, 'db>,
    ) -> Result<(), Self::Error> {
        expansion.restore(parent);
        Ok(())
    }

    fn install(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        distributed: DistributionSet<'db>,
        has_disjunction: bool,
    ) -> Result<(), Self::Error> {
        expansion.install(distributed, has_disjunction);
        Ok(())
    }

    fn next_inner(
        &self,
        expansion: &Expansion<'_, 'db>,
        cursor: &mut usize,
    ) -> Result<Option<usize>, Self::Error> {
        Ok(expansion.next_inner(cursor))
    }

    fn insert_recursive(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        index: usize,
        ty: Type<'db>,
        sign: Sign,
    ) -> Result<(), Self::Error> {
        let inner = &mut expansion.builder.get_mut().intersections[index];
        match sign {
            Sign::Positive => {
                inner.insert_signed(super::intersection_insertion::Sign::Positive, ty);
            }
            Sign::Negative => {
                inner.insert_signed(super::intersection_insertion::Sign::Negative, ty);
            }
        }
        Ok(())
    }

    fn add_inner(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        index: usize,
        ty: Type<'db>,
        sign: Sign,
    ) -> Result<(), Self::Error> {
        let builder = expansion.builder.get_mut();
        let inner = &mut builder.intersections[index];
        match sign {
            Sign::Positive => inner.add_positive(builder.db, &builder.env, ty),
            Sign::Negative => inner.add_negative(builder.db, &builder.env, ty),
        }
        Ok(())
    }
}
