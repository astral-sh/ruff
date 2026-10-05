use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::BindingWithConstraintsIterator;
use ty_python_core::definition::Definition;
use ty_python_core::reachability_constraints::ScopedReachabilityConstraintId;
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;

use super::{SourceAccess, SourceEffects};
use crate::types::function::{FunctionIdentityEffects, FunctionType, OverloadLiteral};
use crate::types::infer::DefinitionTypes;
use crate::types::list_members::Member;
use crate::types::list_members::local_functions::{
    FunctionBindings, LocalDefinitions, LocalFunctionEffects, LocalFunctions,
    contains_definition_with, end_scope_functions_with, local_functions_from_type_with,
    local_member_functions_with, underlying_functions_with,
};
use crate::types::property_provenance::PropertyProvenanceEffects;
use crate::types::storage_quote::StorageQuote;
use crate::types::{BoundMethodType, PropertyInstanceType, Type, UnionType};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn local_member_functions(
        &self,
        member: &Member<'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<LocalFunctions<'db>> {
        self.check_file_program(self.scope_file(scope).await?)
            .await?;
        self.allocate_future(|| local_member_functions_with(member, scope, self))
            .await?
            .await
    }

    pub(in crate::types::infer) async fn underlying_functions(
        &self,
        ty: Type<'db>,
    ) -> RunResult<LocalFunctions<'db>> {
        self.allocate_future(|| underlying_functions_with(ty, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LocalFunctionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn local_quoted<T>(
        &self,
        quote: Option<StorageQuote>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        SourceEffects::local_quoted(
            self,
            quote
                .map(|quote| (quote.work, quote.bytes))
                .ok_or(RunError::Contract(
                    "local function storage quotation overflow",
                )),
            action,
        )
        .await
    }

    async fn initialize_value<T>(&self, action: impl FnOnce() -> T) -> RunResult<T> {
        SourceEffects::initialize_value(self, action).await
    }

    async fn underlying(&self, ty: Type<'db>) -> RunResult<LocalFunctions<'db>> {
        self.underlying_functions(ty).await
    }

    async fn from_type(
        &self,
        ty: Type<'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<LocalFunctions<'db>> {
        self.allocate_future(|| local_functions_from_type_with(ty, scope, self))
            .await?
            .await
    }

    async fn end_scope(
        &self,
        scope: ScopeId<'db>,
        name: &Name,
    ) -> RunResult<LocalDefinitions<'db>> {
        self.allocate_future(|| end_scope_functions_with(scope, name, self))
            .await?
            .await
    }

    async fn contains_definition(
        &self,
        function: FunctionType<'db>,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        self.allocate_future(|| contains_definition_with(function, definition, self))
            .await?
            .await
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.union_elements_source(union).await
    }

    async fn accessors(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> RunResult<[Option<Type<'db>>; 3]> {
        let getter = PropertyProvenanceEffects::getter(self, property).await?;
        let setter = PropertyProvenanceEffects::setter(self, property).await?;
        let deleter = PropertyProvenanceEffects::deleter(self, property).await?;
        self.initialize_value(|| [getter, setter, deleter]).await
    }

    async fn getter(&self, property: PropertyInstanceType<'db>) -> RunResult<Option<Type<'db>>> {
        PropertyProvenanceEffects::getter(self, property).await
    }

    async fn bound_callable(&self, method: BoundMethodType<'db>) -> RunResult<Type<'db>> {
        PropertyProvenanceEffects::bound_callable(self, method).await
    }

    async fn function_scope(&self, function: FunctionType<'db>) -> RunResult<ScopeId<'db>> {
        let definition = function.definition_with(self.db(), self).await?;
        self.definition_scope(definition).await
    }

    async fn function_name(&self, function: FunctionType<'db>) -> RunResult<&'db Name> {
        self.check_file_program(self.function_file(function).await?)
            .await?;
        PropertyProvenanceEffects::function_name(self, function).await
    }

    async fn names_equal(&self, left: &Name, right: &Name) -> RunResult<bool> {
        FunctionIdentityEffects::names_equal(self, left, right).await
    }

    async fn bindings(
        &self,
        scope: ScopeId<'db>,
        name: &Name,
    ) -> RunResult<Option<FunctionBindings<'db>>> {
        self.check_file_program(self.scope_file(scope).await?)
            .await?;
        let table = self.access.place_table(scope).await?;
        let work = self
            .local(2, size_of::<Option<usize>>(), || {
                table.symbol_lookup_work(name.len())
            })
            .await?;
        let symbol = self
            .local(
                Self::checked(work)?,
                size_of::<Option<ScopedSymbolId>>(),
                || table.symbol_id(name),
            )
            .await?;
        let Some(symbol) = symbol else {
            return self.initialize_value(|| None).await;
        };
        let uses = self.access.use_def_map(scope).await?;
        let bindings = self
            .local(
                4,
                size_of::<BindingWithConstraintsIterator<'db, 'db>>(),
                || uses.end_of_scope_symbol_bindings(symbol),
            )
            .await?;
        self.initialize_value(|| Some(FunctionBindings { uses, bindings }))
            .await
    }

    async fn reachable(
        &self,
        bindings: &FunctionBindings<'db>,
        constraint: ScopedReachabilityConstraintId,
    ) -> RunResult<bool> {
        let reachability = self
            .evaluate_reachability(
                bindings.uses.reachability_constraints(),
                bindings.uses.predicates(),
                constraint,
            )
            .await?;
        self.local(1, size_of::<bool>(), || !reachability.is_always_false())
            .await
    }

    async fn is_function(&self, definition: Definition<'db>) -> RunResult<bool> {
        self.check_file_program(self.definition_file(definition).await?)
            .await?;
        let fields = self.access.endpoint().field_request_context();
        let kind = self.field(definition.read_fields(fields).kind()).await?;
        self.local(1, size_of::<bool>(), || kind.is_function_def())
            .await
    }

    async fn inferred_function(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<Option<FunctionType<'db>>> {
        self.check_file_program(self.definition_file(definition).await?)
            .await?;
        let inference = self.access.definition(definition).await?;
        let entries = match &inference.types {
            DefinitionTypes::Other(types) => types.declarations.len(),
            _ => 1,
        };
        self.local(
            Self::checked(entries.checked_add(2))?,
            size_of::<Option<FunctionType<'db>>>(),
            || inference.function_type(definition),
        )
        .await
    }

    async fn overloads(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>)> {
        self.check_file_program(self.function_file(function).await?)
            .await?;
        function
            .overloads_and_implementation_with(self.db(), self)
            .await
    }

    async fn overload_definition(
        &self,
        overload: OverloadLiteral<'db>,
    ) -> RunResult<Definition<'db>> {
        FunctionIdentityEffects::definition(self, self.db(), overload).await
    }
}
