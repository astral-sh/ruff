//! Class-object entry keeps the canonical key, source endpoint and optional callable guard.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::class_selection::{FixedFieldBorrow, FixedFieldCopy};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::PlaceAndQualifiers;
use crate::types::descriptor::{DescriptorRequest, DescriptorResult};
use crate::types::enums::EnumClassLiteral;
use crate::types::local_transfer::generated_field_quote;
use crate::types::member_lookup::class_object::ClassObjectEffects;
use crate::types::member_lookup::class_object_entry::{
    ClassObjectEntryEffects, ClassObjectEntryFacts, ClassObjectEntryRequest, ClassObjectEntryWork,
    class_object_entry_with,
};
use crate::types::member_lookup::general::GeneralMemberOperation;
use crate::types::{
    AttributeDescriptorEffects, AttributeDescriptorResult, CallableRecursionGuard, ClassLiteral,
    ClassType, DynamicType, InstanceFallbackShadowsNonDataDescriptor, LookupDescriptorEffects,
    LookupFacts, LookupParts, MemberEntryEffects, MemberLookupKey, MemberLookupPolicy,
    MemberLookupResult, SubclassOfInner, SubclassOfType, Type, attribute_descriptor_with,
    invoke_lookup_descriptor_with,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Resolves a class-object member through both descriptor stages and metaclass fallback.
    /// The existing caller has validated the key's program. The key fields are read in the
    /// ordinary executor's order; the same borrowed guard reaches both descriptor stages.
    pub(super) async fn class_object_entry(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Option<Type<'db>>,
        guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        let endpoint = self.access.endpoint();
        // These direct interned copy/borrow requests have the same generated shape audited
        // by generated_field_quote; the borrowed name is never cloned or traversed here.
        let ty = self
            .boxed_future_with_fixed_transfers(
                generated_field_quote(
                    |key: MemberLookupKey<'db>, context| key.field_requests(context),
                    |key: MemberLookupKey<'db>, context| key.field_requests(context).ty(),
                ),
                || {
                    endpoint.read_field(
                        key.field_requests(endpoint.field_request_context()).ty(),
                        &FixedFieldCopy,
                    )
                },
            )
            .await?
            .await;
        let name = self
            .boxed_future_with_fixed_transfers(
                generated_field_quote(
                    |key: MemberLookupKey<'db>, context| key.field_requests(context),
                    |key: MemberLookupKey<'db>, context| key.field_requests(context).name(),
                ),
                || {
                    endpoint.read_field(
                        key.field_requests(endpoint.field_request_context()).name(),
                        &FixedFieldBorrow,
                    )
                },
            )
            .await?
            .await;
        let policy = self
            .boxed_future_with_fixed_transfers(
                generated_field_quote(
                    |key: MemberLookupKey<'db>, context| key.field_requests(context),
                    |key: MemberLookupKey<'db>, context| key.field_requests(context).policy(),
                ),
                || {
                    endpoint.read_field(
                        key.field_requests(endpoint.field_request_context())
                            .policy(),
                        &FixedFieldCopy,
                    )
                },
            )
            .await?
            .await;
        let (request, effects) = self
            .local_with_fixed_transfers(9, 0, || {
                (
                    ClassObjectEntryRequest {
                        key,
                        ty,
                        name,
                        policy,
                        receiver,
                    },
                    SourceClassObjectEntry {
                        source: self,
                        guard,
                    },
                )
            })
            .await?;
        self.type_parameter_future(|| {
            class_object_entry_with(request, ClassObjectEntryFacts, &effects)
        })
        .await?
        .await
    }
}

