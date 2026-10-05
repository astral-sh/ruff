//! Supplies admitted metadata and member lookup to the shared override continuation. The existing
//! `SourceEffects` endpoint owns execution, including semantic children that refuse before running
//! when no controlled provider is available.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use smallvec::SmallVec;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::place::PlaceAndQualifiers;
use crate::types::class::member_source::MemberSourceEffects;
use crate::types::class::own_member::OwnMemberLookupRequest;
use crate::types::class::synthesized_member::own_synthesized_member_with;
use crate::types::class::{CodeGeneratorKind, static_code_generator_with};
use crate::types::enums::EnumMetadata;
use crate::types::function::{FunctionDecorators, FunctionType};
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation, storage,
};
use crate::types::instance::{NominalClassFacts, nominal_known_class_with};
use crate::types::member::Member;
use crate::types::overrides::local_functions::{
    function_has_decorator_with, invalid_explicit_override_definition_with,
    missing_override_definition_with,
};
use crate::types::overrides::member_entry::{
    OverrideLookupEffects, OverrideMemberEffects, OverrideMemberRequest,
};
use crate::types::overrides::remaining::{OverrideReport, RemainingOverrideEffects};
use crate::types::overrides::{EnumConstructorMethod, MissingOverrideTarget, VariableKind};
use crate::types::{
    ClassBase, ClassLiteral, ClassType, KnownClass, Specialization, StaticClassLiteral, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RemainingOverrideEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote = SourceEffects::<A>::checked(work)
            .and_then(|work| SourceEffects::<A>::checked(bytes).map(|bytes| (work, bytes)));
        self.source.local_quoted(quote, action).await
    }

    async fn named_tuple_conflict(
        &self,
        literal: StaticClassLiteral<'db>,
        name: &Name,
    ) -> Result<Option<(ClassType<'db>, Option<Definition<'db>>)>, Self::Error> {
        self.source
            .allocate_future(|| {
                crate::types::overrides::namedtuple_fields::conflicting_named_tuple_field_with(
                    literal, name, self,
                )
            })
            .await?
            .await
    }

    async fn report(
        &self,
        _request: OverrideMemberRequest<'_, 'db>,
        _report: OverrideReport<'_, 'db>,
    ) -> Result<(), Self::Error> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
            .await
    }

    async fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error> {
        self.source
            .check_file_program(self.source.definition_file(definition).await?)
            .await?;
        let fields = self.source.access.endpoint().field_request_context();
        self.source
            .field(definition.read_fields(fields).kind())
            .await
    }

    async fn enum_value(
        &self,
        info: &EnumMetadata<'db>,
        name: &Name,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let (len, capacity) = self
            .source
            .local(2, size_of::<(usize, usize)>(), || {
                (info.members.len(), info.members.capacity())
            })
            .await?;
        let slots = storage::slots(capacity);
        let work = slots
            .and_then(|n| n.checked_add(len))
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(name.len().checked_add(1)?));
        RemainingOverrideEffects::local(self, work, Some(size_of::<Option<Type<'db>>>()), || {
            info.members.get(name).copied()
        })
        .await
    }

    async fn enum_auto(&self, info: &EnumMetadata<'db>, name: &Name) -> Result<bool, Self::Error> {
        let capacity = self
            .source
            .local(1, size_of::<usize>(), || info.auto_members.capacity())
            .await?;
        let work = storage::slots(capacity)
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(name.len().checked_add(1)?));
        RemainingOverrideEffects::local(self, work, Some(size_of::<bool>()), || {
            info.auto_members.contains(name)
        })
        .await
    }

    async fn is_ellipsis(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        self.source.work(1).await?;
        let Type::NominalInstance(instance) = ty else {
            return self.source.initialize_value(|| false).await;
        };
        let known = self
            .source
            .allocate_future(|| nominal_known_class_with(instance, NominalClassFacts, self.source))
            .await?
            .await?;
        self.source
            .local(1, size_of::<bool>(), || {
                known == Some(KnownClass::EllipsisType)
            })
            .await
    }

    async fn in_stub(&self) -> Result<bool, Self::Error> {
        let file = self.source.initialize_value(|| self.builder.file()).await?;
        let in_stub = self.source.file_is_stub(file).await?;
        self.source.initialize_value(|| in_stub).await
    }

    async fn enum_constructor(
        &self,
        _request: OverrideMemberRequest<'_, 'db>,
        _function: FunctionType<'db>,
        _receiver: Type<'db>,
        _value: Type<'db>,
        _method: EnumConstructorMethod,
    ) -> Result<(), Self::Error> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
            .await
    }

    async fn static_identity(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error> {
        self.source.work(1).await?;
        let identity = self.source.static_class_identity(class).await?;
        self.source.initialize_value(|| identity).await
    }

    async fn scope_symbol(
        &self,
        literal: StaticClassLiteral<'db>,
        name: &Name,
    ) -> Result<(ScopeId<'db>, Option<(ScopedSymbolId, bool)>), Self::Error> {
        self.source.override_scope_symbol(literal, name).await
    }

    async fn synthesized_member(
        &self,
        literal: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &Name,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let request = self
            .source
            .initialize_value(|| OwnMemberLookupRequest {
                class: literal,
                specialization,
                inherited_generic_context: None,
                name,
            })
            .await?;
        self.source
            .allocate_future(|| own_synthesized_member_with(request, self.source))
            .await?
            .await
    }

    async fn code_generator(
        &self,
        literal: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        self.source
            .allocate_future(|| static_code_generator_with(literal, self.source))
            .await?
            .await
    }

    async fn class_literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Self::Error> {
        self.source.work(1).await?;
        match class {
            ClassType::NonGeneric(literal) => self.source.initialize_value(|| literal).await,
            ClassType::Generic(alias) => {
                let fields = self.source.access.endpoint().field_request_context();
                let origin = self
                    .source
                    .field(alias.field_requests(fields).origin())
                    .await?;
                self.source.initialize_value(|| origin.into()).await
            }
        }
    }

    async fn own_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<Member<'db>, Self::Error> {
        self.source.source_own_class_member(class, name, None).await
    }

    async fn lookup_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        OverrideMemberEffects::lookup_member(self, class, name).await
    }

    async fn symbol_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        self.source.override_symbol_definition(scope, symbol).await
    }

    async fn first_declaration(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        let map = MemberSourceEffects::use_def_map(self, scope).await?;
        let mut declarations = self
            .source
            .initialize_value(|| map.end_of_scope_symbol_declarations(symbol))
            .await?;
        let work = self
            .source
            .local(1, size_of::<usize>(), || declarations.traversal_len())
            .await?;
        RemainingOverrideEffects::local(self, work.checked_add(1), Some(0), || ()).await?;
        while let Some(definition) = self
            .source
            .local(1, size_of::<Option<Option<Definition<'db>>>>(), || {
                declarations
                    .next()
                    .map(|decl| decl.declaration.definition())
            })
            .await?
        {
            if definition.is_some() {
                return self.source.initialize_value(|| definition).await;
            }
        }
        self.source.initialize_value(|| None).await
    }

    async fn functions(
        &self,
        ty: Type<'db>,
    ) -> Result<SmallVec<[FunctionType<'db>; 1]>, Self::Error> {
        self.source.underlying_functions(ty).await
    }

    async fn has_decorator(
        &self,
        function: FunctionType<'db>,
        decorator: FunctionDecorators,
    ) -> Result<bool, Self::Error> {
        self.source
            .allocate_future(|| function_has_decorator_with(function, decorator, self))
            .await?
            .await
    }

    async fn is_function_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<bool, Self::Error> {
        self.source.access.is_function_definition(scope, symbol).await
    }

    async fn effective_variable_kind(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> Result<Option<VariableKind>, Self::Error> {
        self.source.access.effective_variable_kind(class, name).await
    }

    async fn variable_kind(
        &self,
        own: PlaceAndQualifiers<'db>,
        instance: PlaceAndQualifiers<'db>,
    ) -> Result<Option<VariableKind>, Self::Error> {
        self.source.infer_variable_kind(own, instance).await
    }

    async fn is_subclass(
        &self,
        _child: ClassType<'db>,
        _parent: ClassType<'db>,
    ) -> Result<bool, Self::Error> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
            .await
    }

    async fn override_types(
        &self,
        _class: ClassType<'db>,
        _name: &Name,
        _subclass: Type<'db>,
        _superclass: Type<'db>,
    ) -> Result<Option<(Type<'db>, Type<'db>)>, Self::Error> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
            .await
    }

    async fn assignable(
        &self,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
            .await
    }

    async fn inherited_violation(
        &self,
        _bases: &[ClassBase<'db>],
        _owner: ClassType<'db>,
        _superclass: ClassType<'db>,
        _superclass_type: Type<'db>,
        _name: &Name,
    ) -> Result<bool, Self::Error> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
            .await
    }

    async fn fallback_member(
        &self,
        class: KnownClass,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        let instance = self
            .source
            .access
            .known_class_instance(self.source.program, class)
            .await?;
        OverrideLookupEffects::instance_member(self, instance, name).await
    }

    async fn missing_override(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
        _target: MissingOverrideTarget<'db>,
    ) -> Result<(), Self::Error> {
        let definition = self
            .source
            .allocate_future(|| {
                missing_override_definition_with(&request.member.member, request.scope, self)
            })
            .await?
            .await?;
        if self
            .source
            .local(1, size_of::<bool>(), || definition.is_some())
            .await?
        {
            self.source
                .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
                .await
        } else {
            self.source.initialize_value(|| ()).await
        }
    }

    async fn explicit_override(
        &self,
        request: OverrideMemberRequest<'_, 'db>,
    ) -> Result<(), Self::Error> {
        let definition = self
            .source
            .allocate_future(|| {
                invalid_explicit_override_definition_with(
                    &request.member.member,
                    request.scope,
                    self,
                )
            })
            .await?
            .await?;
        if self
            .source
            .local(1, size_of::<bool>(), || definition.is_some())
            .await?
        {
            self.source
                .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
                .await
        } else {
            self.source.initialize_value(|| ()).await
        }
    }
}
