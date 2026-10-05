//! Ordered class-member lookup with direct and queued dependency providers.

use std::convert::Infallible;

use super::dunder_callable::{
    DunderCallableFacts, DunderCallableTransform, OrdinaryDunderCallableEffects,
    dunder_callable_sync,
};
use super::implicit_attributes::{AugmentedBindings, ImplicitAttribute};
use super::{
    ClassMemberResult, ClassType, CompletedMemberLookup, InstanceMemberResult, KnownClass,
    MethodDecorator, MroLookup, StaticClassLiteral,
};
use crate::place::{
    DefinedPlace, Definedness, LookupError, LookupResult, Place, PlaceAndQualifiers, Provenance,
    PublicTypePolicy, TypeOrigin,
};
use crate::types::class_base::ClassBase;
use crate::types::generics::GenericContext;
use crate::types::member::Member;
use crate::types::{
    IntersectionType, MemberLookupPolicy, Type, TypeQualifiers, UnionBuilder, UnionType,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) fn into_function_like_callable<'d>(
    db: &'d dyn Db,
    env: &ProgramEnvironment<'d>,
    ty: Type<'d>,
) -> Type<'d> {
    match dunder_callable_sync(
        ty,
        DunderCallableTransform::FunctionLike,
        DunderCallableFacts,
        &OrdinaryDunderCallableEffects { db, env },
    ) {
        Ok(ty) => ty,
        Err(never) => match never {},
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct MroClassMemberRequest<'a, 'db> {
    pub(in crate::types) name: &'a str,
    pub(in crate::types) policy: MemberLookupPolicy,
    pub(in crate::types) inherited_generic_context: Option<GenericContext<'db>>,
    pub(in crate::types) is_self_object: bool,
}

/// Declaration-derived assignments from classmethods. The final own-member result determines
/// whether these assignments remain applicable after generated-member suppression.
#[derive(Clone, Copy)]
pub(in crate::types) struct MroImplicitAttribute<'db>(ImplicitAttribute<'db>);

impl<'db> MroImplicitAttribute<'db> {
    pub(in crate::types) fn from_attribute(attribute: ImplicitAttribute<'db>) -> Self {
        Self(attribute)
    }

    pub(in crate::types) fn read(
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Self {
        Self::from_attribute(class.implicit_attribute_bindings(
            db,
            name,
            MethodDecorator::ClassMethod,
        ))
    }
}

/// The lookup owns these bindings until inference and combination have completed.
#[derive(Clone, Copy)]
pub(in crate::types) struct MroPendingBindings<'a, 'db>(
    &'a [(ClassType<'db>, AugmentedBindings<'db>)],
);

impl MroPendingBindings<'_, '_> {
    #[inline]
    pub(in crate::types) fn pending_count(self) -> usize {
        self.0.len()
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) enum MroMemberWork {
    Advance,
    KnownClass,
    OwnMember,
    ImplicitAttribute,
    PushAugmented { prefix_len: usize },
    InferAugmented { pending_len: usize },
    UnionAugmented,
    ClearAugmented { pending_len: usize },
    Fallback,
    Publish,
}

#[derive(Clone, Copy)]
pub(in crate::types) enum MemberFinalizationWork {
    Begin,
    Intersect,
    Publish,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

/// Advancing a prepared cursor can require specialization of the next base.
pub(in crate::types) trait MroMemberEffects<'db, C>: sealed::Sealed {
    type Error;

