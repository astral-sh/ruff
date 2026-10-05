//! Prepared inputs for the shared class-member implementation.

use super::mapping::MappingFailure;
pub(super) use super::mro::PreparedMroCursor;
use super::mro::{PreparedMroRootEffects, PreparedMroWork};
use super::source::declarations::{DeclarationKey, MissingDeclaration, PreparedDeclarations};
use super::{Boundary, Router};
use crate::place::source_effects::{PublicLookupEffects, sealed as lookup_sealed};
use crate::place::{LookupError, LookupResult, Place, PlaceAndQualifiers, Provenance};
use crate::types::class::implicit_attributes::AugmentedBindings;
use crate::types::class::member_lookup::{
    MemberFinalizationEffects, MemberFinalizationWork, MroClassMemberRequest, MroImplicitAttribute,
    MroMemberEffects, MroMemberWork, MroPendingBindings, finalize_class_member_with,
    into_function_like_callable, mro_class_member_with, sealed as mro_member_sealed,
};
use crate::types::class::own_member::{
    ClassTypeOwnMemberEffects, ClassTypeOwnMemberRequest, ClassTypeOwnMemberWork, OwnMemberEffects,
    OwnMemberLookupRequest, class_type_own_member_with, into_dunder_paramspec_callable,
    own_class_member_with, sealed,
};
use crate::types::class::synthesized_member::{
    SynthesizedMemberEffects, SynthesizedMemberWork, own_synthesized_member_with,
    sealed as synthesized_member_sealed,
};
use crate::types::class::{
    ClassMemberResult, CodeGeneratorKind, DynamicClassLiteral, DynamicEnumLiteral,
    DynamicNamedTupleLiteral, DynamicTypedDictLiteral, FrozenDataclassMethod,
};
use crate::types::class_base::ClassBase;
use crate::types::enums::try_unwrap_nonmember_value;
use crate::types::generics::Specialization;
use crate::types::member::Member;
use crate::types::promotion::{
    PublicPromotionEffects, PublicPromotionFacts, PublicPromotionWork, sealed as promotion_sealed,
};
use crate::types::storage_quote::{buffer_push_quote, buffer_retirement};
use crate::types::tuple::TupleSpec;
use crate::types::{
    ClassLiteral, ClassType, FunctionType, GenericAlias, GenericContext, KnownClass,
    MemberLookupPolicy, NominalInstanceType, StaticClassLiteral, Type,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LookupOperation {
    DataclassFields,
    NamedTupleField,
    NamedTupleProperty,
    DunderParamSpec,
    ConstructorContext,
    SlotDescriptor,
    TotalOrderingMember,
    FrozenDataclassSubclassMember,
    GeneratedMember,
    NonmemberValue,
    UnionNormalization,
    DefaultSpecialization,
    TupleRuntimeSpecialization,
    MroCycleRecovery,
    StaticMroErrorConstruction,
    StaticMroErrorDetails,
    DynamicProperMro,
    DynamicNamedTupleProperMro,
    DynamicTypedDictProperMro,
    DynamicEnumProperMro,
    DynamicOwnMember,
    NamedTupleOwnMember,
    TypedDictOwnMember,
    EnumOwnMember,
    TupleLenMember,
    TupleGetitemMember,
    TupleNewMember,
    AugmentedInference,
    UnionAugmented,
    DynamicIntersection,
    TypedDictClassMember,
    DunderFunctionLike,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum LookupFailure<'db> {
    Boundary(Boundary),
    Missing(MissingDeclaration<'db>),
    Unsupported(LookupOperation),
}

impl From<Boundary> for LookupFailure<'_> {
    fn from(error: Boundary) -> Self {
        Self::Boundary(error)
    }
}

impl<'db> From<MissingDeclaration<'db>> for LookupFailure<'db> {
    fn from(error: MissingDeclaration<'db>) -> Self {
        Self::Missing(error)
    }
}

