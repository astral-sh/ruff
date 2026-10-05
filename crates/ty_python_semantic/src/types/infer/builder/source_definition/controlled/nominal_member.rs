use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::analysis::ClassCheckOperation;
use crate::place::PlaceAndQualifiers;
use crate::types::class::member_source::runtime_binding_absent_with;
use crate::types::class::metaclass_selection::static_inferred_metaclass_with;
use crate::types::class::namespace::{
    NamespaceLookupEffects, NamespaceLookupRequest, NamespaceLookupWork, namespace_lookup_with,
    sealed,
};
use crate::types::class::slots::SlotSelectorEffects;
use crate::types::class::{ClassMetaclass, KnownClassInstanceEffects};
use crate::types::class_selection::{
    LiteralFallbackFacts, NominalSelectionEffects, literal_meta_type_with,
};
use crate::types::descriptor::effects::{DescriptorEffects, DescriptorOperation};
use crate::types::descriptor::{DescriptorRequest, DescriptorResult};
use crate::types::instance::{NominalClassFacts, nominal_class_with};
use crate::types::member_lookup::class_dispatch::{
    ClassMemberDispatchEffects, ClassMemberDispatchFacts, ClassMemberMroKind, PropertyClassEffects,
    class_member_dispatch_with, property_class_literal_with,
};
use crate::types::member_lookup::general::{GeneralMemberEffects, GeneralMemberOperation};
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::property_provenance::PropertyProvenanceEffects;
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::subclass_of::{SubclassConstructionFacts, SubclassOfInner, subclass_from_with};
use crate::types::typed_dict::SynthesizedTypedDictType;
use crate::types::{
    ClassBase, ClassLiteral, ClassType, DescriptorOrigin, InstanceFallbackShadowsNonDataDescriptor,
    IntersectionType, KnownClass, LiteralValueType, LookupDescriptorEffects, LookupFacts,
    LookupParts, MemberEntryEffects, MemberLookupKey, MemberLookupPolicy, MemberLookupResult,
    NominalInstanceType, PropertyDeprecations, ProtocolInstanceType, Type, UnionType,
    invoke_lookup_descriptor_with, nominal_class_member_with, property_metadata_with,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn class_member_body(
        &self,
        key: MemberLookupKey<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let (ty, name, policy) = GeneralMemberEffects::key_parts(self, key).await?;
        self.allocate_future(|| {
            class_member_dispatch_with(ty, name, policy, ClassMemberDispatchFacts, self)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassMemberDispatchEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &Name) -> RunResult<()> {
        self.work(Self::checked(name.len().checked_add(1))?).await
    }

    async fn lookup(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.access.class_member_lookup(ty, name, policy).await
    }

    async fn materialized_protocol(
        &self,
        _protocol: ProtocolInstanceType<'db>,
        _name: &Name,
        _policy: MemberLookupPolicy,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::ProtocolMember,
        ))
        .await
    }

    async fn union(
        &self,
        _union: UnionType<'db>,
        _name: &Name,
        _policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(GeneralMemberOperation::Union))
            .await
    }

    async fn intersection(
        &self,
        _intersection: IntersectionType<'db>,
        _name: &Name,
        _policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::Intersection,
        ))
        .await
    }

    async fn synthesized_typed_dict(
        &self,
        _typed_dict: SynthesizedTypedDictType<'db>,
        _name: &Name,
        _policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::ClassCheck(ClassCheckOperation::TypedDict))
            .await
    }

    async fn protocol_has_no_origin(
        &self,
        _protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::ProtocolOrigin,
        ))
        .await
    }

    async fn protocol_instance_member(
        &self,
        _ty: Type<'db>,
        _name: &Name,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::InstanceStorage,
        ))
        .await
    }

    async fn literal_length(&self, _literal: LiteralValueType<'db>) -> RunResult<Option<i64>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::ClassMemberDispatch,
        ))
        .await
    }

    async fn literal_length_member(
        &self,
        _ty: Type<'db>,
        _length: i64,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::ClassMemberDispatch,
        ))
        .await
    }

    async fn known_type(&self) -> RunResult<Type<'db>> {
        KnownClassInstanceEffects::class_literal(self, KnownClass::Type).await
    }

    async fn mro_member(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
        _kind: ClassMemberMroKind,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let member = self.source_find_name_in_mro(ty, name, policy).await?;
        self.initialize_value(|| {
            member.ok_or(RunError::Contract(
                "class-member MRO lookup has no class-like type",
            ))
        })
        .await?
    }

    async fn nominal_member(
        &self,
        ty: Type<'db>,
        instance: NominalInstanceType<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.allocate_future(|| nominal_class_member_with(ty, instance, name, policy, self))
            .await?
            .await
    }

    async fn meta_type(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.work(1).await?;
        if let Type::PropertyInstance(property) = ty {
            let class = PropertyProvenanceEffects::instance_class(self, property).await?;
            self.allocate_future(|| property_class_literal_with(class, self))
                .await?
                .await
        } else {
            MemberEntryEffects::meta_type(self, ty).await
        }
    }

    async fn class_object_member(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.class_object_member_value(ty, name, policy).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> PropertyClassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn known_class(&self, class: KnownClass) -> RunResult<Type<'db>> {
        KnownClassInstanceEffects::class_literal(self, class).await
    }

    async fn subclass(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        self.initialize_value(|| Type::from(class)).await
    }

    async fn known_instance(&self, class: KnownClass) -> RunResult<Type<'db>> {
        self.access.known_class_instance(self.program, class).await
    }

    async fn subclass_instance(&self, class: ClassType<'db>) -> RunResult<Type<'db>> {
        self.allocate_future(|| KnownClassInstanceEffects::instance(self, class))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MemberEntryEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn key_parts(
        &self,
        key: MemberLookupKey<'db>,
    ) -> RunResult<(Type<'db>, &'db Name, MemberLookupPolicy)> {
        GeneralMemberEffects::key_parts(self, key).await
    }

    async fn suppress_typed_dict_classvar(
        &self,
        ty: Type<'db>,
        result: MemberLookupResult<'db>,
    ) -> RunResult<bool> {
        let parts = self.member_lookup_parts(result).await?;
        self.local(2, 0, || parts.member.is_class_var() && ty.is_typed_dict())
            .await
    }

    async fn meta_type(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        let bytes = Self::checked(
            size_of::<Type<'db>>()
                .checked_mul(4)
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Type<'db>>>()))
                .and_then(|bytes| bytes.checked_add(size_of::<&Self>())),
        )?;
        let ty = self.class_object_local(16, bytes, || ty).await?;
        match ty {
            Type::ClassLiteral(ClassLiteral::Static(class)) => {
                return self
                    .class_object_child(|| self.infer_static_metaclass(class))
                    .await;
            }
            Type::GenericAlias(alias) => {
                return self
                    .class_object_child(|| self.generic_alias_metaclass(alias))
                    .await;
            }
            Type::SubclassOf(subclass) => {
                return self
                    .class_object_child(|| self.subclass_metaclass_value(subclass))
                    .await;
            }
            Type::Dynamic(dynamic) => {
                return self
                    .class_object_child(|| {
                        subclass_from_with(
                            SubclassOfInner::Dynamic(dynamic),
                            SubclassConstructionFacts,
                            self,
                        )
                    })
                    .await;
            }
            Type::Divergent(_) | Type::Never => return Ok(ty),
            _ => {}
        }
        if let Type::LiteralValue(literal) = ty {
            return self
                .allocate_future(|| literal_meta_type_with(literal, LiteralFallbackFacts, self))
                .await?
                .await;
        }
        let Type::NominalInstance(instance) = ty else {
            return self
                .unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::MetaType,
                ))
                .await;
        };
        let class = nominal_class_with(instance, NominalClassFacts, self).await?;
        subclass_from_with(
            SubclassOfInner::Class(class),
            SubclassConstructionFacts,
            self,
        )
        .await
    }

    async fn nominal_class(&self, instance: NominalInstanceType<'db>) -> RunResult<ClassType<'db>> {
        nominal_class_with(instance, NominalClassFacts, self).await
    }

    async fn namespace(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.allocate_future(|| {
            namespace_lookup_with(
                ty,
                NamespaceLookupRequest {
                    class,
                    name,
                    policy,
                },
                self,
            )
        })
        .await?
        .await
    }

    async fn enum_member(
        &self,
        ty: Type<'db>,
        _name: &Name,
    ) -> RunResult<Option<MemberLookupResult<'db>>> {
        self.work(1).await?;
        let Type::NominalInstance(instance) = ty else {
            return self
                .unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::EnumLiteral,
                ))
                .await;
        };
        let class = nominal_class_with(instance, NominalClassFacts, self).await?;
        let Some((class, _)) = self.static_class_identity(class).await? else {
            return self
                .unavailable(SourceOperation::ClassCheck(
                    ClassCheckOperation::EnumMetadata,
                ))
                .await;
        };
        if self
            .access
            .enum_class_literal(class.into())
            .await?
            .is_some()
        {
            self.unavailable(SourceOperation::MemberLookup(
                GeneralMemberOperation::EnumLiteral,
            ))
            .await
        } else {
            Ok(None)
        }
    }

    async fn instance_storage(
        &self,
        ty: Type<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.work(1).await?;
        let Type::NominalInstance(instance) = ty else {
            return self
                .unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::InstanceStorage,
                ))
                .await;
        };
        let class = nominal_class_with(instance, NominalClassFacts, self).await?;
        self.class_instance_storage(class, name).await
    }

    async fn invoke_descriptor(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
        fallback: MemberLookupResult<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.allocate_future(|| {
            invoke_lookup_descriptor_with(
                key,
                receiver,
                fallback,
                InstanceFallbackShadowsNonDataDescriptor::No,
                LookupFacts,
                self,
            )
        })
        .await?
        .await
    }

    async fn fallback(
        &self,
        ty: Type<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.allocate_future(|| {
            crate::types::member_lookup::finalization::fallback_to_getattr_with(
                ty,
                name,
                result,
                policy,
                crate::types::member_lookup::finalization::MemberFinalizationFacts,
                self,
            )
        })
        .await?
        .await
    }

    async fn bind_self(
        &self,
        result: MemberLookupResult<'db>,
        receiver: Type<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.allocate_future(|| {
            crate::types::member_lookup::finalization::map_member_type_with(
                result,
                crate::types::member_lookup::finalization::MemberTypeMapping::BindSelf(receiver),
                crate::types::member_lookup::finalization::MemberFinalizationFacts,
                self,
            )
        })
        .await?
        .await
    }

    async fn promote(&self, result: MemberLookupResult<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.allocate_future(|| {
            crate::types::member_lookup::finalization::promote_inferred_member_with(
                result,
                crate::types::member_lookup::finalization::MemberFinalizationFacts,
                self,
            )
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NamespaceLookupEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type DynamicCursor = MroCursor<'db>;

    async fn checkpoint(&self, _work: NamespaceLookupWork) -> RunResult<()> {
        self.work(1).await
    }

    async fn find_in_mro(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.source_find_name_in_mro(ty, name, policy).await
    }

    async fn inferred_metaclass(&self, class: ClassType<'db>) -> RunResult<ClassMetaclass<'db>> {
        self.work(1).await?;
        let Some((class, specialization)) = self.static_class_identity(class).await? else {
            return self
                .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Metaclass))
                .await;
        };
        let metaclass = static_inferred_metaclass_with(class, self).await?;
        match (metaclass, specialization) {
            (ClassMetaclass::Selected(ty), Some(specialization)) => Ok(ClassMetaclass::Selected(
                self.access
                    .apply_specialization(ty, specialization, false)
                    .await?,
            )),
            _ => Ok(metaclass),
        }
    }

    async fn for_inheritance(&self, metaclass: ClassMetaclass<'db>) -> RunResult<Type<'db>> {
        self.work(1).await?;
        match metaclass {
            ClassMetaclass::Selected(ty) => Ok(ty),
            ClassMetaclass::ProtocolFallback => {
                KnownClassInstanceEffects::class_literal(self, KnownClass::Type).await
            }
        }
    }

    async fn instance_approximation(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        self.work(1).await?;
        if !matches!(ty, Type::ClassLiteral(_) | Type::GenericAlias(_)) {
            return self
                .unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::InstanceApproximation,
                ))
                .await;
        }
        let Some(class) = KnownClassInstanceEffects::to_class_type(self, ty).await? else {
            return Err(RunError::Contract(
                "class-object instance approximation is missing",
            ));
        };
        Ok(Some(
            KnownClassInstanceEffects::instance(self, class).await?,
        ))
    }

    async fn nominal_class(&self, ty: Type<'db>) -> RunResult<Option<ClassType<'db>>> {
        NominalSelectionEffects::nominal_class(self, ty).await
    }

    async fn instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.class_instance_storage(class, name).await
    }

    async fn own_member(
        &self,
        request: NamespaceLookupRequest<'_, 'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.source_class_member_from_mro(
            request.class,
            request.name,
            request.policy,
            super::own_member::SourceMemberMroSelection::Own,
        )
        .await
    }

    async fn runtime_binding_absent(&self, class: ClassType<'db>, name: &str) -> RunResult<bool> {
        let Some((class, _)) = self.static_class_identity(class).await? else {
            return Ok(false);
        };
        let scope = SlotSelectorEffects::body_scope(self, class).await?;
        let env = PairUnionEffects::environment(self, self.program).await?;
        self.allocate_future(|| runtime_binding_absent_with(&env, scope, name, self))
            .await?
            .await
    }

    async fn inherited_member(
        &self,
        request: NamespaceLookupRequest<'_, 'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.source_class_member_from_mro(
            request.class,
            request.name,
            request.policy,
            super::own_member::SourceMemberMroSelection::Inherited,
        )
        .await
    }

    async fn fall_back_to(
        &self,
        member: PlaceAndQualifiers<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let env = self
            .local(size_of::<ProgramEnvironment<'db>>() * 2 + 1, 0, || {
                ProgramEnvironment::from_program(self.program)
            })
            .await?;
        self.allocate_future(|| async {
            Ok(match member
                .into_lookup_result_with(self.db(), &env, self)
                .await?
            {
                Ok(member) => Ok(member),
                Err(error) => {
                    error
                        .or_fall_back_to_with(self.db(), &env, self, fallback)
                        .await?
                }
            }
            .into())
        })
        .await?
        .await
    }

    async fn start_dynamic_mro(&self, class: ClassType<'db>) -> RunResult<Self::DynamicCursor> {
        self.work(1).await?;
        let Some((class, specialization)) = self.static_class_identity(class).await? else {
            return self
                .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::MroDynamic))
                .await;
        };
        self.local(size_of::<MroCursor<'db>>() * 2 + 1, 0, || {
            MroCursor::new(class.into(), specialization)
        })
        .await
    }

    async fn next_dynamic_base(
        &self,
        cursor: &mut Self::DynamicCursor,
    ) -> RunResult<Option<ClassBase<'db>>> {
        mro_next_with(
            MroFieldReads::new(self.db()),
            cursor,
            MroDirection::Forward,
            self,
        )
        .await
    }

    async fn may_be_data_descriptor(&self, _ty: Type<'db>) -> RunResult<bool> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::DataDescriptor,
        ))
        .await
    }

    async fn filter_possible_data_descriptors(
        &self,
        _ty: Type<'db>,
    ) -> RunResult<(Type<'db>, bool)> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::DataDescriptor,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LookupDescriptorEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn fallback_parts(&self, result: MemberLookupResult<'db>) -> RunResult<LookupParts<'db>> {
        self.member_lookup_parts(result).await
    }

    async fn class_attribute(
        &self,
        key: MemberLookupKey<'db>,
        _receiver: Type<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let (ty, name, policy) = GeneralMemberEffects::key_parts(self, key).await?;
        let special = self
            .local(Self::checked(name.len().checked_add(1))?, 0, || {
                name == "__dict__" || matches!(ty, Type::TypeVar(_))
            })
            .await?;
        if special {
            return self
                .unavailable(SourceOperation::Descriptor(
                    DescriptorOperation::ClassMember,
                ))
                .await;
        }
        self.access.class_member_lookup(ty, name, policy).await
    }

    async fn owner(&self, receiver: Type<'db>) -> RunResult<Type<'db>> {
        MemberEntryEffects::meta_type(self, receiver).await
    }

    async fn descriptor(
        &self,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        let env = self
            .local(size_of::<ProgramEnvironment<'db>>() * 2 + 1, 0, || {
                ProgramEnvironment::from_program(self.program)
            })
            .await?;
        DescriptorEffects::descriptor(self, self.db(), &env, request).await
    }

    async fn properties(&self, ty: Type<'db>) -> RunResult<Option<PropertyDeprecations<'db>>> {
        property_metadata_with(ty, self).await
    }

    async fn union(&self, first: Type<'db>, second: Type<'db>) -> RunResult<Type<'db>> {
        self.access.union_from_two_elements(first, second).await
    }

    async fn merge_properties(
        &self,
        _first: Option<PropertyDeprecations<'db>>,
        _second: Option<PropertyDeprecations<'db>>,
    ) -> RunResult<Option<PropertyDeprecations<'db>>> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::PropertyMetadata,
        ))
        .await
    }

    async fn merge_origins(
        &self,
        first: DescriptorOrigin<'db>,
        second: DescriptorOrigin<'db>,
    ) -> RunResult<DescriptorOrigin<'db>> {
        DescriptorEffects::merge_origins(self, self.db(), first, second).await
    }

    async fn result(&self, parts: LookupParts<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.access.member_result(parts).await
    }
}
