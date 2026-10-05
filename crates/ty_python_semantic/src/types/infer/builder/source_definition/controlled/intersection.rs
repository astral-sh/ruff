//! Controlled signed insertion uses the shared order and admitted builder primitives.

mod assembly;
mod enums;
mod expansion;
mod finalization;

use std::alloc::Layout;

use salsa::execution_probe::{RunError, RunResult};

use super::storage::StorageQuote;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::set_theoretic::builder::intersection_insertion::{
    Elements, Frame, Insertion, InsertionEffects, InsertionFacts, Sign, add_with,
};
use crate::types::set_theoretic::builder::intersection_storage::SignedSetStorage;
use crate::types::set_theoretic::builder::{
    InnerIntersectionBuilder, IntersectionPolarity, IntersectionSimplification,
};
use crate::types::set_theoretic::generic_gradual_intersections::GenericIntersection;
pub(super) use crate::types::storage_quote::{buffer_push_quote, buffer_retirement};
use crate::types::{
    ClassLiteral, ClassType, EnumLiteralType, IntersectionType, KnownClass, NominalInstanceType,
    Type, TypeFormType,
};

fn quotation<T>(value: Option<T>) -> RunResult<T> {
    value.ok_or(RunError::Contract(
        "intersection insertion quotation overflow",
    ))
}