impl<'db> From<MappingFailure<'db>> for LookupFailure<'db> {
    fn from(error: MappingFailure<'db>) -> Self {
        match error {
            MappingFailure::Boundary(error) => Self::Boundary(error),
            MappingFailure::MissingPromotionFact(key) => {
                Self::Missing(MissingDeclaration(DeclarationKey::Promotion(key)))
            }
        }
    }
}

/// Looks up a static root with exactly the specialization supplied to its ordinary MRO iterator.
pub(super) async fn class_member_from_mro<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    router: &Router<'db, '_>,
    root: StaticClassLiteral<'db>,
    iterator_specialization: Option<Specialization<'db>>,
    name: &str,
    policy: MemberLookupPolicy,
) -> Result<PlaceAndQualifiers<'db>, LookupFailure<'db>> {
    let effects = PreparedMroMemberEffects::new(db, env, router, name)?;
    router.consumer_checkpoint(effects.name_units).await?;
    router.validate_declarations(db, env)?;
    if root.program_file(db).program(db) != env.program(db)
        || iterator_specialization.is_some_and(|specialization| {
            specialization.generic_context(db).program(db) != env.program(db)
        })
    {
        return Err(Boundary::ProgramDomain.into());
    }

    let inherited_generic_context = effects.prepared()?.inherited_generic_context(root)?;
    let result = mro_class_member_with(
        MroClassMemberRequest {
            name,
            policy,
            inherited_generic_context,
            is_self_object: root.known(db) == Some(KnownClass::Object),
        },
        PreparedMroCursor::new(root.into(), iterator_specialization),
        &effects,
    )
    .await?;
    let mut member = match result {
        ClassMemberResult::Done(result) => finalize_class_member_with(result, &effects).await?,
        ClassMemberResult::TypedDict(_) => {
            return Err(LookupFailure::Unsupported(
                LookupOperation::TypedDictClassMember,
            ));
        }
    };

    router.consumer_checkpoint(8).await?;
    if name.starts_with("__")
        && name.ends_with("__")
        && let Place::Defined(defined) = &mut member.place
    {
        if matches!(
            defined.ty,
            Type::Callable(_) | Type::Union(_) | Type::Intersection(_)
        ) {
            return Err(LookupFailure::Unsupported(
                LookupOperation::DunderFunctionLike,
            ));
        }
        defined.ty = into_function_like_callable(db, env, defined.ty);
    }
    Ok(member)
}

pub(super) struct PreparedMroMemberEffects<'eval, 'db, 'c> {
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    work: PreparedMroWork<'eval, 'db, 'c>,
    name_units: usize,
}

impl<'eval, 'db, 'c> PreparedMroMemberEffects<'eval, 'db, 'c> {
    pub(super) fn new(
        db: &'db dyn Db,
        env: &'eval ProgramEnvironment<'db>,
        router: &'eval Router<'db, 'c>,
        name: &str,
    ) -> Result<Self, LookupFailure<'db>> {
        let name_units = name
            .len()
            .checked_mul(4)
            .and_then(|units| units.checked_add(48))
            .ok_or(Boundary::CostOverflow)?;
        Ok(Self {
            db,
            env,
            router,
            work: PreparedMroWork::consumer(router),
            name_units,
        })
    }

    fn prepared(&self) -> Result<&PreparedDeclarations<'db>, LookupFailure<'db>> {
        self.router
            .declarations
            .as_deref()
            .ok_or_else(|| Boundary::SourcePreparation.into())
    }

    fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, LookupFailure<'db>> {
        PreparedMroRootEffects::new(self.db, &self.work)
            .tuple_runtime_specialization(specialization)
    }
}

impl mro_member_sealed::Sealed for PreparedMroMemberEffects<'_, '_, '_> {}
impl sealed::Sealed for PreparedMroMemberEffects<'_, '_, '_> {}

