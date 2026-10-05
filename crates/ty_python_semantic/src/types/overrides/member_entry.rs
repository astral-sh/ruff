use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::scope::ScopeId;

use super::{
    OverrideRulesConfig, PROHIBITED_NAMEDTUPLE_ATTRS, check_named_tuple_attribute,
    check_post_init_signature, check_remaining_class_declaration, lookup_override_member,
};
use crate::place::{DefinedPlace, Place, PlaceAndQualifiers};
use crate::types::class::CodeGeneratorKind;
use crate::types::context::InferContext;
use crate::types::enums::EnumMetadata;
use crate::types::list_members::MemberWithDefinition;
use crate::types::{ClassBase, ClassType, MemberLookupPolicy, StaticClassLiteral, Type};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(in crate::types) struct OverrideMemberRequest<'a, 'db> {
    pub(in crate::types) configuration: OverrideRulesConfig,
    pub(in crate::types) enum_info: Option<&'a EnumMetadata<'db>>,
    pub(in crate::types) class: ClassType<'db>,
    pub(in crate::types) scope: ScopeId<'db>,
    pub(in crate::types) bases: &'a [ClassBase<'db>],
    pub(in crate::types) member: &'a MemberWithDefinition<'db>,
}

pub(in crate::types) struct OverrideLookupFacts;

pub(in crate::types) struct OverrideGeneratorFacts;

#[derive(Clone, Copy)]
pub(in crate::types) enum OverrideGeneratorWork {
    NamedTupleName,
    PostInitName,
}

impl OverrideGeneratorWork {
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn comparisons(self) -> usize {
        match self {
            Self::NamedTupleName => PROHIBITED_NAMEDTUPLE_ATTRS.len(),
            Self::PostInitName => 1,
        }
    }
}

pub(super) struct OrdinaryOverrideLookupEffects<'a, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'a ProgramEnvironment<'db>,
}

pub(super) struct OrdinaryOverrideMemberEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousOverrideLookupEffects)]
    pub(in crate::types) trait OverrideLookupEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, name: &Name) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn class_member(&self, class: ClassType<'db>, name: &Name) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn instance_member(&self, instance: Type<'db>, name: &Name) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    }

    #[finite_capability]
    impl OverrideLookupFacts {
        fn is_new(&self, name: &Name) -> bool {
            name == "__new__"
        }
    }

    #[synchronous(lookup_override_member_sync)]
    #[capabilities(effects = OverrideLookupEffects, facts = OverrideLookupFacts)]
    #[passive_values()]
    pub(in crate::types) async fn lookup_override_member_with<'db, E: OverrideLookupEffects<'db>>(
        class: ClassType<'db>,
        name: &Name,
        facts: OverrideLookupFacts,
        effects: &E,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        effects.checkpoint(name).await?;
        if facts.is_new(name) {
            effects.class_member(class, name).await
        } else {
            let instance = effects.instance(class).await?;
            effects.instance_member(instance, name).await
        }
    }

    #[synchronous(SynchronousOverrideMemberEffects)]
    pub(in crate::types) trait OverrideMemberEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn lookup_member(&self, class: ClassType<'db>, name: &Name) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(source)]
        async fn static_class_literal(&self, class: ClassType<'db>) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
        #[operation(child)]
        async fn check_resolved_declaration(&self, request: OverrideMemberRequest<'_, 'db>, instance_of_class: Type<'db>, subclass_instance_member: PlaceAndQualifiers<'db>, type_on_subclass_instance: Type<'db>, literal: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(check_class_declaration_sync)]
    #[capabilities(effects = OverrideMemberEffects)]
    #[passive_values()]
    pub(in crate::types) async fn check_class_declaration_with<'db, E: OverrideMemberEffects<'db>>(
        request: OverrideMemberRequest<'_, 'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.checkpoint().await?;
        let instance_of_class = effects.instance(request.class).await?;
        let subclass_instance_member = effects.lookup_member(request.class, &request.member.member.name).await?;
        effects.checkpoint().await?;
        let Place::Defined(DefinedPlace { ty: type_on_subclass_instance, .. }) = subclass_instance_member.place else {
            return Ok(());
        };
        let literal = effects.static_class_literal(request.class).await?;
        effects.checkpoint().await?;
        let Some(literal) = literal else {
            return Ok(());
        };
        effects.check_resolved_declaration(request, instance_of_class, subclass_instance_member, type_on_subclass_instance, literal).await
    }

    #[synchronous(SynchronousResolvedOverrideMemberEffects)]
    pub(in crate::types) trait ResolvedOverrideMemberEffects<'db> {
        type Error;

        #[operation(child)]
        async fn code_generator(&self, literal: StaticClassLiteral<'db>) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;
        #[operation(checkpoint)]
        async fn generator_checkpoint(&self, work: OverrideGeneratorWork, name: &Name) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn named_tuple_attribute(&self, request: OverrideMemberRequest<'_, 'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn post_init_signature(&self, request: OverrideMemberRequest<'_, 'db>, policy: CodeGeneratorKind<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn remaining_declaration(&self, request: OverrideMemberRequest<'_, 'db>, instance_of_class: Type<'db>, subclass_instance_member: PlaceAndQualifiers<'db>, type_on_subclass_instance: Type<'db>, literal: StaticClassLiteral<'db>, class_kind: Option<CodeGeneratorKind<'db>>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl OverrideGeneratorFacts {
        fn check_named_tuple(&self, configuration: OverrideRulesConfig, name: &Name) -> bool {
            configuration.check_invalid_named_tuple_definitions()
                && PROHIBITED_NAMEDTUPLE_ATTRS.contains(&name.as_str())
        }

        fn check_post_init(&self, configuration: OverrideRulesConfig, name: &Name) -> bool {
            configuration.check_invalid_dataclasses() && name == "__post_init__"
        }
    }

    #[synchronous(check_resolved_class_declaration_sync)]
    #[capabilities(effects = ResolvedOverrideMemberEffects, facts = OverrideGeneratorFacts)]
    #[passive_values(OverrideGeneratorWork::NamedTupleName, OverrideGeneratorWork::PostInitName)]
    pub(in crate::types) async fn check_resolved_class_declaration_with<'db, E: ResolvedOverrideMemberEffects<'db>>(
        request: OverrideMemberRequest<'_, 'db>,
        instance_of_class: Type<'db>,
        subclass_instance_member: PlaceAndQualifiers<'db>,
        type_on_subclass_instance: Type<'db>,
        literal: StaticClassLiteral<'db>,
        facts: OverrideGeneratorFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        let class_kind = effects.code_generator(literal).await?;

        // Check for prohibited `NamedTuple` attribute overrides.
        //
        // `NamedTuple` classes have certain synthesized attributes (like `_asdict`, `_make`, etc.)
        // that cannot be overwritten. Attempting to assign to these attributes (without type
        // annotations) or define methods with these names will raise an `AttributeError` at runtime.
        match class_kind {
            Some(CodeGeneratorKind::NamedTuple) => {
                effects.generator_checkpoint(OverrideGeneratorWork::NamedTupleName, &request.member.member.name).await?;
                if facts.check_named_tuple(request.configuration, &request.member.member.name) {
                    effects.named_tuple_attribute(request).await?;
                }
            }
            Some(policy @ CodeGeneratorKind::DataclassLike(_)) => {
                effects.generator_checkpoint(OverrideGeneratorWork::PostInitName, &request.member.member.name).await?;
                if facts.check_post_init(request.configuration, &request.member.member.name) {
                    effects.post_init_signature(request, policy).await?;
                }
            }
            Some(CodeGeneratorKind::Pydantic(_) | CodeGeneratorKind::TypedDict) | None => {}
        }

        effects.remaining_declaration(request, instance_of_class, subclass_instance_member, type_on_subclass_instance, literal, class_kind).await
    }
}

impl<'db> SynchronousOverrideLookupEffects<'db> for OrdinaryOverrideLookupEffects<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self, _name: &Name) -> Result<(), Self::Error> {
        Ok(())
    }

    fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::instance(self.db, self.env, class))
    }

    fn class_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(class.class_member(self.db, self.env, name, MemberLookupPolicy::default()))
    }

    fn instance_member(
        &self,
        instance: Type<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(instance.member(self.db, self.env, name))
    }
}

