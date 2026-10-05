use std::borrow::Cow;
use std::iter::{Copied, Enumerate, Zip};
use std::slice;

use super::{GenericContext, Specialization};
use crate::types::generics::context_construction::ContextVariables;
use crate::types::mapping::OwnedTypeMapping;
use crate::types::tuple::TupleType;
use crate::types::typevar::BoundTypeVarIdentity;
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, MaterializationKind, PromotionKind, Type,
    TypeContext, TypeMapping,
};
use crate::{Db, Program};

pub(super) mod ordinary;
#[cfg(test)]
mod tests;

pub(in crate::types) type ArgumentCursor<'db> = Enumerate<
    Zip<
        Copied<ordermap::map::Values<'db, BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>>,
        Copied<slice::Iter<'db, Type<'db>>>,
    >,
>;

pub(in crate::types) fn argument_cursor<'db>(
    variables: &'db ContextVariables<'db>,
    types: &'db [Type<'db>],
) -> ArgumentCursor<'db> {
    variables
        .values()
        .copied()
        .zip(types.iter().copied())
        .enumerate()
}

#[derive(Clone, Copy)]
pub(in crate::types) enum ArgumentMappingMode {
    RegularPromotion,
    Plain,
    Polarity,
}

impl ArgumentMappingMode {
    pub(in crate::types) fn classify(mapping: &TypeMapping<'_, '_>) -> Self {
        if matches!(mapping, TypeMapping::Promote(_, PromotionKind::Regular)) {
            Self::RegularPromotion
        } else if !mapping.is_polarity_sensitive() {
            Self::Plain
        } else {
            Self::Polarity
        }
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSpecializationLookupEffects)]
    pub(in crate::types) trait SpecializationLookupEffects<'db> {
        type Error;
        #[operation(source)]
        async fn generic_context(&self, specialization: Specialization<'db>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Self::Error>;
        #[operation(source)]
        async fn identity(&self, variable: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;
        #[operation(local)]
        async fn index(&self, variables: &'db ContextVariables<'db>, identity: BoundTypeVarIdentity<'db>) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn types(&self, specialization: Specialization<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        async fn type_at(&self, types: &'db [Type<'db>], index: usize) -> Result<Option<Type<'db>>, Self::Error>;
    }

    #[synchronous(lookup_specialization_sync)]
    #[capabilities(effects = SpecializationLookupEffects)]
    #[passive_values()]
    pub(in crate::types) async fn lookup_specialization_with<'db, E: SpecializationLookupEffects<'db>>(
        specialization: Specialization<'db>,
        variable: BoundTypeVarInstance<'db>,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let context = effects.generic_context(specialization).await?;
        let variables = effects.variables(context).await?;
        let identity = effects.identity(variable).await?;
        let Some(index) = effects.index(variables, identity).await? else {
            return Ok(None);
        };
        let types = effects.types(specialization).await?;
        effects.type_at(types, index).await
    }

    #[synchronous(SynchronousCompositionStartEffects)]
    pub(in crate::types) trait CompositionStartEffects<'db> {
        type Error;
        type Environment;
        #[operation(source)]
        async fn generic_context(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(source)]
        async fn program(&self, db: &'db dyn Db, context: GenericContext<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(local)]
        async fn environment(&self, program: Program<'db>) -> Result<Self::Environment, Self::Error>;
        #[operation(child)]
        async fn compose_fresh(&self, db: &'db dyn Db, base: Specialization<'db>, additional: Specialization<'db>, env: &Self::Environment) -> Result<Specialization<'db>, Self::Error>;
    }
    #[synchronous(SynchronousCompositionEffects)]
    pub(in crate::types) trait CompositionEffects<'db> {
        type Error;
        #[operation(local)]
        async fn specialization_mapping(&self, additional: Specialization<'db>) -> Result<OwnedTypeMapping<'db, 'db>, Self::Error>;
        #[operation(local)]
        async fn materialization_mapping(&self, kind: MaterializationKind) -> Result<OwnedTypeMapping<'db, 'db>, Self::Error>;
        #[operation(source)]
        async fn materialization_kind(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<Option<MaterializationKind>, Self::Error>;
        #[operation(child)]
        async fn map_pass(&self, db: &'db dyn Db, specialization: Specialization<'db>, mapping: OwnedTypeMapping<'db, 'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Specialization<'db>, Self::Error>;
    }
    #[synchronous(SynchronousSpecializationMapEffects)]
    pub(in crate::types) trait SpecializationMapEffects<'db> {
        type Error;
        #[operation(local)]
        async fn materialization(&self, mapping: &TypeMapping<'_, 'db>) -> Result<Option<MaterializationKind>, Self::Error>;
        #[operation(child)]
        async fn materialize(&self, db: &'db dyn Db, specialization: Specialization<'db>, kind: MaterializationKind, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(source)]
        async fn materialization_kind(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<Option<MaterializationKind>, Self::Error>;
        #[operation(child)]
        async fn map_arguments(&self, db: &'db dyn Db, specialization: Specialization<'db>, mapping: &TypeMapping<'_, 'db>, contexts: &[Type<'db>], visitor: &ApplyTypeMappingVisitor<'_, 'db>, kind: &mut Option<MaterializationKind>) -> Result<Cow<'db, [Type<'db>]>, Self::Error>;
        #[operation(source)]
        async fn tuple_inner(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<Option<TupleType<'db>>, Self::Error>;
        #[operation(child)]
        async fn map_tuple(&self, db: &'db dyn Db, tuple: TupleType<'db>, mapping: &TypeMapping<'_, 'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<TupleType<'db>, Self::Error>;
        #[operation(local)]
        async fn arguments_borrowed(&self, types: &Cow<'db, [Type<'db>]>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn same_tuple(&self, left: Option<TupleType<'db>>, right: Option<TupleType<'db>>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn same_kind(&self, left: Option<MaterializationKind>, right: Option<MaterializationKind>) -> Result<bool, Self::Error>;
        #[operation(checkpoint)]
        async fn payload(&self, types: &Cow<'db, [Type<'db>]>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn generic_context(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(source)]
        async fn intern(&self, db: &'db dyn Db, context: GenericContext<'db>, types: Cow<'db, [Type<'db>]>, kind: Option<MaterializationKind>, tuple: Option<TupleType<'db>>) -> Result<Specialization<'db>, Self::Error>;
    }
    #[synchronous(SynchronousSpecializationArgumentEffects)]
    pub(in crate::types) trait SpecializationArgumentEffects<'db> {
        type Error;
        type Cursor;
        type Buffer;
        #[operation(source)]
        async fn types(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(source)]
        async fn entries(&self, db: &'db dyn Db, specialization: Specialization<'db>, types: &'db [Type<'db>]) -> Result<Self::Cursor, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next(&self, cursor: &mut Self::Cursor) -> Result<Option<(usize, (BoundTypeVarInstance<'db>, Type<'db>))>, Self::Error>;
        #[operation(local)]
        async fn different(&self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn new_buffer(&self, original: &'db [Type<'db>]) -> Result<Self::Buffer, Self::Error>;
        #[operation(local)]
        async fn copy_prefix(&self, buffer: &mut Self::Buffer, original: &'db [Type<'db>], index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn append(&self, buffer: &mut Self::Buffer, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn retain_buffer(&self, target: &mut Option<Self::Buffer>, buffer: Self::Buffer) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish(&self, original: &'db [Type<'db>], buffer: Option<Self::Buffer>) -> Result<Cow<'db, [Type<'db>]>, Self::Error>;
    }
    #[synchronous(SynchronousSpecializationArgumentMapper)]
    pub(in crate::types) trait SpecializationArgumentMapper<'db> {
        type Error;
        #[operation(child)]
        async fn map(&mut self, index: usize, variable: BoundTypeVarInstance<'db>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }
    #[synchronous(SynchronousSpecializationArgumentMapEffects)]
    pub(in crate::types) trait SpecializationArgumentMapEffects<'db> {
        type Error;
        #[operation(local)]
        async fn argument_context(&self, contexts: &[Type<'db>], index: usize) -> Result<TypeContext<'db>, Self::Error>;
        #[operation(local)]
        async fn mode(&self, mapping: &TypeMapping<'_, 'db>) -> Result<ArgumentMappingMode, Self::Error>;
        #[operation(source)]
        async fn covariant(&self, db: &'db dyn Db, variable: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn copy_mapping<'a>(&self, mapping: &TypeMapping<'a, 'db>) -> Result<TypeMapping<'a, 'db>, Self::Error>;
        #[operation(local)]
        async fn flip_mapping<'a>(&self, mapping: &TypeMapping<'a, 'db>) -> Result<TypeMapping<'a, 'db>, Self::Error>;
        #[operation(child)]
        async fn map_type(&self, db: &'db dyn Db, ty: Type<'db>, mapping: &TypeMapping<'_, 'db>, context: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn polarity_argument(&self, db: &'db dyn Db, variable: BoundTypeVarInstance<'db>, ty: Type<'db>, mapping: &TypeMapping<'_, 'db>, context: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>, kind: &mut Option<MaterializationKind>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(compose_specialization_root_sync)]
    #[capabilities(effects = CompositionStartEffects)]
    #[passive_values()]
    pub(in crate::types) async fn compose_specialization_root_with<'db, E: CompositionStartEffects<'db>>(db: &'db dyn Db, base: Specialization<'db>, additional: Specialization<'db>, effects: &E) -> Result<Specialization<'db>, E::Error> {
        let context = effects.generic_context(db, additional).await?;
        let program = effects.program(db, context).await?;
        let env = effects.environment(program).await?;
        effects.compose_fresh(db, base, additional, &env).await
    }

    #[synchronous(compose_specializations_sync)]
    #[capabilities(effects = CompositionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn compose_specializations_with<'db, E: CompositionEffects<'db>>(db: &'db dyn Db, base: Specialization<'db>, additional: Specialization<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>, effects: &E) -> Result<Specialization<'db>, E::Error> {
        let mapping = effects.specialization_mapping(additional).await?;
        let mapped = effects.map_pass(db, base, mapping, visitor).await?;
        match effects.materialization_kind(db, additional).await? {
            None => Ok(mapped),
            Some(kind) => {
                let mapping = effects.materialization_mapping(kind).await?;
                effects.map_pass(db, mapped, mapping, visitor).await
            }
        }
    }

    #[synchronous(map_specialization_sync)]
    #[capabilities(effects = SpecializationMapEffects)]
    #[passive_values()]
    pub(in crate::types) async fn map_specialization_with<'db, E: SpecializationMapEffects<'db>>(db: &'db dyn Db, specialization: Specialization<'db>, mapping: &TypeMapping<'_, 'db>, contexts: &[Type<'db>], visitor: &ApplyTypeMappingVisitor<'_, 'db>, effects: &E) -> Result<Specialization<'db>, E::Error> {
        if let Some(kind) = effects.materialization(mapping).await? {
            return effects.materialize(db, specialization, kind, visitor).await;
        }
        let mut kind = effects.materialization_kind(db, specialization).await?;
        let types = effects.map_arguments(db, specialization, mapping, contexts, visitor, &mut kind).await?;
        let original_tuple_inner = effects.tuple_inner(db, specialization).await?;
        let tuple_inner = match original_tuple_inner {
            Some(tuple) => Some(effects.map_tuple(db, tuple, mapping, visitor).await?),
            None => None,
        };
        // Keep this check in sync with every field that can be transformed above.
        if effects.arguments_borrowed(&types).await?
            && effects.same_tuple(tuple_inner, original_tuple_inner).await?
            && effects.same_kind(kind, effects.materialization_kind(db, specialization).await?).await?
        {
            Ok(specialization)
        } else {
            effects.payload(&types).await?;
            let context = effects.generic_context(db, specialization).await?;
            effects.intern(db, context, types, kind, tuple_inner).await
        }
    }

    #[synchronous(map_specialization_arguments_sync)]
    #[capabilities(effects = SpecializationArgumentEffects, mapper = SpecializationArgumentMapper)]
    #[passive_values()]
    pub(in crate::types) async fn map_specialization_arguments_with<'db, E: SpecializationArgumentEffects<'db>, M: SpecializationArgumentMapper<'db, Error = E::Error>>(db: &'db dyn Db, specialization: Specialization<'db>, mapper: &mut M, effects: &E) -> Result<Cow<'db, [Type<'db>]>, E::Error> {
        let types = effects.types(db, specialization).await?;
        let mut mapped_types = None;
        let mut entries = effects.entries(db, specialization, types).await?;
        #[cursor_loop]
        while let Some(entry) = effects.next(&mut entries).await? {
            let (index, (typevar, ty)) = entry;
            let mapped_ty = mapper.map(index, typevar, ty).await?;
            if let Some(mapped_types) = &mut mapped_types {
                effects.append(mapped_types, mapped_ty).await?;
            } else if effects.different(mapped_ty, ty).await? {
                let mut changed_types = effects.new_buffer(types).await?;
                effects.copy_prefix(&mut changed_types, types, index).await?;
                effects.append(&mut changed_types, mapped_ty).await?;
                effects.retain_buffer(&mut mapped_types, changed_types).await?;
            }
        }
        effects.finish(types, mapped_types).await
    }

    #[synchronous(map_specialization_argument_sync)]
    #[capabilities(effects = SpecializationArgumentMapEffects)]
    #[passive_values()]
    pub(in crate::types) async fn map_specialization_argument_with<'db, E: SpecializationArgumentMapEffects<'db>>(db: &'db dyn Db, index: usize, variable: BoundTypeVarInstance<'db>, ty: Type<'db>, mapping: &TypeMapping<'_, 'db>, contexts: &[Type<'db>], visitor: &ApplyTypeMappingVisitor<'_, 'db>, kind: &mut Option<MaterializationKind>, effects: &E) -> Result<Type<'db>, E::Error> {
        let context = effects.argument_context(contexts, index).await?;
        match effects.mode(mapping).await? {
            ArgumentMappingMode::RegularPromotion => {
                let child_mapping = if effects.covariant(db, variable).await? {
                    effects.copy_mapping(mapping).await?
                } else {
                    effects.flip_mapping(mapping).await?
                };
                effects.map_type(db, ty, &child_mapping, context, visitor).await
            }
            ArgumentMappingMode::Plain => effects.map_type(db, ty, mapping, context, visitor).await,
            ArgumentMappingMode::Polarity => effects.polarity_argument(db, variable, ty, mapping, context, visitor, kind).await,
        }
    }
}

pub(in crate::types) struct MappingArguments<'a, 'm, 'env, 'db, E> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) effects: &'a E,
    pub(in crate::types) mapping: &'a TypeMapping<'m, 'db>,
    pub(in crate::types) contexts: &'a [Type<'db>],
    pub(in crate::types) visitor: &'a ApplyTypeMappingVisitor<'env, 'db>,
    pub(in crate::types) kind: &'a mut Option<MaterializationKind>,
}

impl<'db, E: SpecializationArgumentMapEffects<'db>> SpecializationArgumentMapper<'db>
    for MappingArguments<'_, '_, '_, 'db, E>
{
    type Error = E::Error;
    async fn map(
        &mut self,
        index: usize,
        variable: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        map_specialization_argument_with(
            self.db,
            index,
            variable,
            ty,
            self.mapping,
            self.contexts,
            self.visitor,
            self.kind,
            self.effects,
        )
        .await
    }
}

impl<'db, E: SynchronousSpecializationArgumentMapEffects<'db>>
    SynchronousSpecializationArgumentMapper<'db> for MappingArguments<'_, '_, '_, 'db, E>
{
    type Error = E::Error;
    fn map(
        &mut self,
        index: usize,
        variable: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        map_specialization_argument_sync(
            self.db,
            index,
            variable,
            ty,
            self.mapping,
            self.contexts,
            self.visitor,
            self.kind,
            self.effects,
        )
    }
}