impl<'db> MroMemberEffects<'db, PreparedMroCursor<'db>> for PreparedMroMemberEffects<'_, 'db, '_> {
    type Error = LookupFailure<'db>;

    async fn known_class(&self, class: ClassType<'db>) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }

    async fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Option<MroImplicitAttribute<'db>>, Self::Error> {
        class
            .static_class_literal(self.db)
            .map(|(class, _)| {
                self.prepared()?
                    .mro_implicit(class, name)
                    .map_err(Into::into)
            })
            .transpose()
    }

    async fn checkpoint(&self, work: MroMemberWork) -> Result<(), Self::Error> {
        let units = match work {
            MroMemberWork::Advance
            | MroMemberWork::KnownClass
            | MroMemberWork::OwnMember
            | MroMemberWork::UnionAugmented
            | MroMemberWork::Fallback
            | MroMemberWork::Publish => 8,
            MroMemberWork::ImplicitAttribute => self.name_units,
            MroMemberWork::PushAugmented { prefix_len } => prefix_len
                .checked_add(1)
                .and_then(|count| count.checked_mul(8))
                .ok_or(Boundary::CostOverflow)?,
            MroMemberWork::InferAugmented { pending_len } => pending_len
                .checked_mul(8)
                .and_then(|units| units.checked_add(8))
                .ok_or(Boundary::CostOverflow)?,
            MroMemberWork::ClearAugmented { pending_len } => pending_len
                .checked_mul(4)
                .and_then(|units| units.checked_add(4))
                .ok_or(Boundary::CostOverflow)?,
        };
        self.router
            .consumer_checkpoint(units)
            .await
            .map_err(Into::into)
    }

    async fn advance(
        &self,
        cursor: &mut PreparedMroCursor<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        cursor.advance(self.db, &self.work).await
    }

    async fn own_member(
        &self,
        class: ClassType<'db>,
        name: &str,
        context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        class_type_own_member_with(
            ClassTypeOwnMemberRequest {
                class,
                name,
                inherited_generic_context: context,
            },
            self,
        )
        .await
    }

    async fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Self::Error> {
        let quote = buffer_push_quote::<(ClassType<'db>, AugmentedBindings<'db>)>((
            pending.len(),
            pending.capacity(),
            pending.capacity() != 0,
        ))
        .ok_or(Boundary::CostOverflow)?;
        self.router.consumer_checkpoint(quote.work).await?;
        pending.push((class, bindings));
        Ok(())
    }

    async fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error> {
        let work = buffer_retirement::<(ClassType<'db>, AugmentedBindings<'db>)>((
            pending.len(),
            0,
            false,
        ))
        .ok_or(Boundary::CostOverflow)?;
        self.router.consumer_checkpoint(work).await?;
        pending.clear();
        Ok(())
    }

    async fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error> {
        let work = buffer_retirement::<(ClassType<'db>, AugmentedBindings<'db>)>((
            pending.len(),
            pending.capacity(),
            pending.capacity() != 0,
        ))
        .ok_or(Boundary::CostOverflow)?;
        self.router.consumer_checkpoint(work).await?;
        drop(pending);
        Ok(())
    }

    async fn infer_augmented(
        &self,
        _bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::AugmentedInference,
        ))
    }

    async fn union_augmented(
        &self,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(LookupOperation::UnionAugmented))
    }

    async fn fall_back_to(
        &self,
        prior: LookupError<'db>,
        member: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Self::Error> {
        prior
            .or_fall_back_to_with(
                self.db,
                self.env,
                &PreparedPublicLookupEffects {
                    router: self.router,
                },
                member,
            )
            .await
    }
}

impl<'db> MemberFinalizationEffects<'db> for PreparedMroMemberEffects<'_, 'db, '_> {
    type Error = LookupFailure<'db>;

    async fn checkpoint(&self, work: MemberFinalizationWork) -> Result<(), Self::Error> {
        let units = match work {
            MemberFinalizationWork::Begin
            | MemberFinalizationWork::Intersect
            | MemberFinalizationWork::Publish => 8,
        };
        self.router
            .consumer_checkpoint(units)
            .await
            .map_err(Into::into)
    }

    async fn intersect_dynamic(
        &self,
        _ty: Type<'db>,
        _dynamic: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::DynamicIntersection,
        ))
    }
}

