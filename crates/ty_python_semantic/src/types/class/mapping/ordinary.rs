use super::{GenericAliasMappingEffects, SynchronousGenericAliasMappingEffects};
use crate::Db;
use crate::types::mapping::effects::{
    MappingEffects, MappingOperation, MappingWork, SynchronousMappingEffects,
};
use crate::types::{
    ApplyTypeMappingVisitor, GenericAlias, Specialization, StaticClassLiteral, Type, TypeContext,
    TypeMapping,
};

pub(in crate::types::class) struct MappingGenericAliasEffects<'a, E>(
    pub(in crate::types::class) &'a E,
);

impl<'db, E: MappingEffects<'db>> GenericAliasMappingEffects<'db>
    for MappingGenericAliasEffects<'_, E>
{
    type Error = E::Error;

    async fn structural(&self, mapping: &TypeMapping<'_, 'db>) -> Result<bool, Self::Error> {
        Ok(mapping.is_structural())
    }

    async fn annotation(
        &self,
        context: TypeContext<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(context.annotation)
    }

    async fn annotation_context(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        annotation: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        self.0.legacy(MappingOperation::AnnotationContext, || {
            annotation
                .specialization_of(db, visitor.env, alias.origin(db))
                .map(|specialization| specialization.types(db))
                .unwrap_or(&[])
        })
    }

    async fn specialization(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(alias.specialization(db))
    }

    async fn map_specialization(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: &TypeMapping<'_, 'db>,
        contexts: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        specialization
            .apply_type_mapping_with(db, mapping, contexts, visitor, self.0)
            .await
    }

    async fn same_specialization(
        &self,
        left: Specialization<'db>,
        right: Specialization<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(left == right)
    }

    async fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Error> {
        self.0.checkpoint(work).await
    }

    async fn origin(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(alias.origin(db))
    }

    async fn intern(
        &self,
        db: &'db dyn Db,
        origin: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<GenericAlias<'db>, Self::Error> {
        Ok(GenericAlias::new(db, origin, specialization))
    }
}

impl<'db, E: SynchronousMappingEffects<'db>> SynchronousGenericAliasMappingEffects<'db>
    for MappingGenericAliasEffects<'_, E>
{
    type Error = E::Error;

    fn structural(&self, mapping: &TypeMapping<'_, 'db>) -> Result<bool, Self::Error> {
        Ok(mapping.is_structural())
    }

    fn annotation(&self, context: TypeContext<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(context.annotation)
    }

    fn annotation_context(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        annotation: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        self.0.legacy(MappingOperation::AnnotationContext, || {
            annotation
                .specialization_of(db, visitor.env, alias.origin(db))
                .map(|specialization| specialization.types(db))
                .unwrap_or(&[])
        })
    }

    fn specialization(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(alias.specialization(db))
    }

    fn map_specialization(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        mapping: &TypeMapping<'_, 'db>,
        contexts: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        specialization.apply_type_mapping_sync(db, mapping, contexts, visitor, self.0)
    }

    fn same_specialization(
        &self,
        left: Specialization<'db>,
        right: Specialization<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(left == right)
    }

    fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Error> {
        self.0.checkpoint(work)
    }

    fn origin(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(alias.origin(db))
    }

    fn intern(
        &self,
        db: &'db dyn Db,
        origin: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> Result<GenericAlias<'db>, Self::Error> {
        Ok(GenericAlias::new(db, origin, specialization))
    }
}
