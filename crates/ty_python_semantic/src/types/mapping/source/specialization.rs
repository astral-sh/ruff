use std::alloc::Layout;
use std::borrow::Cow;
use std::future::Future;

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};

use super::{MappingSourceEffects, RetainedMappingSource, SourceMapping};
use crate::types::generics::mapping::{
    ArgumentCursor, ArgumentMappingMode, CompositionEffects, MappingArguments,
    SpecializationArgumentEffects, SpecializationArgumentMapEffects, SpecializationMapEffects,
    argument_cursor, compose_specializations_with, map_specialization_arguments_with,
    map_specialization_with,
};
use crate::types::mapping::effects::{MappingOperation, SharedMappingEffects};
use crate::types::mapping::{MaterializationOperation, OwnedTypeMapping};
use crate::types::storage_quote::{StorageQuote, sequence_merge};
use crate::types::tuple::TupleType;
use crate::types::tuple::mapping::{TupleMappingFacts, map_tuple_with};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, GenericContext, MaterializationKind,
    Specialization, Type, TypeContext, TypeMapping,
};
use crate::{Db, Program};

pub(in crate::types) async fn compose_specialization_with_retained<
    'run,
    'owner: 'run,
    'env: 'owner,
    'db: 'run,
    R: RetainedMappingSource<'run, 'db>,
>(
    db: &'db dyn Db,
    base: Specialization<'db>,
    additional: Specialization<'db>,
    _program: Program<'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: R,
) -> RunResult<Specialization<'db>> {
    let access = source.effects();
    let endpoint = access.endpoint();
    let effects = endpoint
        .local_call(|| {
            admit(
                endpoint,
                StorageQuote {
                    work: 2,
                    bytes: size_of::<RetainedComposition<'_, 'owner, 'env, 'run, 'db, R>>() * 2,
                },
            )?;
            Ok(RetainedComposition {
                endpoint,
                visitor,
                source: &source,
            })
        })
        .await;
    #[cfg(test)]
    observations::root(base, additional, _program, visitor);
    compose_specializations_with(db, base, additional, visitor, &effects).await
}

struct RetainedComposition<'call, 'owner, 'env, 'run, 'db: 'run, R> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: &'call R,
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    CompositionEffects<'db> for RetainedComposition<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;

    async fn specialization_mapping(
        &self,
        additional: Specialization<'db>,
    ) -> RunResult<OwnedTypeMapping<'db, 'db>> {
        Ok(self
            .endpoint
            .local_call(|| {
                admit(
                    self.endpoint,
                    StorageQuote {
                        work: 1,
                        bytes: size_of::<OwnedTypeMapping<'db, 'db>>() * 2,
                    },
                )?;
                Ok(OwnedTypeMapping::Specialization {
                    specialization: additional,
                    specialize_self_domain: false,
                    materialization_kind: None,
                })
            })
            .await)
    }

    async fn materialization_mapping(
        &self,
        kind: MaterializationKind,
    ) -> RunResult<OwnedTypeMapping<'db, 'db>> {
        Ok(self
            .endpoint
            .local_call(|| {
                admit(
                    self.endpoint,
                    StorageQuote {
                        work: 1,
                        bytes: size_of::<OwnedTypeMapping<'db, 'db>>() * 2,
                    },
                )?;
                Ok(OwnedTypeMapping::Materialize(kind))
            })
            .await)
    }

    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<MaterializationKind>> {
        self.source
            .effects()
            .specialization_materialization_kind(db, specialization)
            .await
    }

    async fn map_pass(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: OwnedTypeMapping<'db, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Specialization<'db>> {
        map_specialization_with_retained(
            db,
            specialization,
            mapping,
            &[],
            visitor,
            self.visitor,
            self.source,
            self.endpoint,
        )
        .await
    }
}

pub(super) async fn map_specialization_with_retained<
    'run,
    'owner: 'run,
    'env: 'owner,
    'db: 'run,
    R: RetainedMappingSource<'run, 'db>,
>(
    db: &'db dyn Db,
    specialization: Specialization<'db>,
    mapping: OwnedTypeMapping<'run, 'db>,
    contexts: &[Type<'db>],
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    retained_visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: &R,
    endpoint: &TaskEndpoint<'run, 'db>,
) -> RunResult<Specialization<'db>> {
    let (effects, borrowed_mapping) = crate::types::local_transfer::local_with_fixed_transfers_at(
        endpoint, 4, 0, || {
            if !std::ptr::addr_eq(
                std::ptr::from_ref(visitor),
                std::ptr::from_ref(retained_visitor),
            ) {
                return Err(RunError::Contract("specialization visitor is not retained"));
            }
            Ok((
                RetainedSpecialization {
                    endpoint,
                    visitor: retained_visitor,
                    source,
                    mapping,
                },
                mapping.into_mapping(),
            ))
        },
    ).await??;
    effects.child(|| map_specialization_with(
        db,
        specialization,
        &borrowed_mapping,
        contexts,
        retained_visitor,
        &effects,
    )).await
}