/// Borrows source capabilities and the guard for one complete class-object lookup.
struct SourceClassObjectEntry<'effects, 'access, 'run, 'db: 'run, 'guard, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    guard: Option<&'guard CallableRecursionGuard<'db>>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassObjectEntryEffects<'db>
    for SourceClassObjectEntry<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: ClassObjectEntryWork) -> RunResult<()> {
        // Per phase, at most eight tag/option tests, eight field extractions and sixteen
        // fixed constructions/returns occur outside the admitted semantic children.
        // The enum shortcut includes Place::bound and its member-result conversions.
        // The copied slots cover the largest phase (enum selection or finalization);
        // width contributes to requested bytes only. No slot owns a variable payload.
        let bytes = const {
            size_of::<ClassObjectEntryRequest<'_, 'db>>()
                + 4 * size_of::<Type<'db>>()
                + 2 * size_of::<Option<Type<'db>>>()
                + 2 * size_of::<Option<EnumClassLiteral<'db>>>()
                + size_of::<Option<ClassType<'db>>>()
                + size_of::<Option<&Name>>()
                + 2 * size_of::<PlaceAndQualifiers<'db>>()
                + size_of::<AttributeDescriptorResult<'db>>()
                + size_of::<LookupParts<'db>>()
                + 2 * size_of::<MemberLookupResult<'db>>()
                + 2 * size_of::<SubclassOfInner<'db>>()
                + size_of::<ClassObjectEntryWork>()
        };
        self.source
            .local_with_fixed_transfers(32, bytes, || ())
            .await
    }

    async fn instance_approximation(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        self.source
            .type_parameter_future(|| ClassObjectEffects::instance_approximation(self.source, ty))
            .await?
            .await
    }

    async fn receiver_instance(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        let instance = self
            .source
            .type_parameter_future(|| ClassObjectEffects::instance_approximation(self.source, ty))
            .await?
            .await?;
        self.source
            .local_with_fixed_transfers(3, 0, || {
                instance.ok_or(RunError::Contract(
                    "class-object receiver is not instantiable",
                ))
            })
            .await?
    }

    async fn subclass_class(
        &self,
        inner: SubclassOfInner<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.source
            .type_parameter_future(|| ClassObjectEffects::subclass_inner_class(self.source, inner))
            .await?
            .await
    }

    async fn class_literal(&self, class: ClassType<'db>) -> RunResult<ClassLiteral<'db>> {
        self.source
            .type_parameter_future(|| self.source.equality_class_literal(class))
            .await?
            .await
    }

    async fn enum_class(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>> {
        self.source
            .type_parameter_future(|| self.source.access.enum_class_literal(class))
            .await?
            .await
    }

    async fn enum_member(
        &self,
        _class: EnumClassLiteral<'db>,
        _name: &Name,
    ) -> RunResult<Option<&'db Name>> {
        self.source
            .type_parameter_future(|| {
                self.source.unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::EnumLiteral,
                ))
            })
            .await?
            .await
    }

    async fn enum_literal(
        &self,
        _class: EnumClassLiteral<'db>,
        _name: &Name,
    ) -> RunResult<Type<'db>> {
        self.source
            .type_parameter_future(|| {
                self.source.unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::EnumLiteral,
                ))
            })
            .await?
            .await
    }

    async fn plain_member(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.source
            .type_parameter_future(|| self.source.class_object_member_value(ty, name, policy))
            .await?
            .await
    }

    async fn bind_self(&self, ty: Type<'db>, receiver: Type<'db>) -> RunResult<Type<'db>> {
        self.source
            .type_parameter_future(|| self.source.bind_member_self_type(ty, receiver))
            .await?
            .await
    }

    async fn attribute_descriptor(
        &self,
        attribute: PlaceAndQualifiers<'db>,
        receiver: Type<'db>,
    ) -> RunResult<AttributeDescriptorResult<'db>> {
        self.source
            .type_parameter_future(|| {
                attribute_descriptor_with(attribute, None, receiver, LookupFacts, self)
            })
            .await?
            .await
    }

    async fn result(&self, parts: LookupParts<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .type_parameter_future(|| self.source.access.member_result(parts))
            .await?
            .await
    }

    async fn invoke_descriptor(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
        fallback: MemberLookupResult<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        match self.guard {
            Some(guard) => {
                self.source
                    .type_parameter_future(|| {
                        self.source
                            .guarded_class_object_descriptor(key, receiver, fallback, guard)
                    })
                    .await?
                    .await
            }
            None => {
                self.source
                    .type_parameter_future(|| {
                        invoke_lookup_descriptor_with(
                            key,
                            receiver,
                            fallback,
                            InstanceFallbackShadowsNonDataDescriptor::Yes,
                            LookupFacts,
                            self.source,
                        )
                    })
                    .await?
                    .await
            }
        }
    }

    async fn fallback(
        &self,
        ty: Type<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .type_parameter_future(|| {
                MemberEntryEffects::fallback(self.source, ty, name, result, policy)
            })
            .await?
            .await
    }

    async fn typevar_upper_bound(
        &self,
        _subclass: SubclassOfType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .type_parameter_future(|| {
                self.source.unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::ClassObjectTypeVarUpperBound,
                ))
            })
            .await?
            .await
    }

    async fn promote(&self, result: MemberLookupResult<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .type_parameter_future(|| MemberEntryEffects::promote(self.source, result))
            .await?
            .await
    }

    async fn dynamic_result(
        &self,
        _result: MemberLookupResult<'db>,
        _dynamic: DynamicType<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .type_parameter_future(|| {
                self.source.unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::ClassObjectDynamicResult,
                ))
            })
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> AttributeDescriptorEffects<'db>
    for SourceClassObjectEntry<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // Including PlaceAndQualifiers::map_type, the recovery arm is bounded by eight
        // tag/option tests, twelve field extractions, sixteen carrier constructions
        // and twelve fixed transfers/returns. Each category is independent of width.
        let bytes = const {
            2 * size_of::<DescriptorRequest<'db>>()
                + 3 * size_of::<DescriptorResult<'db>>()
                + 2 * size_of::<PlaceAndQualifiers<'db>>()
                + 2 * size_of::<AttributeDescriptorResult<'db>>()
        };
        self.source
            .local_with_fixed_transfers(48, bytes, || ())
            .await
    }

    async fn descriptor(
        &self,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        match self.guard {
            Some(guard) => {
                self.source
                    .type_parameter_future(|| self.source.guarded_member_descriptor(request, guard))
                    .await?
                    .await
            }
            None => {
                self.source
                    .type_parameter_future(|| {
                        LookupDescriptorEffects::descriptor(self.source, request)
                    })
                    .await?
                    .await
            }
        }
    }
}
