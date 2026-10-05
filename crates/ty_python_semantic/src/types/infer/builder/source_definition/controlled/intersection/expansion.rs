//! Outer expansion retains parents and distributed branches across admitted operations.

use std::alloc::Layout;
use std::convert::Infallible;
use std::ops::ControlFlow;

use salsa::execution_probe::{RunError, RunResult};

use super::{
    SourceAccess, SourceEffects, SourceOperation, StorageQuote, buffer_push_quote,
    buffer_retirement, lookup_work, quotation,
};
use crate::ProgramEnvironment;
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::enums::EnumComplement;
use crate::types::set_theoretic::builder::intersection_assembly::IntersectionBranches;
use crate::types::set_theoretic::builder::intersection_distribution::{
    DistributionEffects, DistributionFacts, extend_with,
};
use crate::types::set_theoretic::builder::intersection_distribution_storage::{
    DistributionSet, DistributionStorage, InnerProfile, RemovalIndices,
    inner_profile as retained_inner_profile,
};
use crate::types::set_theoretic::builder::intersection_expansion::{
    Elements, Expansion, ExpansionEffects, ExpansionFacts, Frame, Parent, Sign, add_with,
};
use crate::types::set_theoretic::builder::intersection_insertion::Sign as InnerSign;
use crate::types::set_theoretic::builder::{InnerIntersectionBuilder, IntersectionBuilder};
use crate::types::visitor::runtime::TypeSliceDeref;
use crate::types::{IntersectionType, RecursivelyDefined, Type, UnionType};

#[derive(Clone, Copy, Default)]
struct OwnerProfile {
    cleanup: usize,
    clone_work: usize,
    clone_bytes: usize,
}

impl OwnerProfile {
    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            cleanup: self.cleanup.checked_add(other.cleanup)?,
            clone_work: self.clone_work.checked_add(other.clone_work)?,
            clone_bytes: self.clone_bytes.checked_add(other.clone_bytes)?,
        })
    }
}

fn inner_sign(sign: Sign) -> InnerSign {
    match sign {
        Sign::Positive => InnerSign::Positive,
        Sign::Negative => InnerSign::Negative,
    }
}

fn distribution_lookup_work(storage: DistributionStorage, incoming: InnerProfile) -> Option<usize> {
    storage
        .table_slots
        .checked_add(1)?
        .checked_mul(
            incoming
                .key_work
                .checked_add(storage.max_key_work)?
                .checked_add(8)?,
        )?
        .checked_add(incoming.key_work)?
        .checked_add(4)
}