struct RetainedSpecialization<'call, 'owner, 'env, 'run, 'db: 'run, R> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    visitor: &'owner ApplyTypeMappingVisitor<'env, 'db>,
    source: &'call R,
    mapping: OwnedTypeMapping<'run, 'db>,
}

fn checked<T>(value: Option<T>) -> RunResult<T> {
    value.ok_or(RunError::Contract(
        "specialization storage quotation overflow",
    ))
}

/// Funds copied entries, possible relocation, and retirement while retaining Vec's growth policy.
fn argument_growth_quote<'db>(len: usize, capacity: usize, incoming: usize) -> RunResult<StorageQuote> {
    let mut quote = checked(sequence_merge::<Type<'db>>(len, capacity, incoming))?;
    if quote.bytes != 0 {
        let requested_capacity = checked(capacity.checked_mul(2))?
            .max(checked(len.checked_add(incoming))?).max(4);
        quote.bytes = Layout::array::<Type<'db>>(requested_capacity)
            .map_err(|_| RunError::Contract("specialization growth layout overflow"))?.size();
        quote.work = checked(quote.work.checked_add(len).and_then(|work| work.checked_add(incoming)).and_then(|work| work.checked_add(4)))?;
    }
    Ok(quote)
}

fn admit(endpoint: &TaskEndpoint<'_, '_>, quote: StorageQuote) -> RunResult<()> {
    endpoint.admit_work(quote.work)?;
    if quote.bytes != 0 {
        endpoint.admit(ExecutionWork::Resource {
            requested_bytes: quote.bytes,
        })?;
    }
    endpoint.check_completion()
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    RetainedSpecialization<'_, 'owner, 'env, 'run, 'db, R>
{
    async fn local<T>(
        &self,
        work: usize,
        bytes: usize,
        operation: impl FnOnce() -> RunResult<T>,
    ) -> RunResult<T> {
        crate::types::local_transfer::local_with_fixed_transfers_at(
            self.endpoint, work, bytes, operation,
        ).await?
    }

    /// Admits a selected mapping future while retaining captured buffers through refused admission.
    async fn child<T, F: Future<Output = RunResult<T>>>(&self, make: impl FnOnce() -> F) -> RunResult<T> {
        let future = crate::types::local_transfer::boxed_future_with_fixed_transfers_at(
            self.endpoint, Ok((0, 0)), make,
        ).await?;
        future.await
    }

    async fn unavailable<T>(&self, operation: MappingOperation) -> RunResult<T> {
        self.child(|| async {
            self.source.effects().unavailable(self.mapping, MaterializationOperation::Leaf(operation)).await
        }).await
    }

    async fn child_view(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<SourceMapping<'_, 'owner, 'env, 'run, 'db, R>> {
        self.local(2, 0, || {
                if !std::ptr::addr_eq(
                    std::ptr::from_ref(visitor),
                    std::ptr::from_ref(self.visitor),
                ) {
                    return Err(RunError::Contract("specialization visitor is not retained"));
                }
                #[cfg(not(test))]
                let _ = (db, ty);
                Ok(SourceMapping {
                    endpoint: self.endpoint,
                    visitor: self.visitor,
                    source: self.source,
                    mapping: self.mapping,
                    #[cfg(test)]
                    input: (db, ty, self.mapping),
                })
            }).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SpecializationArgumentEffects<'db> for RetainedSpecialization<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;
    type Cursor = ArgumentCursor<'db>;
    type Buffer = Vec<Type<'db>>;

    async fn types(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        self.child(|| async { self.source.effects().specialization_types(db, specialization).await }).await
    }

    async fn entries(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        types: &'db [Type<'db>],
    ) -> RunResult<Self::Cursor> {
        let context = self.child(|| async { self.source.effects().specialization_context(db, specialization).await }).await?;
        let variables = self.child(|| async { self.source.effects().specialization_variables(db, context).await }).await?;
        self.local(4, size_of::<Self::Cursor>() * 2, || {
            Ok(argument_cursor(variables, types))
        })
        .await
    }

    async fn next(
        &self,
        cursor: &mut Self::Cursor,
    ) -> RunResult<Option<(usize, (BoundTypeVarInstance<'db>, Type<'db>))>> {
        self.local(
            8,
            size_of::<Self::Cursor>() * 2
                + size_of::<(BoundTypeVarInstance<'db>, Type<'db>)>() * 2,
            || Ok(cursor.next()),
        )
        .await
    }

    async fn different(&self, left: Type<'db>, right: Type<'db>) -> RunResult<bool> {
        let work = self.local(4, 0, || {
            checked(1usize
                        .checked_add(left.inline_payload_bytes())
                        .and_then(|work| work.checked_add(right.inline_payload_bytes())))
        }).await?;
        self.local(work, 0, || Ok(left != right)).await
    }

    async fn new_buffer(&self, original: &'db [Type<'db>]) -> RunResult<Self::Buffer> {
        let width = self.local(1, 0, || Ok(original.len())).await?;
        let (work, bytes) = self.local(8, 0, || {
                let bytes = Layout::array::<Type<'db>>(width)
                    .map_err(|_| RunError::Contract("specialization allocation layout overflow"))?
                    .size();
                let work = checked(width.checked_mul(2).and_then(|work| work.checked_add(4)))?;
                Ok((work, bytes))
        }).await?;
        self.local(work, bytes, || Ok(Vec::with_capacity(width))).await
    }

    async fn copy_prefix(
        &self,
        buffer: &mut Self::Buffer,
        original: &'db [Type<'db>],
        index: usize,
    ) -> RunResult<()> {
        let (len, capacity) = self
            .local(2, 0, || Ok((buffer.len(), buffer.capacity())))
            .await?;
        let quote = self.local(12, 0, || {
            argument_growth_quote(len, capacity, index)
        }).await?;
        self.local(quote.work, quote.bytes, || {
                let prefix = original.get(..index).ok_or(RunError::Contract(
                    "specialization prefix is outside the original arguments",
                ))?;
                buffer.extend_from_slice(prefix);
                Ok(())
            }).await
    }

    async fn append(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> RunResult<()> {
        let quote = self.local(12, 0, || {
            argument_growth_quote(buffer.len(), buffer.capacity(), 1)
        }).await?;
        self.local(quote.work, quote.bytes, || {
                buffer.push(ty);
                Ok(())
            }).await
    }

    async fn retain_buffer(
        &self,
        target: &mut Option<Self::Buffer>,
        buffer: Self::Buffer,
    ) -> RunResult<()> {
        let mut owner = Some(buffer);
        self.local(1, size_of::<Option<Self::Buffer>>() * 2, || {
            if target.is_some() {
                return Err(RunError::Contract(
                    "specialization argument buffer already retained",
                ));
            }
            *target = owner.take();
            Ok(())
        })
        .await
    }

    async fn finish(
        &self,
        original: &'db [Type<'db>],
        buffer: Option<Self::Buffer>,
    ) -> RunResult<Cow<'db, [Type<'db>]>> {
        let mut owner = buffer;
        self.local(1, size_of::<Cow<'db, [Type<'db>]>>() * 2, || {
            Ok(owner.take().map(Cow::Owned).unwrap_or(Cow::Borrowed(original)))
        })
        .await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SpecializationMapEffects<'db> for RetainedSpecialization<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;
    async fn materialization(
        &self,
        mapping: &TypeMapping<'_, 'db>,
    ) -> RunResult<Option<MaterializationKind>> {
        self.local(2, size_of::<TypeMapping<'_, 'db>>(), || {
            Ok(match mapping {
                TypeMapping::Materialize(kind) => Some(*kind),
                _ => None,
            })
        })
        .await
    }
    async fn materialize(
        &self,
        _db: &'db dyn Db,
        _specialization: Specialization<'db>,
        _kind: MaterializationKind,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Specialization<'db>> {
        self.unavailable(MappingOperation::MaterializationOrPolarity)
            .await
    }
    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<MaterializationKind>> {
        self.child(|| async { self.source.effects().specialization_materialization_kind(db, specialization).await }).await
    }
    async fn map_arguments(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: &TypeMapping<'_, 'db>,
        contexts: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        kind: &mut Option<MaterializationKind>,
    ) -> RunResult<Cow<'db, [Type<'db>]>> {
        let mut mapper = self
            .local(
                2,
                size_of::<MappingArguments<'_, '_, '_, 'db, Self>>() * 2,
                || {
                    Ok(MappingArguments {
                        db,
                        effects: self,
                        mapping,
                        contexts,
                        visitor,
                        kind,
                    })
                },
            )
            .await?;
        self.child(|| map_specialization_arguments_with(db, specialization, &mut mapper, self)).await
    }
    async fn tuple_inner(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<TupleType<'db>>> {
        self.child(|| async { self.source.effects().specialization_tuple(db, specialization).await }).await
    }
    async fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<TupleType<'db>> {
        let (ty, context) = self
            .local(
                4,
                size_of::<Type<'db>>() * 2 + size_of::<TypeContext<'db>>() * 2,
                || Ok((Type::tuple(tuple), TypeContext::default())),
            )
            .await?;
        let effects = self.child_view(db, ty, visitor).await?;
        self.child(|| map_tuple_with(
            db,
            tuple,
            mapping,
            context,
            self.visitor,
            &effects,
            TupleMappingFacts,
        )).await
    }
    async fn arguments_borrowed(&self, types: &Cow<'db, [Type<'db>]>) -> RunResult<bool> {
        self.local(1, 0, || Ok(matches!(types, Cow::Borrowed(_))))
            .await
    }
    async fn same_tuple(
        &self,
        left: Option<TupleType<'db>>,
        right: Option<TupleType<'db>>,
    ) -> RunResult<bool> {
        self.local(1, size_of::<Option<TupleType<'db>>>() * 2, || {
            Ok(left == right)
        })
        .await
    }
    async fn same_kind(
        &self,
        left: Option<MaterializationKind>,
        right: Option<MaterializationKind>,
    ) -> RunResult<bool> {
        self.local(3, 0, || Ok(left == right)).await
    }
    async fn payload(&self, types: &Cow<'db, [Type<'db>]>) -> RunResult<()> {
        self.local(1, size_of::<Cow<'db, [Type<'db>]>>() * 2, || {
            let _ = types;
            Ok(())
        })
        .await
    }
    async fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        self.child(|| async { self.source.effects().specialization_context(db, specialization).await }).await
    }
    async fn intern(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'db, [Type<'db>]>,
        kind: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> RunResult<Specialization<'db>> {
        self.child(|| async { self.source.effects().intern_mapped_specialization(db, context, types, kind, tuple).await }).await
    }
}

