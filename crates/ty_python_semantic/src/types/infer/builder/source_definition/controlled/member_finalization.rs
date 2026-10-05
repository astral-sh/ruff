use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::PlaceAndQualifiers;
use crate::types::class::instance_flags::{InstanceFlagFacts, queued_instance_flags_with};
use crate::types::class::namespace::NamespaceLookupEffects;
use crate::types::class::{ClassInstanceFlags, KnownClassInstanceEffects};
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::descriptor::effects::DescriptorOperation;
use crate::types::member_lookup::class_object::ClassObjectEffects;
use crate::types::member_lookup::finalization::{
    GetattrCallKind, GetattrCallResult, MemberFinalizationEffects, MemberFinalizationFacts,
    MemberTypeMapping, custom_getattr_with, custom_getattribute_affects_with,
    getattr_fallback_with, member_fallback_with, metaclass_nominal_class_with,
};
use crate::types::member_lookup::general::GeneralMemberOperation;
use crate::types::{
    ClassType, DescriptorOrigin, KnownClass, LookupDescriptorEffects, LookupParts, MemberEntryEffects,
    MemberLookupPolicy, MemberLookupResult, PropertyDeprecations, StaticClassLiteral, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MemberFinalizationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn parts(&self, result: MemberLookupResult<'db>) -> RunResult<LookupParts<'db>> {
        self.member_lookup_parts(result).await
    }

    async fn result(&self, parts: LookupParts<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.access.member_result(parts).await
    }

    async fn map_type(
        &self,
        ty: Type<'db>,
        mapping: MemberTypeMapping<'db>,
    ) -> RunResult<Type<'db>> {
        match mapping {
            MemberTypeMapping::BindSelf(receiver) => self.bind_member_self_type(ty, receiver).await,
            MemberTypeMapping::InferredClassLiterals => {
                self.unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::InferredAttributePromotion,
                ))
                .await
            }
        }
    }

    async fn nominal_class(&self, ty: Type<'db>) -> RunResult<Option<ClassType<'db>>> {
        NominalSelectionEffects::nominal_class(self, ty).await
    }

    async fn meta_type(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.type_parameter_future(|| MemberEntryEffects::meta_type(self, ty)).await?.await
    }

    async fn instance_approximation(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        self.type_parameter_future(|| ClassObjectEffects::instance_approximation(self, ty)).await?.await
    }

    async fn metaclass_nominal_class(&self, ty: Type<'db>) -> RunResult<Option<ClassType<'db>>> {
        // The shared helper adds one Option test, two extractions and five fixed
        // constructions/returns around its three separately admitted semantic children.
        let bytes = const { 2 * size_of::<Type<'db>>()
            + size_of::<Option<Type<'db>>>()
            + size_of::<Option<ClassType<'db>>>() };
        self.local_with_fixed_transfers(8, bytes, || ()).await?;
        self.type_parameter_future(|| metaclass_nominal_class_with(ty, self)).await?.await
    }

    async fn static_class(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let identity = self.static_class_identity(class).await?;
        self.local(1, 0, || identity.map(|(class, _)| class)).await
    }

    async fn instance_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags> {
        queued_instance_flags_with(class, InstanceFlagFacts, self).await
    }

    async fn custom_getattribute_affects(
        &self,
        ty: Type<'db>,
        result: MemberLookupResult<'db>,
    ) -> RunResult<bool> {
        self.allocate_future(|| {
            custom_getattribute_affects_with(ty, result, MemberFinalizationFacts, self)
        })
        .await?
        .await
    }

    async fn getattribute_member(&self, ty: Type<'db>) -> RunResult<PlaceAndQualifiers<'db>> {
        let name = self
            .local(1, 0, || Name::new_static("__getattribute__"))
            .await?;
        self.access
            .class_member_lookup(
                ty,
                &name,
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
            .await
    }

    async fn name_type(&self, name: &Name) -> RunResult<Type<'db>> {
        self.access.string_literal(name.as_str()).await
    }

    async fn type_member(
        &self,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>> {
        let ty = KnownClassInstanceEffects::class_literal(self, KnownClass::Type).await?;
        self.access.member_lookup(ty, name, policy).await
    }

    async fn call_dunder(
        &self,
        ty: Type<'db>,
        name: Type<'db>,
        kind: GetattrCallKind,
    ) -> RunResult<GetattrCallResult<'db>> {
        self.call_member_fallback_dunder(ty, name, kind).await
    }

    async fn custom_getattr(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.allocate_future(|| {
            custom_getattr_with(ty, name, policy, MemberFinalizationFacts, self)
        })
        .await?
        .await
    }

    async fn getattr_fallback(
        &self,
        ty: Type<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.allocate_future(|| {
            getattr_fallback_with(ty, name, result, policy, MemberFinalizationFacts, self)
        })
        .await?
        .await
    }

    async fn merge_place(
        &self,
        first: PlaceAndQualifiers<'db>,
        second: PlaceAndQualifiers<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        NamespaceLookupEffects::fall_back_to(self, first, second).await
    }

    async fn merge_properties(
        &self,
        _first: PropertyDeprecations<'db>,
        _second: PropertyDeprecations<'db>,
    ) -> RunResult<PropertyDeprecations<'db>> {
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
        LookupDescriptorEffects::merge_origins(self, first, second).await
    }

    async fn fall_back(
        &self,
        result: MemberLookupResult<'db>,
        fallback: MemberLookupResult<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.allocate_future(|| {
            member_fallback_with(result, fallback, MemberFinalizationFacts, self)
        })
        .await?
        .await
    }
}
