//! Controlled finalization retains the supplied environment and the shared decision order.

use std::alloc::Layout;

use salsa::execution_probe::{RunError, RunResult};
use smallvec::SmallVec;

use super::{
    SourceAccess, SourceEffects, SourceOperation, StorageQuote, buffer_push_quote,
    buffer_retirement, insertion_quote, quotation,
};
use crate::types::enums::EnumComplement;
use crate::types::relation::source::subtyping_condition;
use crate::types::set_theoretic::builder::intersection_distribution_storage::inner_profile;
#[cfg(test)]
use crate::types::set_theoretic::builder::intersection_finalization::finalization_observations;
use crate::types::set_theoretic::builder::intersection_finalization::{
    FinalizationEffects, FinalizationFacts, RemainingConstraints, build_with,
    simplify_constrained_typevars_with,
};
use crate::types::set_theoretic::builder::intersection_insertion::Sign;
use crate::types::set_theoretic::builder::intersection_speculation::{self, SpeculationEffects};
use crate::types::set_theoretic::builder::intersection_storage::SignedSetStorage;
use crate::types::set_theoretic::builder::{InnerIntersectionBuilder, IntersectionBuilder};
use crate::types::typevar::{TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints};
use crate::types::{
    BoundTypeVarInstance, IntersectionType, NegativeIntersectionElements, NewType, Type,
    TypeVarBoundOrConstraints,
};
use crate::{FxOrderSet, ProgramEnvironment};

