use std::borrow::Cow;
use std::convert::Infallible;

use super::*;
use crate::ProgramEnvironment;
use crate::types::TypeVarVariance;
use crate::types::mapping::effects::{
    MappingEffects, MappingOperation, MappingWork, SynchronousMappingEffects,
};

pub(in crate::types::generics) struct MappingSpecializationEffects<'a, E>(
    pub(in crate::types::generics) &'a E,
);
pub(in crate::types::generics) struct AsyncCallback<F>(pub(in crate::types::generics) F);
pub(in crate::types::generics) struct SyncCallback<F>(pub(in crate::types::generics) F);

pub(in crate::types::generics) struct OrdinarySpecializationLookup<'db>(
    pub(in crate::types::generics) &'db dyn Db,
);

impl<'db> SynchronousSpecializationLookupEffects<'db> for OrdinarySpecializationLookup<'db> {
    type Error = Infallible;

    fn generic_context(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Infallible> {
        Ok(specialization.generic_context(self.0))
    }

    fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Infallible> {
        Ok(context.variables_inner(self.0))
    }

    fn identity(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Infallible> {
        Ok(variable.identity(self.0))
    }

    fn index(
        &self,
        variables: &'db ContextVariables<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> Result<Option<usize>, Infallible> {
        Ok(variables.get_index_of(&identity))
    }

    fn types(&self, specialization: Specialization<'db>) -> Result<&'db [Type<'db>], Infallible> {
        Ok(specialization.types(self.0))
    }

    fn type_at(
        &self,
        types: &'db [Type<'db>],
        index: usize,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(types.get(index).copied())
    }
}

impl<
    'db,
    Error,
    F: AsyncFnMut(usize, BoundTypeVarInstance<'db>, Type<'db>) -> Result<Type<'db>, Error>,
> SpecializationArgumentMapper<'db> for AsyncCallback<F>
{
    type Error = Error;
    async fn map(
        &mut self,
        index: usize,
        variable: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Error> {
        (self.0)(index, variable, ty).await
    }
}
impl<'db, Error, F: FnMut(usize, BoundTypeVarInstance<'db>, Type<'db>) -> Result<Type<'db>, Error>>
    SynchronousSpecializationArgumentMapper<'db> for SyncCallback<F>
{
    type Error = Error;
    fn map(
        &mut self,
        index: usize,
        variable: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Error> {
        (self.0)(index, variable, ty)
    }
}

fn polarity_argument<'db>(
    db: &'db dyn Db,
    typevar: BoundTypeVarInstance<'db>,
    ty: Type<'db>,
    type_mapping: &TypeMapping<'_, 'db>,
    tcx: TypeContext<'db>,
    visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    new_materialization_kind: &mut Option<MaterializationKind>,
) -> Type<'db> {
    match (typevar.variance(db), type_mapping) {
        (
            TypeVarVariance::Invariant,
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            },
        ) => {
            // An invariant type argument cannot be materialized in isolation. Keep the
            // specialized argument and record the materialization on this specialization.
            // Comparing both mappings distinguishes substituted gradual types from
            // unrelated gradual types already present in the argument.
            let specialized = ty.apply_type_mapping_impl(
                db,
                &TypeMapping::ApplySpecialization(*specialization),
                tcx,
                visitor,
            );

            if new_materialization_kind.is_none() {
                let materialized = ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor);
                if specialized != materialized {
                    *new_materialization_kind = Some(*materialization_kind);
                }
            }

            specialized
        }
        (variance, _) if variance.is_covariant() => {
            ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
        }
        _ => ty.apply_type_mapping_impl(db, &type_mapping.flip(), tcx, visitor),
    }
}