impl<'db> ClassTypeOwnMemberEffects<'db> for PreparedMroMemberEffects<'_, 'db, '_> {
    async fn alias_origin(
        &self,
        value: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        self.router.consumer_checkpoint(1).await?;
        Ok(value.origin(self.db))
    }
    async fn alias_specialization(
        &self,
        value: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.router.consumer_checkpoint(1).await?;
        Ok(value.specialization(self.db))
    }
    async fn is_tuple(&self, value: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.router.consumer_checkpoint(1).await?;
        Ok(value.is_tuple(self.db))
    }
    async fn specialization_tuple(
        &self,
        value: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error> {
        self.router.consumer_checkpoint(1).await?;
        Ok(value.tuple(self.db))
    }
    type Error = LookupFailure<'db>;

    async fn checkpoint(&self, work: ClassTypeOwnMemberWork) -> Result<(), Self::Error> {
        let units = match work {
            ClassTypeOwnMemberWork::Admission { name_bytes } => name_bytes
                .checked_mul(4)
                .and_then(|units| units.checked_add(16))
                .ok_or(Boundary::CostOverflow)?,
            ClassTypeOwnMemberWork::Dispatch
            | ClassTypeOwnMemberWork::TupleClass
            | ClassTypeOwnMemberWork::TuplePayload
            | ClassTypeOwnMemberWork::Dependency
            | ClassTypeOwnMemberWork::Publish => 8,
        };
        self.router
            .consumer_checkpoint(units)
            .await
            .map_err(Into::into)
    }

    async fn dynamic_member(
        &self,
        _class: DynamicClassLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::DynamicOwnMember,
        ))
    }

    async fn named_tuple_member(
        &self,
        _class: DynamicNamedTupleLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::NamedTupleOwnMember,
        ))
    }

    async fn typed_dict_member(
        &self,
        _class: DynamicTypedDictLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::TypedDictOwnMember,
        ))
    }

    async fn enum_member(
        &self,
        _class: DynamicEnumLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(LookupOperation::EnumOwnMember))
    }

    async fn tuple_len(
        &self,
        _class: ClassType<'db>,
        _specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(LookupOperation::TupleLenMember))
    }

    async fn tuple_getitem(&self, _tuple: &'db TupleSpec<'db>) -> Result<Member<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::TupleGetitemMember,
        ))
    }

    async fn tuple_new(
        &self,
        _class: ClassType<'db>,
        _specialization: Option<Specialization<'db>>,
        _context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(LookupOperation::TupleNewMember))
    }

    async fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        PreparedMroMemberEffects::tuple_runtime_specialization(self, specialization)
    }

    async fn static_own_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        static_own_member(self.db, self.env, self.router, request).await
    }

    async fn owner_specialize(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        let root = self.router.mapping_root(ty, specialization, true)?;
        self.router
            .consumer_mapping_demand(root)
            .await
            .map_err(Into::into)
    }
}

/// Public conversion within the active member consumer. Its work tickets cannot be shared with
/// independently scheduled source or MRO tasks.
pub(super) struct PreparedPublicLookupEffects<'eval, 'db, 'c> {
    pub(super) router: &'eval Router<'db, 'c>,
}

impl promotion_sealed::Sealed for PreparedPublicLookupEffects<'_, '_, '_> {}
impl lookup_sealed::Sealed for PreparedPublicLookupEffects<'_, '_, '_> {}

