use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;

use super::storage::slots;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::analysis::{ClassCheckOperation, OwnMemberOperation};
use crate::place::{
    ConsideredDefinitions, LookupError, LookupResult, PlaceAndQualifiers, Provenance,
    RequiresExplicitReExport,
};
use crate::types::class::dunder_callable::{
    DunderCallableFacts, DunderCallableTransform, dunder_callable_with,
};
use crate::types::class::implicit_attributes::{
    AugmentedBindings, implicit_attribute_bindings_with,
};
use crate::types::class::member_lookup::{
    MroClassMemberRequest, MroImplicitAttribute, MroMemberEffects, MroMemberWork,
    MroPendingBindings, finalize_class_member_with, mro_class_member_with,
};
use crate::types::class::member_source::{
    RawClassMemberEffects, RawClassMemberFacts, raw_class_member_with,
};
use crate::types::class::own_member::{
    self, ClassTypeOwnMemberEffects, ClassTypeOwnMemberRequest, ClassTypeOwnMemberWork,
    OwnMemberEffects, OwnMemberLookupRequest, class_type_own_member_with, own_class_member_with,
};
use crate::types::class::slots::{
    SlotSelectorEffects, generated_slots_with, own_class_binding_with, own_slot_descriptor_with,
};
use crate::types::class::synthesized_member::{
    self, SynthesizedMemberEffects, SynthesizedMemberWork, own_synthesized_member_with,
};
use crate::types::class::{
    ClassMemberResult, CodeGeneratorKind, DynamicClassLiteral, DynamicEnumLiteral,
    DynamicNamedTupleLiteral, DynamicTypedDictLiteral, FrozenDataclassMethod, MethodDecorator,
    static_code_generator_with,
};
use crate::types::enums::{NonmemberValueEffects, nonmember_value_with};
use crate::types::generics::tuple_runtime::{TupleRuntimeFacts, tuple_runtime_specialization_with};
use crate::types::instance::{NominalClassFacts, nominal_known_class_with};
use crate::types::member::Member;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::tuple::TupleSpec;
use crate::types::{
    ClassBase, ClassType, FunctionType, GenericAlias, GenericContext, KnownClass,
    MemberLookupPolicy, Specialization, StaticClassLiteral, Type,
};

#[derive(Clone, Copy)]
pub(super) enum SourceMemberMroSelection {
    All,
    Own,
    Inherited,
}