impl<'db, E: MappingEffects<'db>> CompositionStartEffects<'db>
    for MappingSpecializationEffects<'_, E>
{
    type Error = E::Error;
    type Environment = ProgramEnvironment<'db>;
    async fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(specialization.generic_context(db))
    }
    async fn program(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<Program<'db>, Self::Error> {
        Ok(context.program(db))
    }
    async fn environment(&self, program: Program<'db>) -> Result<Self::Environment, Self::Error> {
        Ok(ProgramEnvironment::from_program(program))
    }
    async fn compose_fresh(
        &self,
        db: &'db dyn Db,
        base: Specialization<'db>,
        additional: Specialization<'db>,
        env: &Self::Environment,
    ) -> Result<Specialization<'db>, Self::Error> {
        compose_specializations_with(
            db,
            base,
            additional,
            &ApplyTypeMappingVisitor::new(env),
            self,
        )
        .await
    }
}
impl<'db, E: MappingEffects<'db>> CompositionEffects<'db> for MappingSpecializationEffects<'_, E> {
    type Error = E::Error;
    async fn specialization_mapping(
        &self,
        additional: Specialization<'db>,
    ) -> Result<OwnedTypeMapping<'db, 'db>, Self::Error> {
        Ok(OwnedTypeMapping::Specialization {
            specialization: additional,
            specialize_self_domain: false,
            materialization_kind: None,
        })
    }
    async fn materialization_mapping(
        &self,
        kind: MaterializationKind,
    ) -> Result<OwnedTypeMapping<'db, 'db>, Self::Error> {
        Ok(OwnedTypeMapping::Materialize(kind))
    }
    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(db))
    }
    async fn map_pass(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: OwnedTypeMapping<'db, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        map_specialization_with(
            db,
            specialization,
            &mapping.into_mapping(),
            &[],
            visitor,
            self,
        )
        .await
    }
}
impl<'db, E: MappingEffects<'db>> SpecializationArgumentEffects<'db>
    for MappingSpecializationEffects<'_, E>
{
    type Error = E::Error;
    type Cursor = ArgumentCursor<'db>;
    type Buffer = Vec<Type<'db>>;
    async fn types(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(specialization.types(db))
    }
    async fn entries(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        types: &'db [Type<'db>],
    ) -> Result<Self::Cursor, Self::Error> {
        Ok(argument_cursor(
            specialization.generic_context(db).variables_inner(db),
            types,
        ))
    }
    async fn next(
        &self,
        cursor: &mut Self::Cursor,
    ) -> Result<Option<(usize, (BoundTypeVarInstance<'db>, Type<'db>))>, Self::Error> {
        self.0.checkpoint(MappingWork::ArgumentAdvance).await?;
        Ok(cursor.next())
    }
    async fn different(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error> {
        Ok(left != right)
    }
    async fn new_buffer(&self, original: &'db [Type<'db>]) -> Result<Self::Buffer, Self::Error> {
        self.0
            .checkpoint(MappingWork::ArgumentCapacity {
                width: original.len(),
            })
            .await?;
        Ok(Vec::with_capacity(original.len()))
    }
    async fn copy_prefix(
        &self,
        buffer: &mut Self::Buffer,
        original: &'db [Type<'db>],
        index: usize,
    ) -> Result<(), Self::Error> {
        self.0
            .checkpoint(MappingWork::ArgumentPrefixCopy { len: index })
            .await?;
        buffer.extend_from_slice(&original[..index]);
        Ok(())
    }
    async fn append(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Self::Error> {
        self.0.checkpoint(MappingWork::ArgumentAppend).await?;
        buffer.push(ty);
        Ok(())
    }
    async fn retain_buffer(
        &self,
        target: &mut Option<Self::Buffer>,
        buffer: Self::Buffer,
    ) -> Result<(), Self::Error> {
        *target = Some(buffer);
        Ok(())
    }
    async fn finish(
        &self,
        original: &'db [Type<'db>],
        buffer: Option<Self::Buffer>,
    ) -> Result<Cow<'db, [Type<'db>]>, Self::Error> {
        Ok(buffer.map_or(Cow::Borrowed(original), Cow::Owned))
    }
}
impl<'db, E: MappingEffects<'db>> SpecializationMapEffects<'db>
    for MappingSpecializationEffects<'_, E>
{
    type Error = E::Error;
    async fn materialization(
        &self,
        mapping: &TypeMapping<'_, 'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(match mapping {
            TypeMapping::Materialize(kind) => Some(*kind),
            _ => None,
        })
    }
    async fn materialize(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.0
            .legacy(MappingOperation::MaterializationOrPolarity, || {
                specialization.materialize_impl(db, kind, visitor)
            })
    }
    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(db))
    }
    async fn map_arguments(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: &TypeMapping<'_, 'db>,
        contexts: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        kind: &mut Option<MaterializationKind>,
    ) -> Result<Cow<'db, [Type<'db>]>, Self::Error> {
        let mut mapper = MappingArguments {
            db,
            effects: self,
            mapping,
            contexts,
            visitor,
            kind,
        };
        specialization
            .map_types_with(
                db,
                async |index, variable, ty| {
                    SpecializationArgumentMapper::map(&mut mapper, index, variable, ty).await
                },
                self.0,
            )
            .await
    }
    async fn tuple_inner(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<TupleType<'db>>, Self::Error> {
        Ok(specialization.tuple_inner(db))
    }
    async fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TupleType<'db>, Self::Error> {
        self.0.legacy(MappingOperation::Tuple, || {
            tuple.apply_type_mapping_impl(db, mapping, TypeContext::default(), visitor)
        })
    }
    async fn arguments_borrowed(&self, types: &Cow<'db, [Type<'db>]>) -> Result<bool, Self::Error> {
        Ok(matches!(types, Cow::Borrowed(_)))
    }
    async fn same_tuple(
        &self,
        left: Option<TupleType<'db>>,
        right: Option<TupleType<'db>>,
    ) -> Result<bool, Self::Error> {
        Ok(left == right)
    }
    async fn same_kind(
        &self,
        left: Option<MaterializationKind>,
        right: Option<MaterializationKind>,
    ) -> Result<bool, Self::Error> {
        Ok(left == right)
    }
    async fn payload(&self, types: &Cow<'db, [Type<'db>]>) -> Result<(), Self::Error> {
        self.0
            .checkpoint(MappingWork::SpecializationPayload { width: types.len() })
            .await
    }
    async fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(specialization.generic_context(db))
    }
    async fn intern(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'db, [Type<'db>]>,
        kind: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(Specialization::new(db, context, types, kind, tuple))
    }
}
impl<'db, E: MappingEffects<'db>> SpecializationArgumentMapEffects<'db>
    for MappingSpecializationEffects<'_, E>
{
    type Error = E::Error;
    async fn argument_context(
        &self,
        contexts: &[Type<'db>],
        index: usize,
    ) -> Result<TypeContext<'db>, Self::Error> {
        Ok(TypeContext::new(contexts.get(index).copied()))
    }
    async fn mode(
        &self,
        mapping: &TypeMapping<'_, 'db>,
    ) -> Result<ArgumentMappingMode, Self::Error> {
        Ok(ArgumentMappingMode::classify(mapping))
    }
    async fn covariant(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        self.0.checkpoint(MappingWork::VarianceLookup).await?;
        Ok(self.0.variance(db, variable)?.is_covariant())
    }
    async fn copy_mapping<'a>(
        &self,
        mapping: &TypeMapping<'a, 'db>,
    ) -> Result<TypeMapping<'a, 'db>, Self::Error> {
        Ok(mapping.clone())
    }
    async fn flip_mapping<'a>(
        &self,
        mapping: &TypeMapping<'a, 'db>,
    ) -> Result<TypeMapping<'a, 'db>, Self::Error> {
        Ok(mapping.flip())
    }
    async fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.0.checkpoint(MappingWork::ChildRequest).await?;
        self.0.map_type(db, ty, mapping, context, visitor).await
    }
    async fn polarity_argument(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        kind: &mut Option<MaterializationKind>,
    ) -> Result<Type<'db>, Self::Error> {
        self.0
            .legacy(MappingOperation::MaterializationOrPolarity, || {
                polarity_argument(db, variable, ty, mapping, context, visitor, kind)
            })
    }
}