impl<'db> PublicPromotionFacts<'db> for PreparedPublicLookupEffects<'_, 'db, '_> {
    type Error = LookupFailure<'db>;

    fn enum_singleton(
        &self,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.router
            .declarations
            .as_deref()
            .ok_or(Boundary::SourcePreparation)?
            .promotion_enum_singleton(class)
            .map_err(MappingFailure::MissingPromotionFact)
            .map_err(Into::into)
    }
}

impl<'db> PublicPromotionEffects<'db> for PreparedPublicLookupEffects<'_, 'db, '_> {
    type Error = LookupFailure<'db>;

    async fn checkpoint(&self, work: PublicPromotionWork) -> Result<(), Self::Error> {
        let units = match work {
            PublicPromotionWork::Admission | PublicPromotionWork::UnionRequest => 4,
            PublicPromotionWork::SingletonDispatch => 3,
            PublicPromotionWork::SingletonClassification => 8,
        };
        self.router
            .consumer_checkpoint(units)
            .await
            .map_err(Into::into)
    }

    async fn regular(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        let root = self.router.promotion_root(ty)?;
        self.router
            .consumer_mapping_demand(root)
            .await
            .map_err(Into::into)
    }

    async fn is_singleton(
        &self,
        db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        instance.is_singleton_with(db, self)
    }

    async fn union_two(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.checkpoint(PublicPromotionWork::UnionRequest).await?;
        Err(LookupFailure::Unsupported(
            LookupOperation::UnionNormalization,
        ))
    }
}

impl<'db> PublicLookupEffects<'db> for PreparedPublicLookupEffects<'_, 'db, '_> {
    type Error = LookupFailure<'db>;

    async fn promote_public_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        ty.promote_public_with(db, env, self).await
    }

    async fn union_two(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        PublicPromotionEffects::union_two(self, db, env, first, second).await
    }
}

pub(super) async fn static_own_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    router: &Router<'db, '_>,
    request: OwnMemberLookupRequest<'_, 'db>,
) -> Result<Member<'db>, LookupFailure<'db>> {
    own_class_member_with(
        request,
        &PreparedOwnMemberEffects::new(db, env, router, request.name)?,
    )
    .await
}

pub(super) async fn static_synthesized_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    router: &Router<'db, '_>,
    request: OwnMemberLookupRequest<'_, 'db>,
) -> Result<Option<Type<'db>>, LookupFailure<'db>> {
    own_synthesized_member_with(
        request,
        &PreparedOwnMemberEffects::new(db, env, router, request.name)?,
    )
    .await
}

struct PreparedOwnMemberEffects<'eval, 'db, 'c> {
    db: &'db dyn Db,
    env: &'eval ProgramEnvironment<'db>,
    router: &'eval Router<'db, 'c>,
    units: usize,
}

impl<'eval, 'db, 'c> PreparedOwnMemberEffects<'eval, 'db, 'c> {
    fn new(
        db: &'db dyn Db,
        env: &'eval ProgramEnvironment<'db>,
        router: &'eval Router<'db, 'c>,
        name: &str,
    ) -> Result<Self, LookupFailure<'db>> {
        let units = name
            .len()
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(32))
            .ok_or(Boundary::CostOverflow)?;
        Ok(Self {
            db,
            env,
            router,
            units,
        })
    }

    fn prepared(&self) -> Result<&PreparedDeclarations<'db>, LookupFailure<'db>> {
        self.router
            .declarations
            .as_deref()
            .ok_or_else(|| Boundary::SourcePreparation.into())
    }
}

impl sealed::Sealed for PreparedOwnMemberEffects<'_, '_, '_> {}

impl synthesized_member_sealed::Sealed for PreparedOwnMemberEffects<'_, '_, '_> {}