impl<'run, 'owner: 'run, 'env: 'owner, 'db: 'run, R: RetainedMappingSource<'run, 'db>>
    SpecializationArgumentMapEffects<'db>
    for RetainedSpecialization<'_, 'owner, 'env, 'run, 'db, R>
{
    type Error = RunError;
    async fn argument_context(
        &self,
        contexts: &[Type<'db>],
        index: usize,
    ) -> RunResult<TypeContext<'db>> {
        self.local(3, size_of::<TypeContext<'db>>() * 2, || {
            Ok(TypeContext::new(contexts.get(index).copied()))
        })
        .await
    }
    async fn mode(&self, mapping: &TypeMapping<'_, 'db>) -> RunResult<ArgumentMappingMode> {
        self.local(3, size_of::<TypeMapping<'_, 'db>>(), || {
            Ok(ArgumentMappingMode::classify(mapping))
        })
        .await
    }
    async fn covariant(
        &self,
        _db: &'db dyn Db,
        _variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.unavailable(MappingOperation::MaterializationOrPolarity)
            .await
    }
    async fn copy_mapping<'a>(
        &self,
        _mapping: &TypeMapping<'a, 'db>,
    ) -> RunResult<TypeMapping<'a, 'db>> {
        self.unavailable(MappingOperation::MaterializationOrPolarity)
            .await
    }
    async fn flip_mapping<'a>(
        &self,
        _mapping: &TypeMapping<'a, 'db>,
    ) -> RunResult<TypeMapping<'a, 'db>> {
        self.unavailable(MappingOperation::MaterializationOrPolarity)
            .await
    }
    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        let effects = self.child_view(db, ty, visitor).await?;
        self.child(|| SharedMappingEffects::map_type(&effects, db, ty, mapping, context, visitor)).await
    }
    async fn polarity_argument(
        &self,
        _db: &'db dyn Db,
        _variable: BoundTypeVarInstance<'db>,
        _ty: Type<'db>,
        _mapping: &TypeMapping<'_, 'db>,
        _context: TypeContext<'db>,
        _visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        _kind: &mut Option<MaterializationKind>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(MappingOperation::MaterializationOrPolarity)
            .await
    }
}

#[cfg(test)]
pub(in crate::types) mod observations {
    use super::*;
    use salsa::plumbing::AsId;
    use std::cell::Cell;

    #[derive(Clone, Copy, Debug)]
    pub(in crate::types) struct Root {
        pub(in crate::types) base: salsa::Id,
        pub(in crate::types) additional: salsa::Id,
        pub(in crate::types) program: salsa::Id,
        pub(in crate::types) visitor: usize,
        pub(in crate::types) environment: usize,
    }
    thread_local! {
        static ROOT: Cell<Option<Root>> = const { Cell::new(None) };
    }
    pub(in crate::types) fn reset() {
        ROOT.set(None);
    }
    pub(in crate::types) fn snapshot() -> Option<Root> {
        ROOT.get()
    }
    pub(super) fn root(
        base: Specialization<'_>,
        additional: Specialization<'_>,
        program: Program<'_>,
        visitor: &ApplyTypeMappingVisitor<'_, '_>,
    ) {
        ROOT.set(Some(Root {
            base: base.as_id(),
            additional: additional.as_id(),
            program: program.as_id(),
            visitor: std::ptr::from_ref(visitor).addr(),
            environment: std::ptr::from_ref(visitor.env).addr(),
        }));
    }
}