impl<'db, E: SynchronousMappingEffects<'db>> SynchronousCompositionStartEffects<'db>
    for MappingSpecializationEffects<'_, E>
{
    type Error = E::Error;
    type Environment = ProgramEnvironment<'db>;
    fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(specialization.generic_context(db))
    }
    fn program(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<Program<'db>, Self::Error> {
        Ok(context.program(db))
    }
    fn environment(&self, program: Program<'db>) -> Result<Self::Environment, Self::Error> {
        Ok(ProgramEnvironment::from_program(program))
    }
    fn compose_fresh(
        &self,
        db: &'db dyn Db,
        base: Specialization<'db>,
        additional: Specialization<'db>,
        env: &Self::Environment,
    ) -> Result<Specialization<'db>, Self::Error> {
        compose_specializations_sync(
            db,
            base,
            additional,
            &ApplyTypeMappingVisitor::new(env),
            self,
        )
    }
}
impl<'db, E: SynchronousMappingEffects<'db>> SynchronousCompositionEffects<'db>
    for MappingSpecializationEffects<'_, E>
{
    type Error = E::Error;
    fn specialization_mapping(
        &self,
        additional: Specialization<'db>,
    ) -> Result<OwnedTypeMapping<'db, 'db>, Self::Error> {
        Ok(OwnedTypeMapping::Specialization {
            specialization: additional,
            specialize_self_domain: false,
            materialization_kind: None,
        })
    }
    fn materialization_mapping(
        &self,
        kind: MaterializationKind,
    ) -> Result<OwnedTypeMapping<'db, 'db>, Self::Error> {
        Ok(OwnedTypeMapping::Materialize(kind))
    }
    fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(db))
    }
    fn map_pass(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: OwnedTypeMapping<'db, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        map_specialization_sync(
            db,
            specialization,
            &mapping.into_mapping(),
            &[],
            visitor,
            self,
        )
    }
}
impl<'db, E: SynchronousMappingEffects<'db>> SynchronousSpecializationArgumentEffects<'db>
    for MappingSpecializationEffects<'_, E>
{
    type Error = E::Error;
    type Cursor = ArgumentCursor<'db>;
    type Buffer = Vec<Type<'db>>;
    fn types(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(specialization.types(db))
    }
    fn entries(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        types: &'db [Type<'db>],
    ) -> Result<Self::Cursor, Self::Error> {
        Ok(argument_cursor(
            specialization.generic_context(db).variables_inner(db),
            types,
        ))
    }
    fn next(
        &self,
        cursor: &mut Self::Cursor,
    ) -> Result<Option<(usize, (BoundTypeVarInstance<'db>, Type<'db>))>, Self::Error> {
        self.0.checkpoint(MappingWork::ArgumentAdvance)?;
        Ok(cursor.next())
    }
    fn different(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error> {
        Ok(left != right)
    }
    fn new_buffer(&self, original: &'db [Type<'db>]) -> Result<Self::Buffer, Self::Error> {
        self.0.checkpoint(MappingWork::ArgumentCapacity {
            width: original.len(),
        })?;
        Ok(Vec::with_capacity(original.len()))
    }
    fn copy_prefix(
        &self,
        buffer: &mut Self::Buffer,
        original: &'db [Type<'db>],
        index: usize,
    ) -> Result<(), Self::Error> {
        self.0
            .checkpoint(MappingWork::ArgumentPrefixCopy { len: index })?;
        buffer.extend_from_slice(&original[..index]);
        Ok(())
    }
    fn append(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Self::Error> {
        self.0.checkpoint(MappingWork::ArgumentAppend)?;
        buffer.push(ty);
        Ok(())
    }
    fn retain_buffer(
        &self,
        target: &mut Option<Self::Buffer>,
        buffer: Self::Buffer,
    ) -> Result<(), Self::Error> {
        *target = Some(buffer);
        Ok(())
    }
    fn finish(
        &self,
        original: &'db [Type<'db>],
        buffer: Option<Self::Buffer>,
    ) -> Result<Cow<'db, [Type<'db>]>, Self::Error> {
        Ok(buffer.map_or(Cow::Borrowed(original), Cow::Owned))
    }
}
impl<'db, E: SynchronousMappingEffects<'db>> SynchronousSpecializationMapEffects<'db>
    for MappingSpecializationEffects<'_, E>
{
    type Error = E::Error;
    fn materialization(
        &self,
        mapping: &TypeMapping<'_, 'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(match mapping {
            TypeMapping::Materialize(kind) => Some(*kind),
            _ => None,
        })
    }
    fn materialize(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.0
            .legacy(MappingOperation::MaterializationOrPolarity, || {
                specialization.materialize_impl(db, kind, visitor)
            })
    }
    fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(db))
    }
    fn map_arguments(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: &TypeMapping<'_, 'db>,
        contexts: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        kind: &mut Option<MaterializationKind>,
    ) -> Result<Cow<'db, [Type<'db>]>, Self::Error> {
        let mut mapper = MappingArguments {
            db,
            effects: self,
            mapping,
            contexts,
            visitor,
            kind,
        };
        specialization.map_types_sync(
            db,
            |index, variable, ty| {
                SynchronousSpecializationArgumentMapper::map(&mut mapper, index, variable, ty)
            },
            self.0,
        )
    }
    fn tuple_inner(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<TupleType<'db>>, Self::Error> {
        Ok(specialization.tuple_inner(db))
    }
    fn map_tuple(
        &self,
        db: &'db dyn Db,
        tuple: TupleType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<TupleType<'db>, Self::Error> {
        self.0.legacy(MappingOperation::Tuple, || {
            tuple.apply_type_mapping_impl(db, mapping, TypeContext::default(), visitor)
        })
    }
    fn arguments_borrowed(&self, types: &Cow<'db, [Type<'db>]>) -> Result<bool, Self::Error> {
        Ok(matches!(types, Cow::Borrowed(_)))
    }
    fn same_tuple(
        &self,
        left: Option<TupleType<'db>>,
        right: Option<TupleType<'db>>,
    ) -> Result<bool, Self::Error> {
        Ok(left == right)
    }
    fn same_kind(
        &self,
        left: Option<MaterializationKind>,
        right: Option<MaterializationKind>,
    ) -> Result<bool, Self::Error> {
        Ok(left == right)
    }
    fn payload(&self, types: &Cow<'db, [Type<'db>]>) -> Result<(), Self::Error> {
        self.0
            .checkpoint(MappingWork::SpecializationPayload { width: types.len() })
    }
    fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(specialization.generic_context(db))
    }
    fn intern(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'db, [Type<'db>]>,
        kind: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(Specialization::new(db, context, types, kind, tuple))
    }
}
impl<'db, E: SynchronousMappingEffects<'db>> SynchronousSpecializationArgumentMapEffects<'db>
    for MappingSpecializationEffects<'_, E>
{
    type Error = E::Error;
    fn argument_context(
        &self,
        contexts: &[Type<'db>],
        index: usize,
    ) -> Result<TypeContext<'db>, Self::Error> {
        Ok(TypeContext::new(contexts.get(index).copied()))
    }
    fn mode(&self, mapping: &TypeMapping<'_, 'db>) -> Result<ArgumentMappingMode, Self::Error> {
        Ok(ArgumentMappingMode::classify(mapping))
    }
    fn covariant(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<bool, Self::Error> {
        self.0.checkpoint(MappingWork::VarianceLookup)?;
        Ok(self.0.variance(db, variable)?.is_covariant())
    }
    fn copy_mapping<'a>(
        &self,
        mapping: &TypeMapping<'a, 'db>,
    ) -> Result<TypeMapping<'a, 'db>, Self::Error> {
        Ok(mapping.clone())
    }
    fn flip_mapping<'a>(
        &self,
        mapping: &TypeMapping<'a, 'db>,
    ) -> Result<TypeMapping<'a, 'db>, Self::Error> {
        Ok(mapping.flip())
    }
    fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.0.checkpoint(MappingWork::ChildRequest)?;
        self.0.map_type(db, ty, mapping, context, visitor)
    }
    fn polarity_argument(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        kind: &mut Option<MaterializationKind>,
    ) -> Result<Type<'db>, Self::Error> {
        self.0
            .legacy(MappingOperation::MaterializationOrPolarity, || {
                polarity_argument(db, variable, ty, mapping, context, visitor, kind)
            })
    }
}