fn distribution_insert_quote(
    old: DistributionStorage,
    bound: DistributionStorage,
    incoming: InnerProfile,
) -> Option<StorageQuote> {
    let mut work = distribution_lookup_work(old, incoming)?
        .checked_add(distribution_lookup_work(bound, incoming)?)?
        .checked_add(incoming.retirement_work)?
        .checked_add(size_of::<InnerIntersectionBuilder<'_>>().checked_mul(3)?)?
        .checked_add(16)?;
    // Index rebuilding after removals can occur without allocating. Ordered entries cache
    // their hashes, but each retained index may still probe the whole replacement table.
    work = work.checked_add(
        old.len.checked_mul(
            bound
                .table_slots
                .checked_add(size_of::<(usize, InnerIntersectionBuilder<'_>)>())?
                .checked_add(8)?,
        )?,
    )?;
    let bytes = if old.len.checked_add(1)? > old.capacity {
        let bytes = bound.backing_bytes()?;
        work = work
            .checked_add(old.backing_bytes()?)?
            .checked_add(bytes.checked_mul(2)?)?;
        Layout::from_size_align(
            bytes,
            align_of::<(usize, InnerIntersectionBuilder<'_>)>().max(16),
        )
        .ok()?;
        bytes
    } else {
        0
    };
    Some(StorageQuote { work, bytes })
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn union_elements_source(
        &self,
        union: UnionType<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(3, 0, || union.field_requests(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.elements())
            .await?;
        self.field_with_profile(request, &TypeSliceDeref).await
    }

    pub(in crate::types::infer::builder::source_definition::controlled) async fn union_recursion_source(
        &self,
        union: UnionType<'db>,
    ) -> RunResult<RecursivelyDefined> {
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(3, 0, || union.field_requests(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.recursively_defined())
            .await?;
        self.field(request).await
    }

    pub(in crate::types::infer) async fn new_intersection(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<IntersectionBuilder<'db>> {
        self.environment_program(env).await?;
        // The initial branch has an empty positive set, an empty negative enum, and six
        // zeroed storage counters. Prepay its retirement and the outer Vec's deallocation
        // before a later child can stop with the builder retained.
        let construction_work = 48;
        let cleanup_work = 24;
        let bytes = size_of::<InnerIntersectionBuilder<'db>>();
        self.local_with_fixed_transfers(construction_work + cleanup_work, bytes, || {
            IntersectionBuilder::new(self.db(), env)
        })
        .await
    }

    pub(in crate::types::infer) async fn intersection_add_positive(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.intersection_add_signed(builder, ty, Sign::Positive)
            .await
    }

    pub(in crate::types::infer) async fn intersection_add_negative(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.intersection_add_signed(builder, ty, Sign::Negative)
            .await
    }

    async fn intersection_add_signed(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
        sign: Sign,
    ) -> RunResult<()> {
        let env = self.local(1, 0, || builder.environment()).await?;
        self.environment_program(env).await?;
        let mut aliases = self
            .local(size_of::<Vec<Type<'db>>>() + 1, 0, Vec::new)
            .await?;
        let ControlFlow::Continue(()) = self
            .allocate_future(|| add_with(builder, ty, sign, &mut aliases, ExpansionFacts, self))
            .await?
            .await?;
        let profile = self.alias_profile(&aliases, aliases.capacity()).await?;
        self.work(profile.cleanup).await?;
        drop(aliases);
        Ok(())
    }

    pub(in crate::types::infer) async fn retire_intersection(
        &self,
        builder: IntersectionBuilder<'db>,
    ) -> RunResult<()> {
        let profile = self.intersection_profile(&builder).await?;
        self.work(profile.cleanup).await?;
        drop(builder);
        Ok(())
    }

    async fn inner_profile(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> RunResult<OwnerProfile> {
        let profile = quotation(self.local(4, 0, || retained_inner_profile(builder)).await?)?;
        Layout::from_size_align(
            profile.clone_bytes,
            align_of::<(usize, Type<'db>)>().max(16),
        )
        .map_err(|_| RunError::Contract("intersection inner clone layout overflow"))?;
        Ok(OwnerProfile {
            cleanup: profile.retirement_work,
            clone_work: profile.clone_work,
            clone_bytes: profile.clone_bytes,
        })
    }

    async fn branch_profile(
        &self,
        branches: &[InnerIntersectionBuilder<'db>],
        capacity: usize,
    ) -> RunResult<OwnerProfile> {
        let len = self.local(1, 0, || branches.len()).await?;
        let bytes = quotation(len.checked_mul(size_of::<InnerIntersectionBuilder<'db>>()))?;
        Layout::array::<InnerIntersectionBuilder<'db>>(len)
            .map_err(|_| RunError::Contract("intersection clone layout overflow"))?;
        let mut profile = OwnerProfile {
            cleanup: quotation(capacity.checked_mul(size_of::<InnerIntersectionBuilder<'db>>()))?,
            clone_work: quotation(bytes.checked_mul(2))?,
            clone_bytes: bytes,
        };
        let mut cursor = 0;
        while let Some(inner) = self
            .local(2, 0, || {
                let result = branches.get(cursor);
                cursor += usize::from(result.is_some());
                result
            })
            .await?
        {
            profile = quotation(profile.checked_add(self.inner_profile(inner).await?))?;
        }
        Ok(profile)
    }

    async fn intersection_profile(
        &self,
        builder: &IntersectionBuilder<'db>,
    ) -> RunResult<OwnerProfile> {
        let (branches, capacity) = self.local(1, 0, || builder.branches_storage()).await?;
        let profile = self.branch_profile(branches, capacity).await?;
        quotation(profile.checked_add(OwnerProfile {
            cleanup: size_of::<IntersectionBuilder<'db>>() + 1,
            clone_work: size_of::<IntersectionBuilder<'db>>() * 2 + 1,
            clone_bytes: 0,
        }))
    }

    async fn alias_profile(
        &self,
        aliases: &[Type<'db>],
        capacity: usize,
    ) -> RunResult<OwnerProfile> {
        let len = self.local(1, 0, || aliases.len()).await?;
        Layout::array::<Type<'db>>(len)
            .map_err(|_| RunError::Contract("intersection alias clone layout overflow"))?;
        let bytes = quotation(len.checked_mul(size_of::<Type<'db>>()))?;
        Ok(OwnerProfile {
            cleanup: quotation(
                capacity
                    .checked_mul(size_of::<Type<'db>>())
                    .and_then(|work| work.checked_add(bytes))
                    .and_then(|work| work.checked_add(size_of::<Vec<Type<'db>>>())),
            )?,
            clone_work: quotation(bytes.checked_mul(3).and_then(|work| work.checked_add(4)))?,
            clone_bytes: bytes,
        })
    }

    async fn parent_cleanup(&self, parent: &Parent<'_, 'db>) -> RunResult<usize> {
        let builder = self.local(1, 0, || parent.owned_builder()).await?;
        let mut cleanup = size_of::<Parent<'_, 'db>>();
        if let Some(builder) = builder {
            cleanup =
                quotation(cleanup.checked_add(self.intersection_profile(builder).await?.cleanup))?;
        }
        let aliases = self.local(1, 0, || parent.owned_aliases_storage()).await?;
        if let Some((aliases, capacity)) = aliases {
            cleanup = quotation(
                cleanup.checked_add(self.alias_profile(aliases, capacity).await?.cleanup),
            )?;
        }
        Ok(cleanup)
    }

    async fn frame_cleanup(&self, frame: &Frame<'_, 'db>) -> RunResult<usize> {
        let mut cleanup = size_of::<Frame<'_, 'db>>();
        if let Some(parent) = self.local(1, 0, || frame.parent()).await? {
            cleanup = quotation(cleanup.checked_add(self.parent_cleanup(parent).await?))?;
        }
        if let Some(distribution) = self.local(1, 0, || frame.distribution()).await? {
            cleanup =
                quotation(cleanup.checked_add(self.distribution_cleanup(distribution).await?))?;
        }
        Ok(cleanup)
    }

    async fn expansion_cleanup(&self, expansion: &Expansion<'_, 'db>) -> RunResult<usize> {
        let storage = self.local(1, 0, || expansion.frames_storage()).await?;
        let mut cleanup = quotation(
            buffer_retirement::<Frame<'_, 'db>>(storage)
                .and_then(|work| work.checked_add(size_of::<Expansion<'_, 'db>>())),
        )?;
        if let Some(builder) = self.local(1, 0, || expansion.owned_builder()).await? {
            cleanup =
                quotation(cleanup.checked_add(self.intersection_profile(builder).await?.cleanup))?;
        }
        if let Some((aliases, capacity)) = self
            .local(1, 0, || expansion.owned_aliases_storage())
            .await?
        {
            cleanup = quotation(
                cleanup.checked_add(self.alias_profile(aliases, capacity).await?.cleanup),
            )?;
        }
        let mut cursor = 0;
        while let Some(frame) = self
            .local(2, 0, || {
                let frame = expansion.frame(cursor);
                cursor += usize::from(frame.is_some());
                frame
            })
            .await?
        {
            cleanup = quotation(cleanup.checked_add(self.frame_cleanup(frame).await?))?;
        }
        Ok(cleanup)
    }

    async fn distribution_cleanup(&self, distributed: &DistributionSet<'db>) -> RunResult<usize> {
        let storage = self.local(1, 0, || distributed.storage()).await?;
        quotation(storage.retirement_work())
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ExpansionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Break = Infallible;

    async fn start<'a>(
        &self,
        builder: &'a mut IntersectionBuilder<'db>,
        aliases: &'a mut Vec<Type<'db>>,
        initial: Frame<'a, 'db>,
    ) -> RunResult<Expansion<'a, 'db>> {
        let cleanup = self.frame_cleanup(&initial).await?;
        let work = quotation(cleanup.checked_add(size_of::<Expansion<'a, 'db>>() * 2 + 1))?;
        let mut initial = Some(initial);
        self.local(work, 0, || {
            let initial = initial.take().ok_or(RunError::Contract(
                "expansion initial frame already consumed",
            ))?;
            Ok(Expansion::new(builder, aliases, initial))
        })
        .await?
    }

    async fn next<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
    ) -> RunResult<Option<Frame<'a, 'db>>> {
        self.local(size_of::<Frame<'a, 'db>>() + 1, 0, || {
            expansion.next_frame()
        })
        .await
    }

    async fn push<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
        frame: Frame<'a, 'db>,
    ) -> RunResult<()> {
        let cleanup = self.frame_cleanup(&frame).await?;
        let storage = self.local(1, 0, || expansion.frames_storage()).await?;
        let mut quote = quotation(buffer_push_quote::<Frame<'a, 'db>>(storage))?;
        quote.work = quotation(quote.work.checked_add(cleanup))?;
        let mut frame = Some(frame);
        self.local(quote.work, quote.bytes, || {
            let frame = frame.take().ok_or(RunError::Contract(
                "expansion pending frame already consumed",
            ))?;
            expansion.push_frame(frame);
            Ok(())
        })
        .await?
    }

    async fn finish(&self, expansion: Expansion<'_, 'db>) -> RunResult<()> {
        let cleanup = self.expansion_cleanup(&expansion).await?;
        self.work(cleanup).await?;
        drop(expansion);
        Ok(())
    }

    async fn finish_failed(
        &self,
        expansion: Expansion<'_, 'db>,
        parent: Parent<'_, 'db>,
        distributed: DistributionSet<'db>,
    ) -> RunResult<()> {
        let expansion_cleanup = self.expansion_cleanup(&expansion).await?;
        let parent_cleanup = self.parent_cleanup(&parent).await?;
        let distributed_cleanup = self.distribution_cleanup(&distributed).await?;
        self.work(quotation(
            expansion_cleanup
                .checked_add(parent_cleanup)
                .and_then(|work| work.checked_add(distributed_cleanup)),
        )?)
        .await?;
        drop(distributed);
        drop(parent);
        drop(expansion);
        Ok(())
    }

    async fn seen_alias(&self, expansion: &Expansion<'_, 'db>, ty: Type<'db>) -> RunResult<bool> {
        let (aliases, _) = self.local(1, 0, || expansion.aliases_storage()).await?;
        let incoming = self.local(1, 0, || ty.inline_payload_bytes()).await?;
        let mut work = 1usize;
        let mut cursor = 0;
        while let Some(payload) = self
            .local(2, 0, || {
                let payload = aliases.get(cursor).map(|ty| ty.inline_payload_bytes());
                cursor += usize::from(payload.is_some());
                payload
            })
            .await?
        {
            work = quotation(
                work.checked_add(payload)
                    .and_then(|work| work.checked_add(incoming))
                    .and_then(|work| work.checked_add(size_of::<Type<'db>>() * 2 + 1)),
            )?;
        }
        self.local(work, 0, || expansion.seen_alias(ty)).await
    }

    async fn remember_alias(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let storage = self
            .local(2, 0, || {
                let (aliases, capacity) = expansion.aliases_storage();
                (aliases.len(), capacity, true)
            })
            .await?;
        let quote = quotation(buffer_push_quote::<Type<'db>>(storage))?;
        self.local(quote.work, quote.bytes, || expansion.remember_alias(ty))
            .await
    }

    async fn resolve_alias(
        &self,
        _expansion: &Expansion<'_, 'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        NominalSelectionEffects::resolve_alias(self, ty).await
    }

    async fn union_elements(
        &self,
        _expansion: &Expansion<'_, 'db>,
        union: UnionType<'db>,
    ) -> RunResult<Elements<'db>> {
        let elements = self.union_elements_source(union).await?;
        self.local(size_of::<Elements<'db>>() + 1, 0, || {
            Elements::Union(elements.iter())
        })
        .await
    }

    async fn positive_elements(
        &self,
        _expansion: &Expansion<'_, 'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        let elements = self
            .field(
                intersection
                    .field_requests(self.access.endpoint().field_request_context())
                    .positive(),
            )
            .await?;
        self.local(size_of::<Elements<'db>>() + 1, 0, || {
            Elements::Positive(elements.iter())
        })
        .await
    }

    async fn negative_elements(
        &self,
        _expansion: &Expansion<'_, 'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        let elements = self
            .field(
                intersection
                    .field_requests(self.access.endpoint().field_request_context())
                    .negative(),
            )
            .await?;
        self.local(size_of::<Elements<'db>>() + 1, 0, || {
            Elements::Negative(elements.iter())
        })
        .await
    }

    async fn signed_element_count(
        &self,
        _expansion: &Expansion<'_, 'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<usize> {
        let fields = intersection.field_requests(self.access.endpoint().field_request_context());
        let positive = self.field(fields.positive()).await?;
        let negative = self.field(fields.negative()).await?;
        quotation(
            self.local(3, 0, || positive.len().checked_add(negative.len()))
                .await?,
        )
    }

    async fn next_element(&self, elements: &mut Elements<'db>) -> RunResult<Option<Type<'db>>> {
        self.local(size_of::<Type<'db>>() + 1, 0, || match elements {
            Elements::Union(elements) => elements.next().copied(),
            Elements::Positive(elements) => elements.next().copied(),
            Elements::Negative(elements) => elements.next().copied(),
        })
        .await
    }

    async fn enum_intersection(
        &self,
        _expansion: &Expansion<'_, 'db>,
        _complement: EnumComplement<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn new_distribution(&self) -> RunResult<DistributionSet<'db>> {
        self.local(
            size_of::<DistributionSet<'db>>() * 2 + 1,
            0,
            DistributionSet::default,
        )
        .await
    }

    async fn has_disjunction(&self, expansion: &Expansion<'_, 'db>) -> RunResult<bool> {
        self.local(1, 0, || expansion.builder().has_disjunction())
            .await
    }

    async fn branch<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
        clone_aliases: bool,
    ) -> RunResult<Parent<'a, 'db>> {
        let builder = self.local(1, 0, || expansion.builder()).await?;
        let mut profile = self.intersection_profile(builder).await?;
        if clone_aliases {
            let (aliases, capacity) = self.local(1, 0, || expansion.aliases_storage()).await?;
            profile = quotation(profile.checked_add(self.alias_profile(aliases, capacity).await?))?;
        }
        let work = quotation(
            profile
                .clone_work
                .checked_add(size_of::<Parent<'a, 'db>>() * 2 + 1),
        )?;
        self.local(work, profile.clone_bytes, || {
            expansion.branch(clone_aliases)
        })
        .await
    }

    async fn restore_aliases<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
        parent: &mut Parent<'a, 'db>,
    ) -> RunResult<()> {
        let restores = self.local(1, 0, || parent.restores_aliases()).await?;
        let mut work = size_of::<Vec<Type<'db>>>() * 2 + 1;
        if restores
            && let Some((aliases, capacity)) = self
                .local(1, 0, || expansion.owned_aliases_storage())
                .await?
        {
            work =
                quotation(work.checked_add(self.alias_profile(aliases, capacity).await?.cleanup))?;
        }
        self.local(work, 0, || expansion.restore_aliases(parent))
            .await
    }

    async fn extend(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        _parent: &Parent<'_, 'db>,
        distributed: &mut DistributionSet<'db>,
        check_budget: bool,
    ) -> RunResult<ControlFlow<Self::Break>> {
        let builder = self.local(1, 0, || expansion.builder_mut()).await?;
        self.allocate_future(|| {
            extend_with(builder, distributed, check_budget, DistributionFacts, self)
        })
        .await?
        .await
    }

    async fn restore<'a>(
        &self,
        expansion: &mut Expansion<'a, 'db>,
        parent: Parent<'a, 'db>,
    ) -> RunResult<()> {
        let mut work = size_of::<Parent<'a, 'db>>() * 2 + 1;
        if let Some(builder) = self.local(1, 0, || expansion.owned_builder()).await? {
            work = quotation(work.checked_add(self.intersection_profile(builder).await?.cleanup))?;
        }
        if self.local(1, 0, || parent.restores_aliases()).await?
            && let Some((aliases, capacity)) = self
                .local(1, 0, || expansion.owned_aliases_storage())
                .await?
        {
            work =
                quotation(work.checked_add(self.alias_profile(aliases, capacity).await?.cleanup))?;
        }
        let mut parent = Some(parent);
        self.local(work, 0, || {
            let parent = parent
                .take()
                .ok_or(RunError::Contract("expansion parent already consumed"))?;
            expansion.restore(parent);
            Ok(())
        })
        .await?
    }

    async fn install(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        distributed: DistributionSet<'db>,
        has_disjunction: bool,
    ) -> RunResult<()> {
        let old = self.local(1, 0, || expansion.builder()).await?;
        let cleanup = self.intersection_profile(old).await?.cleanup;
        let distribution_cleanup = self.distribution_cleanup(&distributed).await?;
        let len = self.local(1, 0, || distributed.len()).await?;
        // IndexSet's external iterator does not promise TrustedLen to Vec. Collecting its
        // first element can therefore reserve Vec's minimum nonzero capacity as well.
        let capacity = if len == 0 { 0 } else { len.max(4) };
        let bytes = quotation(capacity.checked_mul(size_of::<InnerIntersectionBuilder<'db>>()))?;
        Layout::array::<InnerIntersectionBuilder<'db>>(capacity)
            .map_err(|_| RunError::Contract("intersection installation layout overflow"))?;
        let work = quotation(
            cleanup
                .checked_add(distribution_cleanup)
                .and_then(|work| work.checked_add(bytes.checked_mul(3)?))
                .and_then(|work| work.checked_add(4)),
        )?;
        let mut distributed = Some(distributed);
        self.local(work, bytes, || {
            let distributed = distributed.take().ok_or(RunError::Contract(
                "intersection distribution already consumed",
            ))?;
            expansion.install(distributed, has_disjunction);
            Ok(())
        })
        .await?
    }

    async fn next_inner(
        &self,
        expansion: &Expansion<'_, 'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<usize>> {
        self.local(2, 0, || expansion.next_inner(cursor)).await
    }

    async fn insert_recursive(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        index: usize,
        ty: Type<'db>,
        sign: Sign,
    ) -> RunResult<()> {
        let inner = self
            .local(1, 0, || expansion.inner_with_environment(index))
            .await?
            .ok_or(RunError::Contract("recursive insertion branch is missing"))?;
        self.inner_intersection_insert(inner.1, ty, inner_sign(sign))
            .await
    }

    async fn add_inner(
        &self,
        expansion: &mut Expansion<'_, 'db>,
        index: usize,
        ty: Type<'db>,
        sign: Sign,
    ) -> RunResult<()> {
        let (env, inner) = self
            .local(1, 0, || expansion.inner_with_environment(index))
            .await?
            .ok_or(RunError::Contract(
                "intersection insertion branch is missing",
            ))?;
        self.inner_intersection_add(env, inner, ty, inner_sign(sign))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DistributionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Break = Infallible;

    async fn bounded(&self) -> RunResult<bool> {
        self.local(1, 0, || false).await
    }

    async fn take_branches(
        &self,
        child: &mut IntersectionBuilder<'db>,
    ) -> RunResult<IntersectionBranches<'db>> {
        let profile = self.intersection_profile(child).await?;
        let work = quotation(
            profile
                .cleanup
                .checked_add(size_of::<IntersectionBranches<'db>>() * 2 + 1),
        )?;
        self.local(work, 0, || child.take_branches()).await
    }

    async fn next_candidate(
        &self,
        branches: &mut IntersectionBranches<'db>,
    ) -> RunResult<Option<InnerIntersectionBuilder<'db>>> {
        self.local(size_of::<InnerIntersectionBuilder<'db>>() + 1, 0, || {
            branches.next()
        })
        .await
    }

    async fn finish_branches(&self, branches: IntersectionBranches<'db>) -> RunResult<()> {
        let (remaining, capacity) = self.local(1, 0, || branches.storage()).await?;
        let profile = self.branch_profile(remaining, capacity).await?;
        self.work(quotation(
            profile
                .cleanup
                .checked_add(size_of::<IntersectionBranches<'db>>() + 1),
        )?)
        .await?;
        drop(branches);
        Ok(())
    }

    async fn contains_never(&self, candidate: &InnerIntersectionBuilder<'db>) -> RunResult<bool> {
        let storage = self
            .local(1, 0, || candidate.signed_storage(InnerSign::Positive))
            .await?;
        let work = quotation(lookup_work(storage, 0))?;
        self.local(work, 0, || {
            candidate.contains_signed(InnerSign::Positive, Type::Never)
        })
        .await
    }

    async fn build_candidate(
        &self,
        _candidate: &InnerIntersectionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn next_old<'a>(
        &self,
        distributed: &'a DistributionSet<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<(usize, &'a InnerIntersectionBuilder<'db>)>> {
        self.local(2, 0, || distributed.next(cursor)).await
    }

    async fn is_redundant(&self, first: Type<'db>, second: Type<'db>) -> RunResult<bool> {
        self.access.is_redundant_with(first, second).await
    }

    async fn new_removals(&self) -> RunResult<RemovalIndices> {
        self.local(
            size_of::<RemovalIndices>() * 2 + 1,
            0,
            RemovalIndices::default,
        )
        .await
    }

    async fn defer_removal(&self, removals: &mut RemovalIndices, index: usize) -> RunResult<()> {
        let storage = self.local(1, 0, || removals.storage()).await?;
        let quote = quotation(buffer_push_quote::<usize>(storage))?;
        self.local(quote.work, quote.bytes, || removals.push(index))
            .await
    }

    async fn apply_removals(
        &self,
        distributed: &mut DistributionSet<'db>,
        removals: RemovalIndices,
    ) -> RunResult<()> {
        let (storage, indices) = self
            .local(2, 0, || (distributed.storage(), removals.storage()))
            .await?;
        let work = quotation(
            storage
                .retirement_work()
                .and_then(|work| work.checked_add(buffer_retirement::<usize>(indices)?))
                .and_then(|work| {
                    work.checked_add(
                        storage.len.checked_mul(
                            storage
                                .table_slots
                                .checked_add(size_of::<(usize, InnerIntersectionBuilder<'db>)>())?
                                .checked_add(8)?,
                        )?,
                    )
                }),
        )?;
        let mut removals = Some(removals);
        self.local(work, 0, || {
            let removals = removals
                .take()
                .ok_or(RunError::Contract("distribution removals already consumed"))?;
            distributed.apply_removals(removals);
            Ok(())
        })
        .await?
    }

    async fn check_terms(
        &self,
        distributed: &DistributionSet<'db>,
    ) -> RunResult<ControlFlow<Self::Break>> {
        quotation(
            self.local(2, 0, || distributed.len().checked_add(1))
                .await?,
        )?;
        Ok(ControlFlow::Continue(()))
    }

    async fn insert(
        &self,
        distributed: &mut DistributionSet<'db>,
        candidate: InnerIntersectionBuilder<'db>,
    ) -> RunResult<()> {
        let (old, incoming) = self
            .local(5, 0, || {
                (distributed.storage(), retained_inner_profile(&candidate))
            })
            .await?;
        let incoming = quotation(incoming)?;
        let bound = quotation(self.local(1, 0, || old.insertion_bound(incoming)).await?)?;
        let quote = quotation(distribution_insert_quote(old, bound, incoming))?;
        let mut candidate = Some(candidate);
        self.local(quote.work, quote.bytes, || {
            let candidate = candidate.take().ok_or(RunError::Contract(
                "distribution candidate already consumed",
            ))?;
            distributed.insert(candidate);
            Ok(())
        })
        .await?
    }

    async fn discard(&self, candidate: InnerIntersectionBuilder<'db>) -> RunResult<()> {
        self.retire_inner_intersection(candidate).await
    }
}