pub(super) struct FinalizationAdapter<'a, 'access, 'run, 'db: 'run, A> {
    pub(super) source: &'a SourceEffects<'access, 'run, 'db, A>,
    pub(super) env: &'a ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SpeculationEffects<'db>
    for FinalizationAdapter<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn new_intersection(&self) -> RunResult<IntersectionBuilder<'db>> {
        self.source.new_intersection(self.env).await
    }

    async fn next_positive(
        &self,
        positive: &FxOrderSet<Type<'db>>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(size_of::<Type<'db>>() + 2, 0, || {
                let next = positive.get_index(*cursor).copied();
                if next.is_some() {
                    *cursor += 1;
                }
                next
            })
            .await
    }

    async fn next_negative(
        &self,
        negative: &NegativeIntersectionElements<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(size_of::<Type<'db>>() + 2, 0, || {
                let next = match negative {
                    NegativeIntersectionElements::Empty => None,
                    NegativeIntersectionElements::Single(ty) => {
                        if *cursor == 0 {
                            Some(*ty)
                        } else {
                            None
                        }
                    }
                    NegativeIntersectionElements::Multiple(elements) => {
                        elements.get_index(*cursor).copied()
                    }
                };
                if next.is_some() {
                    *cursor += 1;
                }
                next
            })
            .await
    }

    async fn require_bounds(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarBoundOrConstraints<'db>> {
        if let Some(bounds) = self
            .source
            .intersection_typevar_bounds(typevar, self.env)
            .await?
        {
            return Ok(bounds);
        }
        let fields = self.source.access.endpoint().field_request_context();
        let identity = self.source.field(typevar.identity_request(fields)).await?;
        let kind = self
            .source
            .field(identity.identity.field_requests(fields).kind())
            .await?;
        if self
            .source
            .local(2, 0, || {
                kind.is_paramspec() && identity.paramspec_attr.is_none()
            })
            .await?
        {
            return self
                .source
                .unavailable(SourceOperation::TypeVarDomainTop)
                .await;
        }
        Ok(TypeVarBoundOrConstraints::UpperBound(Type::object()))
    }

    async fn constraints_as_type(
        &self,
        _constraints: TypeVarConstraints<'db>,
    ) -> RunResult<Type<'db>> {
        self.source
            .unavailable(SourceOperation::TypeVarConstraintUnion)
            .await
    }

    async fn newtype_base(&self, _newtype: NewType<'db>) -> RunResult<Type<'db>> {
        self.source.unavailable(SourceOperation::NewTypeBase).await
    }

    async fn add_positive(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.source.intersection_add_positive(builder, ty).await
    }

    async fn add_negative(
        &self,
        builder: &mut IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.source.intersection_add_negative(builder, ty).await
    }

    async fn build(&self, builder: &mut IntersectionBuilder<'db>) -> RunResult<Type<'db>> {
        self.source.intersection_build(builder).await
    }
}

fn constraint_allocation_quote(len: usize) -> Option<StorageQuote> {
    let bytes = len.checked_mul(size_of::<Option<Type<'_>>>())?;
    Layout::array::<Option<Type<'_>>>(len).ok()?;
    Some(StorageQuote {
        work: bytes
            .checked_mul(3)?
            .checked_add(size_of::<RemainingConstraints<'_>>().checked_mul(2)?)?
            .checked_add(4)?,
        bytes,
    })
}

fn shrink_quote(storage: SignedSetStorage) -> Option<StorageQuote> {
    let dense_bytes = storage
        .dense_capacity
        .max(storage.table_slots)
        .checked_mul(size_of::<(usize, Type<'_>)>())?;
    let bytes = storage
        .table_slots
        .checked_mul(size_of::<usize>().checked_add(1)?)?
        .checked_add(dense_bytes)?;
    Layout::from_size_align(bytes, align_of::<(usize, Type<'_>)>().max(16)).ok()?;
    // Shrinking can replace both allocations and reinsert every cached index hash.
    // Retained backing also pays for cleanup if interning subsequently refuses.
    let work = bytes
        .checked_mul(3)?
        .checked_add(
            storage.len.checked_mul(
                storage
                    .table_slots
                    .checked_add(size_of::<(usize, Type<'_>)>())?
                    .checked_add(8)?,
            )?,
        )?
        .checked_add(8)?;
    Some(StorageQuote { work, bytes })
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn intersection_expand(
        &self,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        let fields = intersection.field_requests(self.access.endpoint().field_request_context());
        let positive = self.field(fields.positive()).await?;
        let negative = self.field(fields.negative()).await?;
        let effects = FinalizationAdapter { source: self, env };
        self.allocate_future(|| intersection_speculation::expand_with(positive, negative, &effects))
            .await?
            .await
    }

    pub(in crate::types::infer) async fn inner_intersection_build(
        &self,
        env: &ProgramEnvironment<'db>,
        builder: &mut InnerIntersectionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        let profile = quotation(self.local(4, 0, || inner_profile(builder)).await?)?;
        self.work(profile.retirement_work).await?;
        let effects = FinalizationAdapter { source: self, env };
        self.allocate_future(|| {
            let builder = std::mem::take(builder);
            #[cfg(test)]
            let lifetime = finalization_observations::owner_ready(self.db());
            async move {
                #[cfg(test)]
                let _lifetime = lifetime;
                build_with(builder, FinalizationFacts, &effects).await
            }
        })
        .await?
        .await
    }

    pub(super) async fn intersection_typevar_bounds(
        &self,
        typevar: BoundTypeVarInstance<'db>,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        let fields = self.access.endpoint().field_request_context();
        let typevar = self.field(typevar.field_requests(fields).typevar()).await?;
        let bounds = self
            .field(typevar.bound_or_constraints_request(fields))
            .await?;
        match bounds {
            None => Ok(None),
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(bounds)) => Ok(Some(bounds)),
            Some(
                TypeVarBoundOrConstraintsEvaluation::LazyUpperBound
                | TypeVarBoundOrConstraintsEvaluation::LazyConstraints,
            ) => self.unavailable(SourceOperation::TypeVarBounds).await,
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FinalizationEffects<'db>
    for FinalizationAdapter<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn has_empty_enum_complement(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> RunResult<bool> {
        let (positive, negative) = self.source.local(1, 0, || builder.signed_parts()).await?;
        super::enums::has_empty_enum_complement(self.source, self.env, positive, negative).await
    }

    async fn simplify_constrained_typevars(
        &self,
        builder: &mut InnerIntersectionBuilder<'db>,
    ) -> RunResult<()> {
        self.source
            .allocate_future(|| simplify_constrained_typevars_with(builder, self))
            .await?
            .await
    }

    async fn new_additions(&self) -> RunResult<SmallVec<[Type<'db>; 1]>> {
        self.source
            .local(
                size_of::<SmallVec<[Type<'db>; 1]>>() * 2 + 1,
                0,
                SmallVec::new,
            )
            .await
    }

    async fn next_positive(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(size_of::<Type<'db>>() + 2, 0, || {
                builder
                    .next_signed(Sign::Positive, cursor)
                    .map(|(_, ty)| ty)
            })
            .await
    }

    async fn bound_or_constraints(
        &self,
        typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        self.source
            .intersection_typevar_bounds(typevar, self.env)
            .await
    }

    async fn constraint_elements(
        &self,
        constraints: TypeVarConstraints<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        let fields = self.source.access.endpoint().field_request_context();
        let elements = self
            .source
            .field(constraints.field_requests(fields).elements())
            .await?;
        self.source.local(1, 0, || elements.as_ref()).await
    }

    async fn remaining_constraints(
        &self,
        original: &'db [Type<'db>],
    ) -> RunResult<RemainingConstraints<'db>> {
        let len = self.source.local(1, 0, || original.len()).await?;
        let quote = quotation(constraint_allocation_quote(len))?;
        self.source
            .local(quote.work, quote.bytes, || {
                #[cfg(test)]
                {
                    let mut constraints = RemainingConstraints::new(original);
                    constraints.record(self.source.db());
                    constraints
                }
                #[cfg(not(test))]
                RemainingConstraints::new(original)
            })
            .await
    }

    async fn next_negative(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(size_of::<Type<'db>>() + 2, 0, || {
                builder
                    .next_signed(Sign::Negative, cursor)
                    .map(|(_, ty)| ty)
            })
            .await
    }

    async fn next_constraint(
        &self,
        constraints: &RemainingConstraints<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<(usize, Type<'db>)>> {
        self.source
            .local(size_of::<Type<'db>>() + 2, 0, || {
                constraints.next_original(cursor)
            })
            .await
    }

    async fn is_subtype(&self, constraint: Type<'db>, negative: Type<'db>) -> RunResult<bool> {
        subtyping_condition(
            self.source.db(),
            self.env,
            constraint,
            negative,
            self.source,
        )
        .await
    }

    async fn exclude_constraint(
        &self,
        constraints: &mut RemainingConstraints<'db>,
        index: usize,
    ) -> RunResult<()> {
        self.source
            .local(size_of::<Option<Type<'db>>>() + 1, 0, || {
                if constraints.exclude(index) {
                    Ok(())
                } else {
                    Err(RunError::Contract(
                        "intersection constraint index is out of bounds",
                    ))
                }
            })
            .await?
    }

    async fn next_remaining(
        &self,
        constraints: &RemainingConstraints<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<Option<Type<'db>>>> {
        self.source
            .local(size_of::<Option<Type<'db>>>() + 2, 0, || {
                constraints.next_remaining(cursor)
            })
            .await
    }

    async fn finish_constraints(&self, constraints: RemainingConstraints<'db>) -> RunResult<()> {
        let (len, capacity) = self.source.local(1, 0, || constraints.storage()).await?;
        let work = quotation(buffer_retirement::<Option<Type<'db>>>((
            len, capacity, true,
        )))?;
        self.source.work(work).await?;
        drop(constraints);
        Ok(())
    }

    async fn set_never(&self, builder: &mut InnerIntersectionBuilder<'db>) -> RunResult<()> {
        let (profile, empty) = self
            .source
            .local(5, 0, || {
                (
                    inner_profile(builder),
                    InnerIntersectionBuilder::default().signed_storage(Sign::Positive),
                )
            })
            .await?;
        let profile = quotation(profile)?;
        let bound = quotation(empty.insertion_bound(Type::Never))?;
        let mut quote = quotation(insertion_quote(empty, bound, 0))?;
        quote.work = quotation(quote.work.checked_add(profile.retirement_work))?;
        self.source
            .local(quote.work, quote.bytes, || builder.reset_to(Type::Never))
            .await
    }

    async fn queue_addition(
        &self,
        additions: &mut SmallVec<[Type<'db>; 1]>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let storage = self
            .source
            .local(1, 0, || {
                (additions.len(), additions.capacity(), additions.spilled())
            })
            .await?;
        let quote = quotation(buffer_push_quote::<Type<'db>>(storage))?;
        self.source
            .local(quote.work, quote.bytes, || additions.push(ty))
            .await
    }

    async fn next_addition(
        &self,
        additions: &SmallVec<[Type<'db>; 1]>,
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(size_of::<Type<'db>>() + 2, 0, || {
                let next = additions.get(*cursor).copied();
                if next.is_some() {
                    *cursor += 1;
                }
                next
            })
            .await
    }

    async fn finish_additions(&self, additions: SmallVec<[Type<'db>; 1]>) -> RunResult<()> {
        let storage = self
            .source
            .local(1, 0, || {
                (additions.len(), additions.capacity(), additions.spilled())
            })
            .await?;
        self.source
            .work(quotation(buffer_retirement::<Type<'db>>(storage))?)
            .await?;
        drop(additions);
        Ok(())
    }

    async fn add_positive(
        &self,
        builder: &mut InnerIntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.source
            .inner_intersection_add(self.env, builder, ty, Sign::Positive)
            .await
    }

    async fn expand_typevars_and_newtypes(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        let (positive, negative) = self.source.local(1, 0, || builder.signed_parts()).await?;
        self.source
            .allocate_future(|| intersection_speculation::expand_with(positive, negative, self))
            .await?
            .await
    }

    async fn is_singleton(&self, complement: EnumComplement<'db>) -> RunResult<bool> {
        super::enums::is_singleton(self.source, complement).await
    }

    async fn remaining_literal_union(
        &self,
        complement: EnumComplement<'db>,
    ) -> RunResult<Type<'db>> {
        super::enums::remaining_literal_union(self.source, self.env, complement).await
    }

    async fn enum_complement(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> RunResult<Option<EnumComplement<'db>>> {
        let (positive, negative) = self.source.local(1, 0, || builder.signed_parts()).await?;
        super::enums::enum_complement(self.source, self.env, positive, negative).await
    }

    async fn lengths(&self, builder: &InnerIntersectionBuilder<'db>) -> RunResult<(usize, usize)> {
        self.source.local(2, 0, || builder.signed_lengths()).await
    }

    async fn first_positive(
        &self,
        builder: &InnerIntersectionBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        self.source
            .local(size_of::<Type<'db>>() + 2, 0, || {
                builder
                    .next_signed(Sign::Positive, &mut 0)
                    .map(|(_, ty)| ty)
                    .ok_or(RunError::Contract(
                        "intersection singleton has no positive element",
                    ))
            })
            .await?
    }

    async fn shrink(&self, builder: &mut InnerIntersectionBuilder<'db>) -> RunResult<()> {
        let (positive, negative) = self
            .source
            .local(2, 0, || {
                (
                    builder.signed_storage(Sign::Positive),
                    builder.signed_storage(Sign::Negative),
                )
            })
            .await?;
        let quote = quotation(
            shrink_quote(positive).and_then(|quote| quote.checked_add(shrink_quote(negative)?)),
        )?;
        self.source
            .local(quote.work, quote.bytes, || builder.shrink_signed())
            .await
    }

    async fn intern(&self, builder: InnerIntersectionBuilder<'db>) -> RunResult<Type<'db>> {
        let profile = quotation(self.source.local(4, 0, || inner_profile(&builder)).await?)?;
        self.source.work(profile.retirement_work).await?;
        let mut builder = Some(builder);
        let (positive, negative) = self
            .source
            .local(
                size_of::<InnerIntersectionBuilder<'db>>() * 2 + 1,
                0,
                || {
                    builder
                        .take()
                        .map(InnerIntersectionBuilder::into_parts)
                        .ok_or(RunError::Contract(
                            "intersection interner owner already consumed",
                        ))
                },
            )
            .await??;
        self.source
            .access
            .intern_intersection(positive, negative)
            .await
            .map(Type::Intersection)
    }

    async fn finish(&self, builder: InnerIntersectionBuilder<'db>) -> RunResult<()> {
        let profile = quotation(self.source.local(4, 0, || inner_profile(&builder)).await?)?;
        self.source.work(profile.retirement_work).await?;
        drop(builder);
        Ok(())
    }
}