fn backing_bytes(storage: SignedSetStorage) -> Option<usize> {
    storage
        .table_slots
        .checked_mul(size_of::<usize>().checked_add(1)?)?
        .checked_add(
            storage
                .dense_capacity
                .checked_mul(size_of::<(usize, Type<'_>)>())?,
        )
}

fn retirement_work(storage: SignedSetStorage) -> Option<usize> {
    backing_bytes(storage)?
        .checked_add(storage.len.checked_mul(size_of::<Type<'_>>())?)?
        .checked_add(4)
}

fn lookup_work(storage: SignedSetStorage, incoming_bytes: usize) -> Option<usize> {
    let comparison = size_of::<Type<'_>>()
        .checked_mul(2)?
        .checked_add(incoming_bytes)?
        .checked_add(storage.max_inline_bytes)?
        .checked_add(8)?;
    storage
        .table_slots
        .checked_add(1)?
        .checked_mul(comparison)?
        .checked_add(incoming_bytes)?
        .checked_add(4)
}

fn insertion_quote(
    old: SignedSetStorage,
    bound: SignedSetStorage,
    incoming_bytes: usize,
) -> Option<StorageQuote> {
    let grows = bound.has_table && (!old.has_table || old.len.checked_add(1)? > old.capacity);
    let mut work = lookup_work(old, incoming_bytes)?
        .checked_add(lookup_work(bound, incoming_bytes)?)?
        .checked_add(size_of::<Type<'_>>().checked_mul(3)?)?
        .checked_add(16)?;
    if bound.has_table {
        // Tombstones can require rebuilding the index without allocating. Each retained
        // cached-hash entry may probe the whole table while finding its replacement slot.
        work = work.checked_add(
            old.len.checked_mul(
                bound
                    .table_slots
                    .checked_add(size_of::<(usize, Type<'_>)>())?
                    .checked_add(old.max_inline_bytes)?,
            )?,
        )?;
    }
    if !old.has_table && old.len != 0 {
        // Single-to-Multiple promotion compares the two types before hashing both into a
        // new set. Its resident insertion can perform another full-payload collision check.
        work = work.checked_add(lookup_work(bound, old.max_inline_bytes)?)?;
    }
    let bytes = if grows { backing_bytes(bound)? } else { 0 };
    if grows {
        // Growth can move every cached-hash entry and rebuild its index. The same admission
        // prepays retiring the resulting allocation if a later semantic child refuses.
        work = work
            .checked_add(retirement_work(old)?)?
            .checked_add(bytes.checked_mul(2)?)?;
        Layout::from_size_align(bytes, align_of::<(usize, Type<'_>)>().max(16)).ok()?;
    }
    Some(StorageQuote { work, bytes })
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn new_inner_intersection(
        &self,
    ) -> RunResult<InnerIntersectionBuilder<'db>> {
        self.local(
            size_of::<InnerIntersectionBuilder<'db>>() * 2 + 1,
            0,
            || InnerIntersectionBuilder::default(),
        )
        .await
    }

    pub(in crate::types::infer) async fn inner_intersection_add(
        &self,
        env: &ProgramEnvironment<'db>,
        builder: &mut InnerIntersectionBuilder<'db>,
        ty: Type<'db>,
        sign: Sign,
    ) -> RunResult<()> {
        self.environment_program(env).await?;
        self.allocate_future(|| add_with(builder, ty, sign, InsertionFacts, self))
            .await?
            .await
    }

    pub(in crate::types::infer) async fn retire_inner_intersection(
        &self,
        builder: InnerIntersectionBuilder<'db>,
    ) -> RunResult<()> {
        let (positive, negative) = self
            .local(2, 0, || {
                (
                    builder.signed_storage(Sign::Positive),
                    builder.signed_storage(Sign::Negative),
                )
            })
            .await?;
        let work = quotation(
            retirement_work(positive)
                .and_then(|work| work.checked_add(retirement_work(negative)?))
                .and_then(|work| work.checked_add(size_of::<InnerIntersectionBuilder<'db>>())),
        )?;
        // Keep the builder in this future until its disposal has passed admission.
        self.work(work).await?;
        drop(builder);
        Ok(())
    }

    async fn insertion_lookup_work(
        &self,
        insertion: &Insertion<'_, 'db>,
        sign: Sign,
        ty: Type<'db>,
    ) -> RunResult<usize> {
        let (storage, incoming_bytes) = self
            .local(2, 0, || {
                (insertion.signed_storage(sign), ty.inline_payload_bytes())
            })
            .await?;
        quotation(lookup_work(storage, incoming_bytes))
    }

    async fn signed_insertion_quote(
        &self,
        old: SignedSetStorage,
        ty: Type<'db>,
    ) -> RunResult<StorageQuote> {
        let (bound, incoming_bytes) = self
            .local(3, 0, || {
                (old.insertion_bound(ty), ty.inline_payload_bytes())
            })
            .await?;
        quotation(insertion_quote(old, quotation(bound)?, incoming_bytes))
    }

    async fn inner_intersection_insert(
        &self,
        builder: &mut InnerIntersectionBuilder<'db>,
        ty: Type<'db>,
        sign: Sign,
    ) -> RunResult<()> {
        let storage = self.local(1, 0, || builder.signed_storage(sign)).await?;
        let quote = self.signed_insertion_quote(storage, ty).await?;
        self.local(quote.work, quote.bytes, || builder.insert_signed(sign, ty))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InsertionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn start<'a>(
        &self,
        builder: &'a mut InnerIntersectionBuilder<'db>,
        initial: Frame<'db>,
    ) -> RunResult<Insertion<'a, 'db>> {
        let mut initial = Some(initial);
        self.local(size_of::<Insertion<'a, 'db>>() * 2 + 1, 0, || {
            let initial = initial.take().ok_or(RunError::Contract(
                "intersection initial frame already consumed",
            ))?;
            Ok(Insertion::new(builder, initial))
        })
        .await?
    }

    async fn next(&self, insertion: &mut Insertion<'_, 'db>) -> RunResult<Option<Frame<'db>>> {
        self.local(size_of::<Frame<'db>>() + 1, 0, || insertion.next_frame())
            .await
    }

    async fn push(&self, insertion: &mut Insertion<'_, 'db>, frame: Frame<'db>) -> RunResult<()> {
        let storage = self.local(1, 0, || insertion.frames_storage()).await?;
        let quote = quotation(buffer_push_quote::<Frame<'db>>(storage))?;
        let mut frame = Some(frame);
        self.local(quote.work, quote.bytes, || {
            let frame = frame.take().ok_or(RunError::Contract(
                "intersection pending frame already consumed",
            ))?;
            insertion.push_frame(frame);
            Ok(())
        })
        .await?
    }

    async fn finish(&self, insertion: Insertion<'_, 'db>) -> RunResult<()> {
        let (frames, removals) = self
            .local(2, 0, || {
                (insertion.frames_storage(), insertion.removals_storage())
            })
            .await?;
        let work = quotation(buffer_retirement::<Frame<'db>>(frames).and_then(|work| {
            work.checked_add(buffer_retirement::<usize>(removals)?)
                .and_then(|work| work.checked_add(size_of::<Insertion<'_, 'db>>()))
        }))?;
        self.work(work).await?;
        drop(insertion);
        Ok(())
    }

    async fn contains(
        &self,
        insertion: &Insertion<'_, 'db>,
        sign: Sign,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let work = self.insertion_lookup_work(insertion, sign, ty).await?;
        self.local(work, 0, || insertion.contains_signed(sign, ty))
            .await
    }

    async fn remove(
        &self,
        insertion: &mut Insertion<'_, 'db>,
        sign: Sign,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let storage = self.local(1, 0, || insertion.signed_storage(sign)).await?;
        let lookup = self.insertion_lookup_work(insertion, sign, ty).await?;
        let work = quotation(lookup.checked_add(quotation(retirement_work(storage))?))?;
        self.local(work, 0, || insertion.remove_signed(sign, ty))
            .await
    }

    async fn remove_index(
        &self,
        insertion: &mut Insertion<'_, 'db>,
        sign: Sign,
        index: usize,
    ) -> RunResult<()> {
        let storage = self.local(1, 0, || insertion.signed_storage(sign)).await?;
        let work = quotation(retirement_work(storage))?;
        self.local(work, 0, || insertion.remove_signed_index(sign, index))
            .await
    }

    async fn insert(
        &self,
        insertion: &mut Insertion<'_, 'db>,
        sign: Sign,
        ty: Type<'db>,
    ) -> RunResult<()> {
        let old = self.local(1, 0, || insertion.signed_storage(sign)).await?;
        let quote = self.signed_insertion_quote(old, ty).await?;
        self.local(quote.work, quote.bytes, || {
            insertion.insert_signed(sign, ty)
        })
        .await
    }

    async fn reset(&self, insertion: &mut Insertion<'_, 'db>, ty: Type<'db>) -> RunResult<()> {
        let (positive, negative, empty, incoming_bytes) = self
            .local(5, 0, || {
                (
                    insertion.signed_storage(Sign::Positive),
                    insertion.signed_storage(Sign::Negative),
                    InnerIntersectionBuilder::default().signed_storage(Sign::Positive),
                    ty.inline_payload_bytes(),
                )
            })
            .await?;
        let bound = self.local(1, 0, || empty.insertion_bound(ty)).await?;
        let mut quote = quotation(insertion_quote(empty, quotation(bound)?, incoming_bytes))?;
        quote.work = quotation(
            quote
                .work
                .checked_add(quotation(retirement_work(positive))?)
                .and_then(|work| work.checked_add(retirement_work(negative)?)),
        )?;
        self.local(quote.work, quote.bytes, || insertion.reset_to(ty))
            .await
    }

    async fn next_stored(
        &self,
        insertion: &Insertion<'_, 'db>,
        sign: Sign,
        cursor: &mut usize,
    ) -> RunResult<Option<(usize, Type<'db>)>> {
        self.local(size_of::<Type<'db>>() + 2, 0, || {
            insertion.next_signed(sign, cursor)
        })
        .await
    }

    async fn assert_divergent_alone(&self, insertion: &Insertion<'_, 'db>) -> RunResult<()> {
        self.local(1, 0, || {
            debug_assert_eq!(insertion.positive_len(), 1, "`Divergent` should be alone");
        })
        .await
    }

    async fn clear_removals(&self, insertion: &mut Insertion<'_, 'db>) -> RunResult<()> {
        let storage = self.local(1, 0, || insertion.removals_storage()).await?;
        let work = quotation(buffer_retirement::<usize>(storage))?;
        self.local(work, 0, || insertion.clear_removals()).await
    }

    async fn defer_removal(
        &self,
        insertion: &mut Insertion<'_, 'db>,
        index: usize,
    ) -> RunResult<()> {
        let storage = self.local(1, 0, || insertion.removals_storage()).await?;
        let quote = quotation(buffer_push_quote::<usize>(storage))?;
        self.local(quote.work, quote.bytes, || insertion.defer_removal(index))
            .await
    }

    async fn next_removal(&self, insertion: &mut Insertion<'_, 'db>) -> RunResult<Option<usize>> {
        self.local(2, 0, || insertion.next_removal()).await
    }

    async fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(3, 0, || intersection.field_requests(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.positive())
            .await?;
        let positive = self.field(request).await?;
        self.local_with_fixed_transfers(6, 0, || Elements::Positive(positive.iter()))
            .await
    }

    async fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(3, 0, || intersection.field_requests(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.negative())
            .await?;
        let negative = self.field(request).await?;
        self.local_with_fixed_transfers(6, 0, || Elements::Negative(negative.iter()))
            .await
    }

    async fn next_element(&self, elements: &mut Elements<'db>) -> RunResult<Option<Type<'db>>> {
        self.local_with_fixed_transfers(6, 0, || match elements {
            Elements::Positive(elements) => elements.next().copied(),
            Elements::Negative(elements) => elements.next().copied(),
        })
        .await
    }

    async fn empty_string(&self) -> RunResult<Type<'db>> {
        self.access.string_literal("").await
    }

    async fn typeform_argument(&self, typeform: TypeFormType<'db>) -> RunResult<Type<'db>> {
        self.field(
            typeform
                .field_requests(self.access.endpoint().field_request_context())
                .type_argument(),
        )
        .await
    }

    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        NominalSelectionEffects::resolve_alias(self, ty).await
    }

    async fn subclass_from_instance(&self, _ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn known_instance(&self, class: KnownClass) -> RunResult<Type<'db>> {
        self.access.known_class_instance(self.program, class).await
    }

    async fn has_known_class(
        &self,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> RunResult<bool> {
        let known = InsertionEffects::known_class(self, instance).await?;
        self.local(1, 0, || known == Some(class)).await
    }

    async fn known_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        let literal = InsertionEffects::instance_class(self, instance).await?;
        match literal {
            ClassLiteral::Static(class) => {
                self.field(
                    class
                        .field_requests(self.access.endpoint().field_request_context())
                        .known(),
                )
                .await
            }
            _ => self.local(1, 0, || None).await,
        }
    }

    async fn enum_class(&self, _literal: EnumLiteralType<'db>) -> RunResult<ClassLiteral<'db>> {
        self.unavailable(SourceOperation::Narrowing).await
    }

    async fn instance_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<ClassLiteral<'db>> {
        let class = NominalSelectionEffects::instance_class(self, instance).await?;
        match class {
            ClassType::NonGeneric(literal) => self.local(1, 0, || literal).await,
            ClassType::Generic(alias) => {
                let origin = self
                    .field(
                        alias
                            .field_requests(self.access.endpoint().field_request_context())
                            .origin(),
                    )
                    .await?;
                self.local(1, 0, || ClassLiteral::Static(origin)).await
            }
        }
    }

    async fn types_equal(&self, first: Type<'db>, second: Type<'db>) -> RunResult<bool> {
        let work = self
            .local(2, 0, || {
                first
                    .inline_payload_bytes()
                    .checked_add(second.inline_payload_bytes())
                    .and_then(|work| work.checked_add(size_of::<Type<'db>>() * 2 + 1))
            })
            .await?;
        self.local(quotation(work)?, 0, || first == second).await
    }

    async fn generic_intersection(
        &self,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Option<GenericIntersection<'db>>> {
        self.generic_intersection_source(
            &ProgramEnvironment::from_program(self.program),
            first,
            second,
        )
        .await
    }

    async fn simplify_pair(
        &self,
        first: Type<'db>,
        second: Type<'db>,
        polarity: IntersectionPolarity,
    ) -> RunResult<IntersectionSimplification> {
        self.access
            .simplify_intersection_pair(first, second, polarity)
            .await
    }
}
