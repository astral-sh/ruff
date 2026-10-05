use ruff_python_ast::PythonVersion;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{BindingWithConstraintsIterator, PlaceTable, UseDefMap};

use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::analysis::ClassCheckOperation;
use crate::place::{PlaceWithDefinition, RequiresExplicitReExport, place_from_bindings_with};
use crate::types::class::context::explicit_class_bases_with;
use crate::types::class::member_source::{MemberSourceEffects, MemberSourceWork, sealed};
use crate::types::class::slots::{
    InstanceLayout, SlotDefinition, SlotSelectorEffects, SlotSelectorWork,
    next_slot_binding_has_definition,
};
use crate::types::instance::tuple_spec::TupleSpecEffects;
use crate::types::{DataclassFlags, DataclassParams, KnownClass, StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MemberSourceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: MemberSourceWork) -> RunResult<()> {
        self.work(1).await
    }

    async fn place_table(&self, scope: ScopeId<'db>) -> RunResult<&'db PlaceTable> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let index = self.access.semantic_index(file).await?;
        let file_scope = self
            .field(scope.read_fields(self.db()).file_scope_id())
            .await?;
        self.local(1, 0, || index.place_table(file_scope))
            .await
    }

    async fn use_def_map(&self, scope: ScopeId<'db>) -> RunResult<&'db UseDefMap<'db>> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let index = self.access.semantic_index(file).await?;
        let file_scope = self
            .field(scope.read_fields(self.db()).file_scope_id())
            .await?;
        self.local(1, 0, || index.use_def_map(file_scope))
            .await
    }

    async fn symbol_id(
        &self,
        table: &'db PlaceTable,
        name: &str,
    ) -> RunResult<Option<ScopedSymbolId>> {
        let work = Self::checked(table.symbol_lookup_work(name.len()))?;
        self.local(work, 0, || table.symbol_id(name)).await
    }

    async fn binding_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> RunResult<PlaceWithDefinition<'db>> {
        self.environment_program(env).await?;
        let work = Self::checked(
            bindings
                .traversal_len()
                .checked_mul(4)
                .and_then(|count| count.checked_add(4)),
        )?;
        self.work(work).await?;
        self.allocate_future(|| {
            place_from_bindings_with(env, self, bindings, RequiresExplicitReExport::No, None)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SlotSelectorEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn body_scope(&self, class: StaticClassLiteral<'db>) -> RunResult<ScopeId<'db>> {
        self.field(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .body_scope(),
        )
        .await
    }

    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<DataclassParams<'db>>> {
        self.field_with_profile(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .dataclass_params(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn dataclass_flags(&self, params: DataclassParams<'db>) -> RunResult<DataclassFlags> {
        self.field(
            params
                .field_requests(self.access.endpoint().field_request_context())
                .flags(),
        )
        .await
    }

    async fn known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        self.field_with_profile(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .known(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field_with_profile(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .has_explicit_bases(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn slot_checkpoint(&self, work: SlotSelectorWork) -> RunResult<()> {
        self.work(Self::checked(work.work_units())?).await
    }

    async fn next_binding_has_definition<'map>(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
    ) -> RunResult<Option<bool>> {
        let work = Self::checked(SlotSelectorWork::BindingAdvance.work_units())?;
        self.local(work, 0, || next_slot_binding_has_definition(bindings))
            .await
    }

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        explicit_class_bases_with(class, self).await
    }

    async fn source_python_version(&self, scope: ScopeId<'db>) -> RunResult<PythonVersion> {
        let env = self
            .local(1, 0, || ProgramEnvironment::from_scope(scope))
            .await?;
        TupleSpecEffects::python_version(self, &env).await
    }

    async fn slot_definition(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db SlotDefinition> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::SlotDefinition,
        ))
        .await
    }

    async fn instance_layout(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db InstanceLayout> {
        self.access.instance_layout(class).await
    }

    async fn is_stub(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await?;
        self.file_is_stub(self.physical_file(file).await?).await
    }
}