impl<'db> SynchronousOverrideMemberEffects<'db> for OrdinaryOverrideMemberEffects<'_, 'db, '_> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::instance(
            self.context.db(),
            self.context.program_environment(),
            class,
        ))
    }

    fn lookup_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(lookup_override_member(
            self.context.db(),
            self.context.program_environment(),
            class,
            name,
        ))
    }

    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Self::Error> {
        Ok(class
            .static_class_literal(self.context.db())
            .map(|(literal, _)| literal))
    }

    fn check_resolved_declaration(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        instance_of_class: Type<'db>,
        subclass_instance_member: PlaceAndQualifiers<'db>,
        type_on_subclass_instance: Type<'db>,
        literal: StaticClassLiteral<'db>,
    ) -> Result<(), Self::Error> {
        check_resolved_class_declaration_sync(
            request,
            instance_of_class,
            subclass_instance_member,
            type_on_subclass_instance,
            literal,
            OverrideGeneratorFacts,
            self,
        )
    }
}

impl<'db> SynchronousResolvedOverrideMemberEffects<'db>
    for OrdinaryOverrideMemberEffects<'_, 'db, '_>
{
    type Error = Infallible;

    fn code_generator(
        &self,
        literal: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        Ok(CodeGeneratorKind::from_class(self.context.db(), literal.into()))
    }

    fn generator_checkpoint(
        &self,
        _work: OverrideGeneratorWork,
        _name: &Name,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn named_tuple_attribute(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
    ) -> Result<(), Self::Error> {
        check_named_tuple_attribute(self.context, request.scope, &request.member.member);
        Ok(())
    }

    fn post_init_signature(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        policy: CodeGeneratorKind<'db>,
    ) -> Result<(), Self::Error> {
        check_post_init_signature(
            self.context,
            request.class,
            &request.member.member,
            request.member.first_reachable_definition,
            policy,
        );
        Ok(())
    }

    fn remaining_declaration(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        instance_of_class: Type<'db>,
        subclass_instance_member: PlaceAndQualifiers<'db>,
        type_on_subclass_instance: Type<'db>,
        literal: StaticClassLiteral<'db>,
        class_kind: Option<CodeGeneratorKind<'db>>,
    ) -> Result<(), Self::Error> {
        check_remaining_class_declaration(
            self.context,
            request,
            instance_of_class,
            subclass_instance_member,
            type_on_subclass_instance,
            literal,
            class_kind,
        );
        Ok(())
    }
}
