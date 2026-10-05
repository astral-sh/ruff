use crate::Db;
use crate::types::mapping::effects::MappingWork;
use crate::types::{
    ApplyTypeMappingVisitor, GenericAlias, Specialization, StaticClassLiteral, Type, TypeContext,
    TypeMapping,
};

pub(super) mod ordinary;
#[cfg(test)]
mod tests;

const EMPTY_CONTEXTS: &[Type<'_>] = &[];

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousGenericAliasMappingEffects)]
    pub(in crate::types) trait GenericAliasMappingEffects<'db> {
        type Error;
        #[operation(local)]
        async fn structural(&self, mapping: &TypeMapping<'_, 'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn annotation(&self, context: TypeContext<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn annotation_context(&self, db: &'db dyn Db, alias: GenericAlias<'db>, annotation: Type<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(source)]
        async fn specialization(&self, db: &'db dyn Db, alias: GenericAlias<'db>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(child)]
        async fn map_specialization(&self, db: &'db dyn Db, specialization: Specialization<'db>, mapping: &TypeMapping<'_, 'db>, contexts: &[Type<'db>], visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Specialization<'db>, Self::Error>;
        #[operation(local)]
        async fn same_specialization(&self, left: Specialization<'db>, right: Specialization<'db>) -> Result<bool, Self::Error>;
        #[operation(checkpoint)]
        async fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn origin(&self, db: &'db dyn Db, alias: GenericAlias<'db>) -> Result<StaticClassLiteral<'db>, Self::Error>;
        #[operation(source)]
        async fn intern(&self, db: &'db dyn Db, origin: StaticClassLiteral<'db>, specialization: Specialization<'db>) -> Result<GenericAlias<'db>, Self::Error>;
    }

    #[synchronous(map_generic_alias_sync)]
    #[capabilities(effects = GenericAliasMappingEffects)]
    #[passive_values(MappingWork::GenericAliasIntern, EMPTY_CONTEXTS)]
    pub(in crate::types) async fn map_generic_alias_with<'db, E: GenericAliasMappingEffects<'db>>(
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<GenericAlias<'db>, E::Error> {
        let contexts = if effects.structural(mapping).await? {
            EMPTY_CONTEXTS
        } else if let Some(annotation) = effects.annotation(context).await? {
            effects.annotation_context(db, alias, annotation, visitor).await?
        } else {
            EMPTY_CONTEXTS
        };
        let original = effects.specialization(db, alias).await?;
        let mapped = effects.map_specialization(db, original, mapping, contexts, visitor).await?;
        if effects.same_specialization(mapped, original).await? {
            Ok(alias)
        } else {
            effects.checkpoint(MappingWork::GenericAliasIntern).await?;
            let origin = effects.origin(db, alias).await?;
            effects.intern(db, origin, mapped).await
        }
    }
}
