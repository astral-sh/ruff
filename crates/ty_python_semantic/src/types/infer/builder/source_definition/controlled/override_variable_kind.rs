use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::BindingWithConstraintsIterator;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::place::PlaceAndQualifiers;
use crate::types::class::instance_storage::class_own_instance_member_with;
use crate::types::class::member_source::MemberSourceEffects;
use crate::types::class::own_member::OwnMemberLookupRequest;
use crate::types::class::synthesized_member::own_synthesized_member_with;
use crate::types::mro::base::class_mro_start_with;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::overrides::VariableKind;
use crate::types::overrides::variable_kind::{
    EffectiveVariableKindEffects, FunctionDefinitionEffects, VariableKindEffects,
    effective_variable_kind_with, is_function_definition_with, variable_kind_with,
};
use crate::types::{
    ClassBase, ClassLiteral, ClassType, MemberLookupPolicy, Specialization, StaticClassLiteral,
    Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn check_override_class_program(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<()> {
        self.work(1).await?;
        let literal = match class {
            ClassType::NonGeneric(literal) => self.initialize_value(|| literal).await?,
            ClassType::Generic(alias) => {
                let fields = self.access.endpoint().field_request_context();
                let origin = self.field(alias.field_requests(fields).origin()).await?;
                self.initialize_value(|| ClassLiteral::Static(origin))
                    .await?
            }
        };
        self.check_file_program(self.class_file(literal).await?)
            .await
    }

    pub(in crate::types::infer) async fn check_override_scope_program(
        &self,
        scope: ScopeId<'db>,
    ) -> RunResult<()> {
        self.check_file_program(self.scope_file(scope).await?).await
    }

    pub(in crate::types::infer) async fn infer_effective_variable_kind(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> RunResult<Option<VariableKind>> {
        self.check_override_class_program(class).await?;
        self.allocate_future(|| effective_variable_kind_with(class, name, self))
            .await?
            .await
    }

    pub(in crate::types::infer) async fn infer_is_function_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<bool> {
        self.check_override_scope_program(scope).await?;
        self.allocate_future(|| is_function_definition_with(scope, symbol, self))
            .await?
            .await
    }

    pub(super) async fn infer_variable_kind(
        &self,
        own: PlaceAndQualifiers<'db>,
        instance: PlaceAndQualifiers<'db>,
    ) -> RunResult<Option<VariableKind>> {
        self.allocate_future(|| variable_kind_with(own, instance, self))
            .await?
            .await
    }

    pub(super) async fn override_scope_symbol(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
    ) -> RunResult<(ScopeId<'db>, Option<(ScopedSymbolId, bool)>)> {
        let fields = self.access.endpoint().field_request_context();
        let scope = self
            .field(class.field_requests(fields).body_scope())
            .await?;
        let table = self.access.place_table(scope).await?;
        let id = MemberSourceEffects::symbol_id(self, table, name).await?;
        self.local(
            3,
            size_of::<(ScopeId<'db>, Option<(ScopedSymbolId, bool)>)>(),
            || {
                (
                    scope,
                    id.map(|id| {
                        let symbol = table.symbol(id);
                        (id, symbol.is_bound() || symbol.is_declared())
                    }),
                )
            },
        )
        .await
    }

    pub(super) async fn override_symbol_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<Option<Definition<'db>>> {
        let map = self.access.use_def_map(scope).await?;
        let mut declarations = self
            .initialize_value(|| map.end_of_scope_symbol_declarations(symbol))
            .await?;
        let count = self
            .local(1, size_of::<usize>(), || declarations.traversal_len())
            .await?;
        self.work(Self::checked(count.checked_add(1))?).await?;
        while let Some(definition) = self
            .local(1, size_of::<Option<Option<Definition<'db>>>>(), || {
                declarations
                    .next()
                    .map(|decl| decl.declaration.definition())
            })
            .await?
        {
            if definition.is_some() {
                return self.initialize_value(|| definition).await;
            }
        }
        let mut bindings = self
            .initialize_value(|| map.end_of_scope_symbol_bindings(symbol))
            .await?;
        let count = self
            .local(1, size_of::<usize>(), || bindings.traversal_len())
            .await?;
        self.work(Self::checked(count.checked_add(1))?).await?;
        while let Some(definition) = self
            .local(1, size_of::<Option<Option<Definition<'db>>>>(), || {
                bindings.next().map(|binding| binding.binding.definition())
            })
            .await?
        {
            if definition.is_some() {
                return self.initialize_value(|| definition).await;
            }
        }
        self.initialize_value(|| None).await
    }

    async fn override_definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        self.check_file_program(self.definition_file(definition).await?)
            .await?;
        let fields = self.access.endpoint().field_request_context();
        self.field(definition.read_fields(fields).kind()).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> VariableKindEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote =
            Self::checked(work).and_then(|work| Self::checked(bytes).map(|bytes| (work, bytes)));
        self.local_quoted(quote, action).await
    }

    async fn has_get_descriptor(&self, ty: Type<'db>) -> RunResult<bool> {
        let name = self
            .initialize_value(|| Name::new_static("__get__"))
            .await?;
        let policy = self.initialize_value(MemberLookupPolicy::default).await?;
        let member = self.access.class_member_lookup(ty, &name, policy).await?;
        self.local(1, size_of::<bool>(), || {
            member.place.ignore_possibly_undefined().is_some()
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EffectiveVariableKindEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn static_identity(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        self.work(1).await?;
        let identity = self.static_class_identity(class).await?;
        self.initialize_value(|| identity).await
    }

    async fn scope_symbol(
        &self,
        class: StaticClassLiteral<'db>,
        name: &Name,
    ) -> RunResult<(ScopeId<'db>, Option<(ScopedSymbolId, bool)>)> {
        self.override_scope_symbol(class, name).await
    }

    async fn synthesized_member(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &Name,
    ) -> RunResult<bool> {
        let request = self
            .initialize_value(|| OwnMemberLookupRequest {
                class,
                specialization,
                inherited_generic_context: None,
                name,
            })
            .await?;
        let member = self
            .allocate_future(|| own_synthesized_member_with(request, self))
            .await?
            .await?;
        self.initialize_value(|| member.is_some()).await
    }

    async fn is_function_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<bool> {
        self.access.is_function_definition(scope, symbol).await
    }

    async fn own_class_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let member = self.source_own_class_member(class, name, None).await?;
        self.initialize_value(|| member.inner).await
    }

    async fn own_instance_member(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let env = self
            .initialize_value(|| ProgramEnvironment::from_program(self.program))
            .await?;
        let member = self
            .allocate_future(|| class_own_instance_member_with(&env, class, name, self))
            .await?
            .await?;
        self.initialize_value(|| member.inner).await
    }

    async fn symbol_is_assignment(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<bool> {
        let Some(definition) = self.override_symbol_definition(scope, symbol).await? else {
            return self.initialize_value(|| false).await;
        };
        let kind = self.override_definition_kind(definition).await?;
        self.local(1, size_of::<bool>(), || {
            matches!(
                kind,
                DefinitionKind::Assignment(_) | DefinitionKind::AugmentedAssignment(_)
            )
        })
        .await
    }

    async fn mro_cursor(&self, class: ClassType<'db>) -> RunResult<MroCursor<'db>> {
        let start = self
            .allocate_future(|| {
                class_mro_start_with(MroFieldReads::new(self.db()), class, None, self)
            })
            .await?
            .await?;
        self.initialize_value(|| MroCursor::new(start.class, start.specialization))
            .await
    }

    async fn next_mro(&self, cursor: &mut MroCursor<'db>) -> RunResult<Option<ClassBase<'db>>> {
        let next = self
            .allocate_future(|| {
                mro_next_with(
                    MroFieldReads::new(self.db()),
                    cursor,
                    MroDirection::Forward,
                    self,
                )
            })
            .await?
            .await?;
        self.initialize_value(|| next).await
    }

    async fn effective_kind(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> RunResult<Option<VariableKind>> {
        self.access.effective_variable_kind(class, name).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionDefinitionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote =
            Self::checked(work).and_then(|work| Self::checked(bytes).map(|bytes| (work, bytes)));
        self.local_quoted(quote, action).await
    }

    async fn bindings(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<BindingWithConstraintsIterator<'db, 'db>> {
        let map = self.access.use_def_map(scope).await?;
        let bindings = self
            .initialize_value(|| map.end_of_scope_symbol_bindings(symbol))
            .await?;
        let count = self
            .local(1, size_of::<usize>(), || bindings.traversal_len())
            .await?;
        self.work(Self::checked(count.checked_add(1))?).await?;
        Ok(bindings)
    }

    async fn is_function(&self, definition: Definition<'db>) -> RunResult<bool> {
        let kind = self.override_definition_kind(definition).await?;
        self.local(1, size_of::<bool>(), || kind.is_function_def())
            .await
    }
}
