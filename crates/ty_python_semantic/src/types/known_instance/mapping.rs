use std::convert::Infallible;

use super::{
    FunctoolsPartialInstance, InternedType, KnownInstanceType, MethodWrapper, UnionTypeInstance,
};
use crate::Db;
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarInstance, CallableType, PromotionKind,
    PromotionMode, Type, TypeContext, TypeMapping, TypeVarNonce, typevar::TypeVarInstance,
};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousKnownInstanceMappingEffects)]
    pub(in crate::types) trait KnownInstanceMappingEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn dispatch(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn bind_typevar(&self, db: &'db dyn Db, typevar: TypeVarInstance<'db>, binding_context: &BindingContext<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
        #[operation(child)]
        async fn union_type(&self, db: &'db dyn Db, instance: UnionTypeInstance<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<UnionTypeInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn interned_inner(&self, db: &'db dyn Db, ty: InternedType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn map_type(&self, db: &'db dyn Db, ty: Type<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn intern_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Result<InternedType<'db>, Self::Error>;
        #[operation(child)]
        async fn callable(&self, db: &'db dyn Db, callable: CallableType<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<CallableType<'db>, Self::Error>;
        #[operation(child)]
        async fn method_wrapper(&self, db: &'db dyn Db, wrapper: MethodWrapper<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<MethodWrapper<'db>, Self::Error>;
        #[operation(child)]
        async fn functools_partial(&self, db: &'db dyn Db, partial: FunctoolsPartialInstance<'db>, mapping: &TypeMapping<'_, 'db>, tcx: TypeContext<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<FunctoolsPartialInstance<'db>, Self::Error>;
        #[operation(child)]
        async fn promote_range(&self, db: &'db dyn Db, instance: KnownInstanceType<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(map_known_instance_sync)]
    #[capabilities(effects = KnownInstanceMappingEffects)]
    #[passive_values(Type::TypeVar, Type::KnownInstance, KnownInstanceType::UnionType, KnownInstanceType::Annotated, KnownInstanceType::Callable, KnownInstanceType::MethodWrapper, KnownInstanceType::FunctoolsPartial, KnownInstanceType::FunctoolsPartialCall, KnownInstanceType::TypeGenericAlias, KnownInstanceType::LiteralStringAlias)]
    pub(in crate::types) async fn map_known_instance_with<'db, E: KnownInstanceMappingEffects<'db>>(
        db: &'db dyn Db,
        instance: KnownInstanceType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.dispatch().await?;
        Ok(match instance {
            KnownInstanceType::TypeVar(typevar) => match mapping {
                TypeMapping::BindLegacyTypevars(binding_context) => {
                    Type::TypeVar(effects.bind_typevar(db, typevar, binding_context).await?)
                }
                TypeMapping::ApplySpecialization(_)
                | TypeMapping::ApplySpecializationWithMaterialization { .. }
                | TypeMapping::Promote(..)
                | TypeMapping::FreshenBoundTypeVars { .. }
                | TypeMapping::BindSelf(..)
                | TypeMapping::ReplaceSelf { .. }
                | TypeMapping::Materialize(_)
                | TypeMapping::ReplaceParameterDefaults
                | TypeMapping::EagerExpansion
                | TypeMapping::RescopeReturnCallables(_)
                | TypeMapping::ApplyRecursiveSubstitution(_) => Type::KnownInstance(instance),
            },
            KnownInstanceType::UnionType(union) => {
                Type::KnownInstance(KnownInstanceType::UnionType(
                    effects.union_type(db, union, mapping, tcx, visitor).await?,
                ))
            }
            KnownInstanceType::Annotated(ty) => {
                let inner = effects.interned_inner(db, ty).await?;
                let inner = effects.map_type(db, inner, mapping, tcx, visitor).await?;
                Type::KnownInstance(KnownInstanceType::Annotated(
                    effects.intern_type(db, inner).await?,
                ))
            }
            KnownInstanceType::Callable(callable_type) => {
                Type::KnownInstance(KnownInstanceType::Callable(
                    effects.callable(db, callable_type, mapping, tcx, visitor).await?,
                ))
            }
            KnownInstanceType::MethodWrapper(wrapper) => {
                Type::KnownInstance(KnownInstanceType::MethodWrapper(
                    effects.method_wrapper(db, wrapper, mapping, tcx, visitor).await?,
                ))
            }
            KnownInstanceType::FunctoolsPartial(partial) => {
                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(
                    effects.functools_partial(db, partial, mapping, tcx, visitor).await?,
                ))
            }
            KnownInstanceType::Range { .. } => match mapping {
                TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular) => {
                    effects.promote_range(db, instance, visitor).await?
                }
                _ => Type::KnownInstance(instance),
            },
            KnownInstanceType::FunctoolsPartialCall(partial) => {
                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(
                    effects.functools_partial(db, partial, mapping, tcx, visitor).await?,
                ))
            }
            KnownInstanceType::TypeGenericAlias(ty) => {
                let inner = effects.interned_inner(db, ty).await?;
                let inner = effects.map_type(db, inner, mapping, tcx, visitor).await?;
                Type::KnownInstance(KnownInstanceType::TypeGenericAlias(
                    effects.intern_type(db, inner).await?,
                ))
            }
            KnownInstanceType::LiteralStringAlias(ty) => {
                let inner = effects.interned_inner(db, ty).await?;
                let inner = effects.map_type(db, inner, mapping, tcx, visitor).await?;
                Type::KnownInstance(KnownInstanceType::LiteralStringAlias(
                    effects.intern_type(db, inner).await?,
                ))
            }

            KnownInstanceType::SubscriptedProtocol(_)
            | KnownInstanceType::SubscriptedGeneric(_)
            | KnownInstanceType::TypeAliasType(_)
            | KnownInstanceType::Deprecated(_)
            | KnownInstanceType::Field(_)
            | KnownInstanceType::ConstraintSet(_)
            | KnownInstanceType::ConstraintSetSolution(_)
            | KnownInstanceType::GenericContext(_)
            | KnownInstanceType::Specialization(_)
            | KnownInstanceType::Literal(_)
            | KnownInstanceType::NamedTupleSpec(_)
            | KnownInstanceType::NewType(_)
            | KnownInstanceType::Sentinel(_) => {
                // TODO: For some of these, we may need to apply the type mapping to inner types.
                Type::KnownInstance(instance)
            }
        })
    }
}

pub(super) struct OrdinaryKnownInstanceMapping;

impl<'db> SynchronousKnownInstanceMappingEffects<'db> for OrdinaryKnownInstanceMapping {
    type Error = Infallible;

    fn dispatch(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn bind_typevar(
        &self,
        db: &'db dyn Db,
        typevar: TypeVarInstance<'db>,
        binding_context: &BindingContext<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(BoundTypeVarInstance::new(
            db,
            typevar,
            *binding_context,
            None,
            TypeVarNonce::NONE,
        ))
    }

    fn union_type(
        &self,
        db: &'db dyn Db,
        instance: UnionTypeInstance<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<UnionTypeInstance<'db>, Infallible> {
        Ok(instance.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }

    fn interned_inner(
        &self,
        db: &'db dyn Db,
        ty: InternedType<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.inner(db))
    }

    fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(ty.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }

    fn intern_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Result<InternedType<'db>, Infallible> {
        Ok(InternedType::new(db, ty))
    }

    fn callable(
        &self,
        db: &'db dyn Db,
        callable: CallableType<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<CallableType<'db>, Infallible> {
        Ok(callable.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }

    fn method_wrapper(
        &self,
        db: &'db dyn Db,
        wrapper: MethodWrapper<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<MethodWrapper<'db>, Infallible> {
        Ok(wrapper.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }

    fn functools_partial(
        &self,
        db: &'db dyn Db,
        partial: FunctoolsPartialInstance<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<FunctoolsPartialInstance<'db>, Infallible> {
        Ok(partial.apply_type_mapping_impl(db, mapping, tcx, visitor))
    }

    fn promote_range(
        &self,
        db: &'db dyn Db,
        instance: KnownInstanceType<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(instance.instance_fallback(db, visitor.env))
    }
}