impl<'db> SynthesizedMemberEffects<'db> for PreparedOwnMemberEffects<'_, 'db, '_> {
    async fn total_ordering(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.router.consumer_checkpoint(1).await?;
        Ok(class.total_ordering(self.db))
    }
    type Error = LookupFailure<'db>;

    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        self.prepared()?.code_generator(class).map_err(Into::into)
    }
    async fn checkpoint(&self, work: SynthesizedMemberWork) -> Result<(), Self::Error> {
        let units = match work {
            SynthesizedMemberWork::Admission { name_bytes } => name_bytes
                .checked_mul(4)
                .and_then(|bytes| bytes.checked_add(32))
                .ok_or(Boundary::CostOverflow)?,
            SynthesizedMemberWork::OrderingRequest
            | SynthesizedMemberWork::FrozenRequest
            | SynthesizedMemberWork::CodeGenerator
            | SynthesizedMemberWork::GeneratedRequest
            | SynthesizedMemberWork::Publish => 8,
        };
        self.router
            .consumer_checkpoint(units)
            .await
            .map_err(Into::into)
    }

    async fn total_ordering_member(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::TotalOrderingMember,
        ))
    }

    async fn frozen_subclass_member(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
        _method: FrozenDataclassMethod,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::FrozenDataclassSubclassMember,
        ))
    }

    async fn generated_member(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
        _generator: CodeGeneratorKind<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Err(LookupFailure::Unsupported(LookupOperation::GeneratedMember))
    }
}

impl<'db> OwnMemberEffects<'db> for PreparedOwnMemberEffects<'_, 'db, '_> {
    type Error = LookupFailure<'db>;

    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        self.prepared()?.code_generator(class).map_err(Into::into)
    }

    async fn raw_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        self.prepared()?
            .namespace(request.class, request.name)
            .copied()
            .map_err(Into::into)
    }

    async fn slot_exists(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<bool, Self::Error> {
        self.prepared()?
            .own_slot(request.class, request.name)
            .map_err(Into::into)
    }

    async fn generated_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.prepared()?.generated_slots(class).map_err(Into::into)
    }

    async fn explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.prepared()?.explicit_slots(class).map_err(Into::into)
    }

    async fn implicit_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        self.prepared()?
            .implicit_class(request.class, request.name)
            .map_err(Into::into)
    }

    async fn is_kw_only(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty.is_instance_of(self.db, KnownClass::KwOnly))
    }

    async fn is_enum_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<bool, Self::Error> {
        self.prepared()?
            .enum_member(request.class, request.name)
            .map_err(Into::into)
    }

    async fn is_enum_class(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.prepared()?.is_enum(class).map_err(Into::into)
    }
    async fn checkpoint(&self) -> Result<(), Self::Error> {
        // Name hashing and a possible missing-fact copy are paid before either operation.
        self.router
            .consumer_checkpoint(self.units)
            .await
            .map_err(Into::into)
    }

    async fn dataclass_fields(&self) -> Result<Type<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(LookupOperation::DataclassFields))
    }

    async fn named_tuple_field(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Err(LookupFailure::Unsupported(LookupOperation::NamedTupleField))
    }

    async fn named_tuple_property(&self, _field_type: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::NamedTupleProperty,
        ))
    }

    async fn dunder_paramspec(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        if matches!(
            ty,
            Type::Callable(_) | Type::Union(_) | Type::Intersection(_)
        ) {
            return Err(LookupFailure::Unsupported(LookupOperation::DunderParamSpec));
        }
        Ok(into_dunder_paramspec_callable(self.db, self.env, ty))
    }

    async fn constructor_context(
        &self,
        _function: FunctionType<'db>,
        _context: GenericContext<'db>,
    ) -> Result<FunctionType<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::ConstructorContext,
        ))
    }

    async fn slot_descriptor(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(LookupOperation::SlotDescriptor))
    }

    async fn synthesized_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        own_synthesized_member_with(request, self).await
    }

    async fn nonmember_value(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        if ty.is_instance_of(self.db, KnownClass::Nonmember) {
            return Err(LookupFailure::Unsupported(LookupOperation::NonmemberValue));
        }
        Ok(try_unwrap_nonmember_value(self.db, self.env, ty))
    }
}