    /// Prepared providers obtain these declaration inputs after the preceding work checkpoint.
    async fn known_class(&self, class: ClassType<'db>) -> Result<Option<KnownClass>, Self::Error>;

    async fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Option<MroImplicitAttribute<'db>>, Self::Error>;

    async fn checkpoint(&self, work: MroMemberWork) -> Result<(), Self::Error>;
    async fn advance(&self, cursor: &mut C) -> Result<Option<ClassBase<'db>>, Self::Error>;
    async fn own_member(
        &self,
        class: ClassType<'db>,
        name: &str,
        context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error>;
    async fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Self::Error>;
    async fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error>;
    async fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error>;
    async fn infer_augmented(
        &self,
        bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Self::Error>;
    async fn union_augmented(
        &self,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn fall_back_to(
        &self,
        prior: LookupError<'db>,
        member: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Self::Error>;
}

pub(in crate::types) trait SynchronousMroMemberEffects<'db, C>:
    sealed::Sealed
{
    type Error;

    fn known_class(&self, class: ClassType<'db>) -> Result<Option<KnownClass>, Self::Error>;

    fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Option<MroImplicitAttribute<'db>>, Self::Error>;

    fn checkpoint(&self, work: MroMemberWork) -> Result<(), Self::Error>;
    fn advance(&self, cursor: &mut C) -> Result<Option<ClassBase<'db>>, Self::Error>;
    fn own_member(
        &self,
        class: ClassType<'db>,
        name: &str,
        context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error>;
    fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Self::Error>;
    fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error>;
    fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error>;
    fn infer_augmented(
        &self,
        bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Self::Error>;
    fn union_augmented(
        &self,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    fn fall_back_to(
        &self,
        prior: LookupError<'db>,
        member: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Self::Error>;
}

pub(in crate::types) trait MemberFinalizationEffects<'db>: sealed::Sealed {
    type Error;

    async fn checkpoint(&self, work: MemberFinalizationWork) -> Result<(), Self::Error>;
    async fn intersect_dynamic(
        &self,
        ty: Type<'db>,
        dynamic: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}

pub(in crate::types) trait SynchronousMemberFinalizationEffects<'db>:
    sealed::Sealed
{
    type Error;

    fn checkpoint(&self, work: MemberFinalizationWork) -> Result<(), Self::Error>;
    fn intersect_dynamic(
        &self,
        ty: Type<'db>,
        dynamic: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_mro_member]
#[inline]
pub(in crate::types) async fn mro_class_member_with<'a, 'db, C, E: MroMemberEffects<'db, C>>(
    request: MroClassMemberRequest<'a, 'db>,
    mut cursor: C,
    effects: &E,
) -> Result<ClassMemberResult<'db>, E::Error> {
    let MroClassMemberRequest {
        name,
        policy,
        inherited_generic_context,
        is_self_object,
    } = request;
    let mut dynamic_type = None;
    let mut lookup_result = Err(LookupError::Undefined(TypeQualifiers::empty()));
    let mut pending_augmented_bindings = Vec::new();

    loop {
        effects.checkpoint(MroMemberWork::Advance).await?;
        let Some(superclass) = effects.advance(&mut cursor).await? else {
            break;
        };
        match superclass {
            ClassBase::Generic | ClassBase::Protocol => {
                // Skip over these very special class bases that aren't really classes.
            }
            ClassBase::Any | ClassBase::Dynamic(_) if policy.require_concrete() => {}
            ClassBase::Any | ClassBase::Dynamic(_) => {
                // Note: calling `Type::from(superclass).member()` would be incorrect here.
                // What we'd really want is a `Type::Any.own_class_member()` method,
                // but adding such a method wouldn't make much sense -- it would always return `Any`!
                dynamic_type.get_or_insert(Type::from(superclass));
            }
            ClassBase::Divergent(_) => {
                dynamic_type.get_or_insert(Type::from(superclass));
            }
            ClassBase::Class(class) => {
                effects.checkpoint(MroMemberWork::KnownClass).await?;
                let known = effects.known_class(class).await?;

                // Only exclude `object` members if this is not an `object` class itself.
                if known == Some(KnownClass::Object)
                    && policy.mro_no_object_fallback()
                    && !is_self_object
                {
                    continue;
                }
                if known == Some(KnownClass::Type) && policy.meta_class_no_type_fallback() {
                    continue;
                }
                if matches!(known, Some(KnownClass::Int | KnownClass::Str))
                    && policy.mro_no_int_or_str_fallback()
                {
                    continue;
                }

                effects.checkpoint(MroMemberWork::OwnMember).await?;
                let member = effects
                    .own_member(class, name, inherited_generic_context)
                    .await?;
                effects.checkpoint(MroMemberWork::ImplicitAttribute).await?;
                if let Some(MroImplicitAttribute(implicit)) =
                    effects.implicit_attribute(class, name).await?
                    && member.is_undefined() == implicit.member.is_undefined()
                    && let Some(bindings) = implicit.augmented_bindings
                {
                    effects
                        .checkpoint(MroMemberWork::PushAugmented {
                            prefix_len: pending_augmented_bindings.len(),
                        })
                        .await?;
                    effects
                        .push_pending(&mut pending_augmented_bindings, class, bindings)
                        .await?;
                }

                let mut member = member.inner;
                if let Place::Defined(defined) = &mut member.place
                    && !pending_augmented_bindings.is_empty()
                {
                    if !defined.origin.is_declared() {
                        effects
                            .checkpoint(MroMemberWork::InferAugmented {
                                pending_len: pending_augmented_bindings.len(),
                            })
                            .await?;
                        let (inferred_ty, inferred_provenance) = effects
                            .infer_augmented(MroPendingBindings(&pending_augmented_bindings))
                            .await?;
                        effects.checkpoint(MroMemberWork::UnionAugmented).await?;
                        defined.ty = effects.union_augmented(defined.ty, inferred_ty).await?;
                        defined.provenance = defined.provenance.or(inferred_provenance);
                    }
                    effects
                        .checkpoint(MroMemberWork::ClearAugmented {
                            pending_len: pending_augmented_bindings.len(),
                        })
                        .await?;
                    effects.clear_pending(&mut pending_augmented_bindings).await?;
                }

                if let Err(error) = lookup_result {
                    effects.checkpoint(MroMemberWork::Fallback).await?;
                    lookup_result = effects.fall_back_to(error, member).await?;
                }
            }
            ClassBase::TypedDict(module) => {
                effects.finish_pending(pending_augmented_bindings).await?;
                effects.checkpoint(MroMemberWork::Publish).await?;
                return Ok(ClassMemberResult::TypedDict(module));
            }
        }
        if lookup_result.is_ok() {
            break;
        }
    }

    effects.finish_pending(pending_augmented_bindings).await?;
    effects.checkpoint(MroMemberWork::Publish).await?;
    Ok(ClassMemberResult::Done(CompletedMemberLookup {
        lookup_result,
        dynamic_type,
    }))
}

#[ty_mapping_probe_macros::dual_mro_member]
#[inline]
pub(in crate::types) async fn finalize_class_member_with<'db, E: MemberFinalizationEffects<'db>>(
    result: CompletedMemberLookup<'db>,
    effects: &E,
) -> Result<PlaceAndQualifiers<'db>, E::Error> {
    effects.checkpoint(MemberFinalizationWork::Begin).await?;
    let member = match (
        PlaceAndQualifiers::from(result.lookup_result),
        result.dynamic_type,
    ) {
        (member, None) => member,
        (
            PlaceAndQualifiers {
                place: Place::Defined(DefinedPlace { ty, provenance, .. }),
                qualifiers,
            },
            Some(dynamic),
        ) => {
            effects
                .checkpoint(MemberFinalizationWork::Intersect)
                .await?;
            let ty = effects.intersect_dynamic(ty, dynamic).await?;
            Place::bound(ty)
                .with_provenance(provenance)
                .with_qualifiers(qualifiers)
        }
        (
            PlaceAndQualifiers {
                place: Place::Undefined,
                qualifiers,
            },
            Some(dynamic),
        ) => Place::bound(dynamic).with_qualifiers(qualifiers),
    };
    effects.checkpoint(MemberFinalizationWork::Publish).await?;
    Ok(member)
}

pub(in crate::types) struct InlineMemberLookupEffects<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'env, 'db> InlineMemberLookupEffects<'env, 'db> {
    #[inline]
    pub(in crate::types) fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>) -> Self {
        Self { db, env }
    }
}

impl sealed::Sealed for InlineMemberLookupEffects<'_, '_> {}

impl<'db, I: Iterator<Item = ClassBase<'db>>> SynchronousMroMemberEffects<'db, I>
    for InlineMemberLookupEffects<'_, 'db>
{
    type Error = Infallible;

    #[inline]
    fn known_class(&self, class: ClassType<'db>) -> Result<Option<KnownClass>, Infallible> {
        Ok(class.known(self.db))
    }

    #[inline]
    fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Option<MroImplicitAttribute<'db>>, Infallible> {
        Ok(class
            .static_class_literal(self.db)
            .map(|(class, _)| MroImplicitAttribute::read(self.db, class, name)))
    }

    #[inline]
    fn checkpoint(&self, _work: MroMemberWork) -> Result<(), Infallible> {
        Ok(())
    }

    #[inline]
    fn advance(&self, cursor: &mut I) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(cursor.next())
    }

    #[inline]
    fn own_member(
        &self,
        class: ClassType<'db>,
        name: &str,
        context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Infallible> {
        Ok(class.own_class_member(self.db, self.env, context, name))
    }

    #[inline]
    fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Infallible> {
        pending.push((class, bindings));
        Ok(())
    }

    #[inline]
    fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Infallible> {
        pending.clear();
        Ok(())
    }

    #[inline]
    fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Infallible> {
        drop(pending);
        Ok(())
    }

    #[inline]
    fn infer_augmented(
        &self,
        bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Infallible> {
        Ok(MroLookup::<I>::infer_augmented_bindings(
            self.db, self.env, bindings.0,
        ))
    }

    #[inline]
    fn union_augmented(
        &self,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(UnionType::from_two_elements(
            self.db, self.env, first, second,
        ))
    }

    #[inline]
    fn fall_back_to(
        &self,
        prior: LookupError<'db>,
        member: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Infallible> {
        Ok(prior.or_fall_back_to(self.db, self.env, member))
    }
}

impl<'db> SynchronousMemberFinalizationEffects<'db> for InlineMemberLookupEffects<'_, 'db> {
    type Error = Infallible;

    #[inline]
    fn checkpoint(&self, _work: MemberFinalizationWork) -> Result<(), Infallible> {
        Ok(())
    }

    #[inline]
    fn intersect_dynamic(
        &self,
        ty: Type<'db>,
        dynamic: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(IntersectionType::from_two_elements(
            self.db, self.env, ty, dynamic,
        ))
    }
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum InstanceMroWork {
    Begin,
    Advance,
    Classify,
    OwnInstance,
    ImplicitAttribute,
    PushAugmented { prefix_len: usize },
    InferAugmented { pending_len: usize },
    UnionAdd,
    OwnClass,
    DescriptorCheck,
    ClearAugmented { pending_len: usize },
    UnionBuild,
    Publish,
}

pub(in crate::types) trait InstanceMroEffects<'db, C>: sealed::Sealed {
    type Error;

    async fn checkpoint(&self, work: InstanceMroWork) -> Result<(), Self::Error>;
    async fn new_union(&self) -> Result<UnionBuilder<'db>, Self::Error>;
    async fn advance(&self, cursor: &mut C) -> Result<Option<ClassBase<'db>>, Self::Error>;
    async fn own_instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    async fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Option<ImplicitAttribute<'db>>, Self::Error>;
    async fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Self::Error>;
    async fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error>;
    async fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error>;
    async fn infer_augmented(
        &self,
        bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Self::Error>;
    async fn union_add(
        &self,
        union: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error>;
    async fn own_class_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    async fn is_definitely_non_data_descriptor(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
    async fn union_build(&self, union: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
}

pub(in crate::types) trait SynchronousInstanceMroEffects<'db, C>:
    sealed::Sealed
{
    type Error;

    fn checkpoint(&self, work: InstanceMroWork) -> Result<(), Self::Error>;
    fn new_union(&self) -> Result<UnionBuilder<'db>, Self::Error>;
    fn advance(&self, cursor: &mut C) -> Result<Option<ClassBase<'db>>, Self::Error>;
    fn own_instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Option<ImplicitAttribute<'db>>, Self::Error>;
    fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Self::Error>;
    fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error>;
    fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error>;
    fn infer_augmented(
        &self,
        bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Self::Error>;
    fn union_add(
        &self,
        union: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error>;
    fn own_class_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    fn is_definitely_non_data_descriptor(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
    fn union_build(&self, union: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_instance_mro]
pub(in crate::types) async fn mro_instance_member_with<
    'a,
    'db,
    C,
    E: InstanceMroEffects<'db, C>,
>(
    name: &'a str,
    mut cursor: C,
    effects: &E,
) -> Result<InstanceMemberResult<'db>, E::Error> {
    effects.checkpoint(InstanceMroWork::Begin).await?;
    let mut union = effects.new_union().await?;
    let mut union_qualifiers = TypeQualifiers::empty();
    let mut definitely_bound_member: Option<PlaceAndQualifiers<'db>> = None;
    let mut provenance = Provenance::Unknown;
    let mut pending_augmented_bindings = Vec::new();

    loop {
        effects.checkpoint(InstanceMroWork::Advance).await?;
        let Some(superclass) = effects.advance(&mut cursor).await? else {
            break;
        };
        effects.checkpoint(InstanceMroWork::Classify).await?;
        match superclass {
            ClassBase::Generic | ClassBase::Protocol => {
                // Skip over these very special class bases that aren't really classes.
            }
            ClassBase::Any | ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {
                // We already return the dynamic type for class member lookup, so we can
                // just return unbound here (to avoid having to build a union of the
                // dynamic type with itself).
                effects.finish_pending(pending_augmented_bindings).await?;
                effects.checkpoint(InstanceMroWork::Publish).await?;
                return Ok(InstanceMemberResult::Done(PlaceAndQualifiers::unbound()));
            }
            ClassBase::Class(class) => {
                effects.checkpoint(InstanceMroWork::OwnInstance).await?;
                let member = effects.own_instance_member(class, name).await?;
                effects
                    .checkpoint(InstanceMroWork::ImplicitAttribute)
                    .await?;
                let implicit = effects.implicit_attribute(class, name).await?;
                // Pair an ordinary member lookup with augmented assignments that first read their target.
                //
                // ```python
                // class Counter:
                //     value = 0
                //
                //     def increment(self):
                //         self.value += 1
                //
                //     @classmethod
                //     def increment_class(cls):
                //         cls.value += 1
                // ```
                //
                // MRO lookup can infer either assignment only after locating an existing `value`. If ordinary
                // lookup suppressed an implicit attribute, such as a generated `NamedTuple` field, its writes
                // must remain suppressed too.
                let augmented_bindings = match implicit {
                    Some(implicit) if member.is_undefined() == implicit.member.is_undefined() => {
                        implicit.augmented_bindings
                    }
                    _ => None,
                };
                let implicit = ImplicitAttribute {
                    member,
                    augmented_bindings,
                };
                if let Some(bindings) = implicit.augmented_bindings {
                    effects
                        .checkpoint(InstanceMroWork::PushAugmented {
                            prefix_len: pending_augmented_bindings.len(),
                        })
                        .await?;
                    effects
                        .push_pending(&mut pending_augmented_bindings, class, bindings)
                        .await?;
                }

                if let member @ PlaceAndQualifiers {
                    place:
                        Place::Defined(DefinedPlace {
                            ty,
                            origin,
                            definedness: boundness,
                            provenance: member_provenance,
                            ..
                        }),
                    qualifiers,
                } = implicit.member.inner
                {
                    if boundness == Definedness::AlwaysDefined {
                        if origin.is_declared() {
                            if definitely_bound_member.is_some_and(|member| {
                                !member
                                    .qualifiers
                                    .contains(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE)
                            }) && !qualifiers
                                .contains(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE)
                            {
                                // An overriding class default shadows inherited declarations,
                                // but inherited instance assignments must still be collected.
                                continue;
                            }

                            // We found a definitely-declared attribute. Discard possibly collected
                            // inferred types from subclasses and return the declared type.
                            effects.finish_pending(pending_augmented_bindings).await?;
                            effects.checkpoint(InstanceMroWork::Publish).await?;
                            return Ok(InstanceMemberResult::Done(member));
                        }

                        definitely_bound_member = Some(member);
                    }

                    // If the attribute is not definitely declared on this class, keep looking
                    // higher up in the MRO, and build a union of all inferred types (and
                    // possibly-declared types):
                    effects.checkpoint(InstanceMroWork::UnionAdd).await?;
                    union = effects.union_add(union, ty).await?;
                    provenance = provenance.or(member_provenance);

                    // TODO: We could raise a diagnostic here if there are conflicting type
                    // qualifiers
                    union_qualifiers |= qualifiers;

                    if !pending_augmented_bindings.is_empty() {
                        effects
                            .checkpoint(InstanceMroWork::InferAugmented {
                                pending_len: pending_augmented_bindings.len(),
                            })
                            .await?;
                        let (inferred_ty, inferred_provenance) = effects
                            .infer_augmented(MroPendingBindings(&pending_augmented_bindings))
                            .await?;
                        effects.checkpoint(InstanceMroWork::UnionAdd).await?;
                        union = effects.union_add(union, inferred_ty).await?;
                        provenance = provenance.or(inferred_provenance);
                        union_qualifiers |= TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE;
                        effects
                            .checkpoint(InstanceMroWork::ClearAugmented {
                                pending_len: pending_augmented_bindings.len(),
                            })
                            .await?;
                        effects.clear_pending(&mut pending_augmented_bindings).await?;
                    }
                }

                if !pending_augmented_bindings.is_empty()
                    && let class_member @ Member {
                        inner:
                            PlaceAndQualifiers {
                                place:
                                    Place::Defined(DefinedPlace {
                                        ty: class_member_ty,
                                        origin,
                                        definedness: class_member_definedness,
                                        provenance: class_member_provenance,
                                        ..
                                    }),
                                ..
                            },
                    } = {
                        effects.checkpoint(InstanceMroWork::OwnClass).await?;
                        effects.own_class_member(class, name).await?
                    }
                {
                    effects.checkpoint(InstanceMroWork::DescriptorCheck).await?;
                    if !effects
                        .is_definitely_non_data_descriptor(class_member_ty)
                        .await?
                    {
                        effects
                            .checkpoint(InstanceMroWork::ClearAugmented {
                                pending_len: pending_augmented_bindings.len(),
                            })
                            .await?;
                        effects.clear_pending(&mut pending_augmented_bindings).await?;
                        continue;
                    }

                    if origin.is_declared() {
                        if union.is_empty() {
                            effects.finish_pending(pending_augmented_bindings).await?;
                            effects.checkpoint(InstanceMroWork::Publish).await?;
                            return Ok(InstanceMemberResult::Done(class_member.inner));
                        }

                        effects.checkpoint(InstanceMroWork::UnionAdd).await?;
                        union = effects.union_add(union, class_member_ty).await?;
                        provenance = provenance.or(class_member_provenance);
                        union_qualifiers |= class_member.inner.qualifiers;
                    } else {
                        effects
                            .checkpoint(InstanceMroWork::InferAugmented {
                                pending_len: pending_augmented_bindings.len(),
                            })
                            .await?;
                        let (inferred_ty, inferred_provenance) = effects
                            .infer_augmented(MroPendingBindings(&pending_augmented_bindings))
                            .await?;
                        effects.checkpoint(InstanceMroWork::UnionAdd).await?;
                        union = effects.union_add(union, inferred_ty).await?;
                        provenance = provenance.or(inferred_provenance);
                        union_qualifiers |= TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE;
                    }

                    effects
                        .checkpoint(InstanceMroWork::ClearAugmented {
                            pending_len: pending_augmented_bindings.len(),
                        })
                        .await?;
                    effects.clear_pending(&mut pending_augmented_bindings).await?;
                    if class_member_definedness == Definedness::AlwaysDefined {
                        definitely_bound_member = Some(class_member.inner);
                    }
                }
            }
            ClassBase::TypedDict(_) => {
                effects.finish_pending(pending_augmented_bindings).await?;
                effects.checkpoint(InstanceMroWork::Publish).await?;
                return Ok(InstanceMemberResult::TypedDict);
            }
        }
    }

    let result = if union.is_empty() {
        Place::Undefined.with_qualifiers(TypeQualifiers::empty())
    } else {
        let boundness = if definitely_bound_member.is_some() {
            Definedness::AlwaysDefined
        } else {
            Definedness::PossiblyUndefined
        };

        Place::Defined(DefinedPlace {
            ty: {
                effects.checkpoint(InstanceMroWork::UnionBuild).await?;
                effects.union_build(union).await?
            },
            origin: TypeOrigin::Inferred,
            definedness: boundness,
            public_type_policy: PublicTypePolicy::Raw,
            provenance,
        })
        .with_qualifiers(union_qualifiers)
    };

    effects.finish_pending(pending_augmented_bindings).await?;
    effects.checkpoint(InstanceMroWork::Publish).await?;
    Ok(InstanceMemberResult::Done(result))
}

impl<'db, C: Iterator<Item = ClassBase<'db>>> SynchronousInstanceMroEffects<'db, C>
    for InlineMemberLookupEffects<'_, 'db>
{
    type Error = Infallible;

    fn checkpoint(&self, _: InstanceMroWork) -> Result<(), Infallible> {
        Ok(())
    }
    fn new_union(&self) -> Result<UnionBuilder<'db>, Infallible> {
        Ok(UnionBuilder::new(self.db, self.env))
    }
    fn advance(&self, cursor: &mut C) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(cursor.next())
    }
    fn own_instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Member<'db>, Infallible> {
        Ok(class.own_instance_member(self.db, self.env, name))
    }
    fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Option<ImplicitAttribute<'db>>, Infallible> {
        Ok(class.static_class_literal(self.db).map(|(class, _)| {
            class.implicit_attribute_bindings(self.db, name, MethodDecorator::None)
        }))
    }
    fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Infallible> {
        pending.push((class, bindings));
        Ok(())
    }
    fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Infallible> {
        pending.clear();
        Ok(())
    }
    fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Infallible> {
        drop(pending);
        Ok(())
    }
    fn infer_augmented(
        &self,
        bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Infallible> {
        Ok(MroLookup::<C>::infer_augmented_bindings(
            self.db, self.env, bindings.0,
        ))
    }
    fn union_add(
        &self,
        union: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Infallible> {
        Ok(union.add(ty))
    }
    fn own_class_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Member<'db>, Infallible> {
        Ok(class.own_class_member(self.db, self.env, None, name))
    }
    fn is_definitely_non_data_descriptor(&self, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(ty.is_definitely_non_data_descriptor(self.db, self.env))
    }
    fn union_build(&self, union: UnionBuilder<'db>) -> Result<Type<'db>, Infallible> {
        Ok(union.build())
    }
}