pub(super) struct SourceMemberMroCursor<'db> {
    inner: MroCursor<'db>,
    selection: SourceMemberMroSelection,
    first: bool,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn source_own_class_member(
        &self,
        class: ClassType<'db>,
        name: &str,
        inherited_generic_context: Option<GenericContext<'db>>,
    ) -> RunResult<Member<'db>> {
        self.allocate_future(|| {
            class_type_own_member_with(
                ClassTypeOwnMemberRequest {
                    class,
                    name,
                    inherited_generic_context,
                },
                self,
            )
        })
        .await?
        .await
    }

    pub(super) async fn source_class_mro(
        &self,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let Some((literal, specialization)) = self.static_class_identity(class).await? else {
            return self
                .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::MroDynamic))
                .await;
        };
        self.check_file_program(self.static_class_file(literal).await?)
            .await?;
        if specialization.is_none() && self.access.class_generic_context(literal).await?.is_some() {
            return self
                .unavailable(SourceOperation::OwnMember(
                    OwnMemberOperation::GenericClassMro,
                ))
                .await;
        }
        self.source_class_member_from_mro(class, name, policy, SourceMemberMroSelection::All)
            .await
    }

    pub(super) async fn source_class_member_from_mro(
        &self,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        selection: SourceMemberMroSelection,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let Some((literal, specialization)) = self.static_class_identity(class).await? else {
            return self
                .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::MroDynamic))
                .await;
        };
        self.check_file_program(self.static_class_file(literal).await?)
            .await?;
        let context = self.access.class_generic_context(literal).await?;
        let known = self
            .field(
                literal
                    .field_requests(self.access.endpoint().field_request_context())
                    .known(),
            )
            .await?;
        let cursor = self
            .local(size_of::<SourceMemberMroCursor<'db>>() * 2 + 1, 0, || {
                SourceMemberMroCursor {
                    inner: MroCursor::new(literal.into(), specialization),
                    selection,
                    first: true,
                }
            })
            .await?;
        let result = self
            .allocate_future(|| {
                mro_class_member_with(
                    MroClassMemberRequest {
                        name,
                        policy,
                        inherited_generic_context: context,
                        is_self_object: known == Some(KnownClass::Object),
                    },
                    cursor,
                    self,
                )
            })
            .await?
            .await?;
        let ClassMemberResult::Done(result) = result else {
            return self
                .unavailable(SourceOperation::OwnMember(OwnMemberOperation::TypedDictMro))
                .await;
        };
        let mut member = finalize_class_member_with(result, self).await?;
        let dunder = self
            .local(5, 0, || name.starts_with("__") && name.ends_with("__"))
            .await?;
        if dunder && let Some(ty) = member.place.raw_type() {
            let ty = self
                .dunder_callable_mapping(ty, DunderCallableTransform::FunctionLike)
                .await?;
            member = self
                .local(size_of::<PlaceAndQualifiers<'db>>() * 2 + 1, 0, || {
                    member.map_type(|_| ty)
                })
                .await?;
        }
        Ok(member)
    }

    async fn dunder_callable_mapping(
        &self,
        ty: Type<'db>,
        transform: DunderCallableTransform,
    ) -> RunResult<Type<'db>> {
        self.dunder_callable_future(|| {
            dunder_callable_with(ty, transform, DunderCallableFacts, self)
        })
        .await?
        .await
    }

    async fn own_member_is_instance_of(&self, ty: Type<'db>, known: KnownClass) -> RunResult<bool> {
        let instance = self.local(1, 0, || ty.as_nominal_instance()).await?;
        let Some(instance) = instance else {
            return Ok(false);
        };
        let actual = nominal_known_class_with(instance, NominalClassFacts, self).await?;
        self.local(1, 0, || actual == Some(known)).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> own_member::sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassTypeOwnMemberEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: ClassTypeOwnMemberWork) -> RunResult<()> {
        let work = match work {
            ClassTypeOwnMemberWork::Admission { name_bytes } => {
                Self::checked(name_bytes.checked_add(1))?
            }
            _ => 1,
        };
        self.work(work).await
    }

    async fn alias_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        self.field(
            alias
                .field_requests(self.access.endpoint().field_request_context())
                .origin(),
        )
        .await
    }

    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.field(
            alias
                .field_requests(self.access.endpoint().field_request_context())
                .specialization(),
        )
        .await
    }

    async fn is_tuple(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let known = self
            .field(
                class
                    .field_requests(self.access.endpoint().field_request_context())
                    .known(),
            )
            .await?;
        self.local(1, 0, || known == Some(KnownClass::Tuple)).await
    }

    async fn specialization_tuple(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<&'db TupleSpec<'db>>> {
        let fields = self.access.endpoint().field_request_context();
        let tuple = self.field(specialization.tuple_request(fields)).await?;
        match tuple {
            Some(tuple) => Ok(Some(
                self.field(tuple.field_requests(fields).tuple()).await?,
            )),
            None => Ok(None),
        }
    }

    async fn dynamic_member(
        &self,
        _class: DynamicClassLiteral<'db>,
        _name: &str,
    ) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::OwnMember(OwnMemberOperation::Dynamic))
            .await
    }

    async fn named_tuple_member(
        &self,
        _class: DynamicNamedTupleLiteral<'db>,
        _name: &str,
    ) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::OwnMember(
            OwnMemberOperation::DynamicNamedTuple,
        ))
        .await
    }

    async fn typed_dict_member(
        &self,
        _class: DynamicTypedDictLiteral<'db>,
        _name: &str,
    ) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::OwnMember(
            OwnMemberOperation::DynamicTypedDict,
        ))
        .await
    }

    async fn enum_member(
        &self,
        _class: DynamicEnumLiteral<'db>,
        _name: &str,
    ) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::OwnMember(OwnMemberOperation::DynamicEnum))
            .await
    }

    async fn tuple_len(
        &self,
        _class: ClassType<'db>,
        _specialization: Option<Specialization<'db>>,
    ) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::OwnMember(OwnMemberOperation::TupleLen))
            .await
    }

    async fn tuple_getitem(&self, _tuple: &'db TupleSpec<'db>) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::OwnMember(OwnMemberOperation::TupleGetitem))
            .await
    }

    async fn tuple_new(
        &self,
        _class: ClassType<'db>,
        _specialization: Option<Specialization<'db>>,
        _context: Option<GenericContext<'db>>,
    ) -> RunResult<Member<'db>> {
        self.unavailable(SourceOperation::OwnMember(OwnMemberOperation::TupleNew))
            .await
    }

    async fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<Specialization<'db>> {
        tuple_runtime_specialization_with(specialization, self, TupleRuntimeFacts).await
    }

    async fn static_own_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> RunResult<Member<'db>> {
        self.check_file_program(self.static_class_file(request.class).await?)
            .await?;
        self.allocate_future(|| own_class_member_with(request, self))
            .await?
            .await
    }

    async fn owner_specialize(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<Type<'db>> {
        self.access
            .apply_specialization(ty, specialization, true)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RawClassMemberEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn public_class_place(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.access
            .place_by_id(
                scope,
                symbol.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            )
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> OwnMemberEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        static_code_generator_with(class, self).await
    }

    async fn raw_member(&self, request: OwnMemberLookupRequest<'_, 'db>) -> RunResult<Member<'db>> {
        let scope = SlotSelectorEffects::body_scope(self, request.class).await?;
        self.allocate_future(|| {
            raw_class_member_with(scope, request.name, RawClassMemberFacts, self)
        })
        .await?
        .await
    }

    async fn slot_exists(&self, request: OwnMemberLookupRequest<'_, 'db>) -> RunResult<bool> {
        self.allocate_future(|| own_slot_descriptor_with(request.class, request.name, self))
            .await?
            .await
    }

    async fn generated_slots(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        generated_slots_with(class, self).await
    }

    async fn explicit_slots(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        own_class_binding_with(class, "__slots__", self).await
    }

    async fn implicit_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> RunResult<Member<'db>> {
        let attribute = self
            .allocate_future(|| {
                implicit_attribute_bindings_with(
                    request.class,
                    request.name,
                    MethodDecorator::ClassMethod,
                    self,
                )
            })
            .await?
            .await?;
        self.local(1, 0, || attribute.member()).await
    }

    async fn is_kw_only(&self, ty: Type<'db>) -> RunResult<bool> {
        self.own_member_is_instance_of(ty, KnownClass::KwOnly).await
    }

    async fn is_enum_member(&self, request: OwnMemberLookupRequest<'_, 'db>) -> RunResult<bool> {
        let Some(metadata) = self.access.enum_metadata(request.class).await? else {
            return Ok(false);
        };
        let (member_slots, alias_slots, name_bytes) = self
            .local(3, 0, || {
                (
                    slots(metadata.members.capacity()),
                    slots(metadata.aliases().capacity()),
                    request.name.len(),
                )
            })
            .await?;
        let member_slots = Self::checked(member_slots)?;
        let alias_slots = Self::checked(alias_slots)?;
        let work = Self::checked(
            member_slots
                .checked_add(alias_slots)
                .and_then(|backing| backing.checked_add(2))
                .and_then(|backing| {
                    name_bytes
                        .checked_add(1)
                        .and_then(|bytes| backing.checked_mul(bytes))
                }),
        )?;
        self.local(work, 0, || metadata.contains_member(request.name))
            .await
    }

    async fn is_enum_class(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let env = PairUnionEffects::environment(self, self.program).await?;
        self.is_enum_class_by_inheritance_source(class, &env).await
    }

    async fn dataclass_fields(&self) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::DataclassFields,
        ))
        .await
    }

    async fn named_tuple_field(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ClassCheck(ClassCheckOperation::NamedTuple))
            .await
    }

    async fn named_tuple_property(&self, _field_type: Type<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ClassCheck(ClassCheckOperation::NamedTuple))
            .await
    }

    async fn dunder_paramspec(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.dunder_callable_mapping(ty, DunderCallableTransform::DunderParamSpec)
            .await
    }

    async fn constructor_context(
        &self,
        function: FunctionType<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<FunctionType<'db>> {
        self.inherit_function_generic_context(function, context).await
    }

    async fn slot_descriptor(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::OwnMember(
            OwnMemberOperation::SlotDescriptor,
        ))
        .await
    }

    async fn synthesized_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.allocate_future(|| own_synthesized_member_with(request, self))
            .await?
            .await
    }

    async fn nonmember_value(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        nonmember_value_with(ty, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> synthesized_member::sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SynthesizedMemberEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn total_ordering(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .total_ordering(),
        )
        .await
    }

    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        static_code_generator_with(class, self).await
    }

    async fn checkpoint(&self, work: SynthesizedMemberWork) -> RunResult<()> {
        let work = match work {
            SynthesizedMemberWork::Admission { name_bytes } => {
                Self::checked(name_bytes.checked_add(1))?
            }
            _ => 1,
        };
        self.work(work).await
    }

    async fn total_ordering_member(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::OwnMember(
            OwnMemberOperation::TotalOrdering,
        ))
        .await
    }

    async fn frozen_subclass_member(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
        _method: FrozenDataclassMethod,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::OwnMember(
            OwnMemberOperation::FrozenSubclass,
        ))
        .await
    }

    async fn generated_member(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
        _field_policy: CodeGeneratorKind<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::OwnMember(
            OwnMemberOperation::GeneratedMember,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MroMemberEffects<'db, SourceMemberMroCursor<'db>>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: MroMemberWork) -> RunResult<()> {
        let extra = match work {
            MroMemberWork::PushAugmented { prefix_len } => prefix_len,
            MroMemberWork::InferAugmented { pending_len }
            | MroMemberWork::ClearAugmented { pending_len } => pending_len,
            _ => 0,
        };
        self.work(Self::checked(extra.checked_add(1))?).await
    }

    async fn known_class(&self, class: ClassType<'db>) -> RunResult<Option<KnownClass>> {
        let Some((class, _)) = self.static_class_identity(class).await? else {
            return Ok(None);
        };
        self.field(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .known(),
        )
        .await
    }

    async fn advance(
        &self,
        cursor: &mut SourceMemberMroCursor<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        let (done, skip) = self
            .local(3, 0, || {
                let first = std::mem::replace(&mut cursor.first, false);
                (
                    matches!(cursor.selection, SourceMemberMroSelection::Own) && !first,
                    matches!(cursor.selection, SourceMemberMroSelection::Inherited) && first,
                )
            })
            .await?;
        if done {
            return Ok(None);
        }
        if skip
            && mro_next_with(
                MroFieldReads::new(self.db()),
                &mut cursor.inner,
                MroDirection::Forward,
                self,
            )
            .await?
            .is_none()
        {
            return Ok(None);
        }
        mro_next_with(
            MroFieldReads::new(self.db()),
            &mut cursor.inner,
            MroDirection::Forward,
            self,
        )
        .await
    }

    async fn own_member(
        &self,
        class: ClassType<'db>,
        name: &str,
        context: Option<GenericContext<'db>>,
    ) -> RunResult<Member<'db>> {
        self.source_own_class_member(class, name, context).await
    }

    async fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> RunResult<Option<MroImplicitAttribute<'db>>> {
        let Some((class, _)) = self.static_class_identity(class).await? else {
            return Ok(None);
        };
        let attribute = self
            .allocate_future(|| {
                implicit_attribute_bindings_with(class, name, MethodDecorator::ClassMethod, self)
            })
            .await?
            .await?;
        self.local(1, 0, || {
            Some(MroImplicitAttribute::from_attribute(attribute))
        })
        .await
    }

    async fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> RunResult<()> {
        self.push_member_pending(pending, class, bindings).await
    }

    async fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> RunResult<()> {
        self.clear_member_pending(pending).await
    }

    async fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> RunResult<()> {
        self.finish_member_pending(pending).await
    }

    async fn infer_augmented(
        &self,
        _bindings: MroPendingBindings<'_, 'db>,
    ) -> RunResult<(Type<'db>, Provenance<'db>)> {
        self.unavailable(SourceOperation::OwnMember(
            OwnMemberOperation::AugmentedBindings,
        ))
        .await
    }

    async fn union_augmented(&self, first: Type<'db>, second: Type<'db>) -> RunResult<Type<'db>> {
        self.access.union_from_two_elements(first, second).await
    }

    async fn fall_back_to(
        &self,
        prior: LookupError<'db>,
        member: PlaceAndQualifiers<'db>,
    ) -> RunResult<LookupResult<'db>> {
        let env = PairUnionEffects::environment(self, self.program).await?;
        self.allocate_future(|| prior.or_fall_back_to_with(self.db(), &env, self, member))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NonmemberValueEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn is_nonmember(&self, ty: Type<'db>) -> RunResult<bool> {
        self.own_member_is_instance_of(ty, KnownClass::Nonmember)
            .await
    }

    async fn value(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        let name = self.local(1, 0, || Name::new_static("value")).await?;
        let result = self
            .access
            .member_lookup(ty, &name, MemberLookupPolicy::default())
            .await?;
        let parts = self.member_lookup_parts(result).await?;
        self.local(1, 0, || {
            parts
                .member
                .place
                .ignore_possibly_undefined()
                .unwrap_or(Type::unknown())
        })
        .await
    }
}
