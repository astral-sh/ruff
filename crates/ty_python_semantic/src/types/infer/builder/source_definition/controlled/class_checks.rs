//! Controlled class validation follows the shared post-inference phase order.

mod argument_checks;
mod base_checks;
mod base_typevars;
mod disjoint_bases;
mod final_values;
mod generic_bases;
mod generic_checks;
mod metaclass_checks;
mod mro_checks;
mod override_local_functions;
mod override_namedtuple;
mod override_remaining;
mod overrides;
mod total_ordering;

use ruff_python_ast::{self as ast, PythonVersion, name::Name};
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{BindingWithConstraintsIterator, PlaceTable, UseDefMap};

use super::class_selection::FixedFieldCopy;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::analysis::ClassCheckOperation;
use crate::place::PlaceWithDefinition;
use crate::types::class::context::explicit_class_bases_with;
use crate::types::class::instance_storage::{
    InstanceClassificationEffects, InstanceStorageWork, sealed as instance_storage_sealed,
    static_is_typed_dict_with,
};
use crate::types::class::member_source::{
    MemberSourceEffects, MemberSourceWork, StaticCodeGeneratorEffects,
    sealed as member_source_sealed,
};
use crate::types::class::metaclass_selection::static_inferred_metaclass_with;
use crate::types::class::protocol_status::{ProtocolStatusEffects, static_is_protocol_with};
use crate::types::class::static_literal::{
    InheritanceCycle, InheritanceCycleEffects, inheritance_cycle_with, static_finality_with,
};
use crate::types::class::{
    ClassInstanceFlags, ClassMetaclass, CodeGeneratorKind, InstanceLayout, SlotDefinition,
    SlotSelectorEffects, SlotSelectorWork, interpret_class_literal_lookup,
    next_slot_binding_has_definition, own_class_binding_with, slot_names_with,
    static_code_generator_with,
};
use crate::types::diagnostic::INVALID_GENERIC_CLASS;
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::post_inference::static_class::argument_checks::{
    ClassArgumentCheckFacts, check_arguments_with,
};
use crate::types::infer::builder::post_inference::static_class::base_checks::{
    ExplicitBaseCheckFacts, check_explicit_bases_with,
};
use crate::types::infer::builder::post_inference::static_class::dataclass_application::{
    DataclassApplicationEffects, check_dataclass_application_with,
};
use crate::types::infer::builder::post_inference::static_class::disjoint_decorator::{
    DisjointBaseDecoratorEffects, check_disjoint_base_decorator_with, next_class_decorator,
};
use crate::types::infer::builder::post_inference::static_class::enum_checks::{
    GenericEnumEffects, check_generic_enum_with,
};
use crate::types::infer::builder::post_inference::static_class::final_values::{
    ClassFinalValueFacts, check_class_final_without_value_with,
};
use crate::types::infer::builder::post_inference::static_class::generic_checks::{
    ClassGenericCheckFacts, check_generic_context_with,
};
use crate::types::infer::builder::post_inference::static_class::metaclass_checks::check_metaclass_with;
use crate::types::infer::builder::post_inference::static_class::mro_checks::check_mro_with;
use crate::types::infer::builder::post_inference::static_class::phases::{
    StaticClassBaseChecks, StaticClassDefinitionEffects, check_static_class_definitions_with,
};
use crate::types::infer::builder::post_inference::static_class::slot_checks::{
    ClassSlotCheckEffects, check_class_slots_with,
};
use crate::types::infer::builder::post_inference::static_class::total_ordering::check_total_ordering_with;
use crate::types::overrides::validation::{OverrideCheckFacts, check_class_with};
use crate::types::{
    ClassLiteral, ClassType, DataclassFlags, DataclassParams, GenericContext, KnownClass,
    StaticClassLiteral, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn check_scope_static_class(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
        class: &AstNodeRef<ast::StmtClassDef>,
    ) -> RunResult<()> {
        let class_node = self.local(1, 0, || class.node(builder.module())).await?;
        check_static_class_definitions_with(
            ty,
            class_node,
            &ClassCheckEffects {
                source: self,
                builder,
            },
        )
        .await
    }
}

struct ClassCheckEffects<'builder, 'access, 'run, 'db: 'run, 'ast, A> {
    source: &'builder SourceEffects<'access, 'run, 'db, A>,
    builder: &'builder TypeInferenceBuilder<'db, 'ast>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InheritanceCycleEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        self.source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .has_explicit_bases(),
            )
            .await
    }

    async fn inheritance_cycle(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<InheritanceCycle>> {
        self.source.access.inheritance_cycle(class).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> member_source_sealed::Sealed
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MemberSourceEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: MemberSourceWork) -> RunResult<()> {
        self.source.work(1).await
    }

    async fn place_table(&self, scope: ScopeId<'db>) -> RunResult<&'db PlaceTable> {
        let db = self.builder.db();
        let file = self.source.scope_file(scope).await?;
        self.source.check_file_program(file).await?;
        let source = self.source.access.prepare_existing(file).await?;
        self.source.check_file_program(source.file).await?;
        if source.file != file {
            return Err(RunError::Contract("prepared class scope file is foreign"));
        }
        let file_scope = self
            .source
            .field(scope.read_fields(db).file_scope_id())
            .await?;
        self.source
            .local(1, 0, || source.index.place_table(file_scope))
            .await
    }

    async fn use_def_map(&self, scope: ScopeId<'db>) -> RunResult<&'db UseDefMap<'db>> {
        let db = self.builder.db();
        let file = self.source.scope_file(scope).await?;
        self.source.check_file_program(file).await?;
        let source = self.source.access.prepare_existing(file).await?;
        self.source.check_file_program(source.file).await?;
        if source.file != file {
            return Err(RunError::Contract("prepared class scope file is foreign"));
        }
        let file_scope = self
            .source
            .field(scope.read_fields(db).file_scope_id())
            .await?;
        self.source
            .local(1, 0, || source.index.use_def_map(file_scope))
            .await
    }

    async fn symbol_id(
        &self,
        table: &'db PlaceTable,
        name: &str,
    ) -> RunResult<Option<ScopedSymbolId>> {
        let work = SourceEffects::<A>::checked(table.symbol_lookup_work(name.len()))?;
        self.source.local(work, 0, || table.symbol_id(name)).await
    }

    async fn binding_place<'map>(
        &self,
        _env: &ProgramEnvironment<'db>,
        _bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> RunResult<PlaceWithDefinition<'db>> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Slots))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SlotSelectorEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    async fn body_scope(&self, class: StaticClassLiteral<'db>) -> RunResult<ScopeId<'db>> {
        self.source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .body_scope(),
            )
            .await
    }

    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<DataclassParams<'db>>> {
        self.source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .dataclass_params(),
            )
            .await
    }

    async fn dataclass_flags(&self, params: DataclassParams<'db>) -> RunResult<DataclassFlags> {
        self.source
            .field(
                params
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .flags(),
            )
            .await
    }

    async fn known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        self.source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .known(),
            )
            .await
    }

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .has_explicit_bases(),
            )
            .await
    }

    async fn slot_checkpoint(&self, work: SlotSelectorWork) -> RunResult<()> {
        self.source
            .work(SourceEffects::<A>::checked(work.work_units())?)
            .await
    }

    async fn next_binding_has_definition<'map>(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
    ) -> RunResult<Option<bool>> {
        let work = SourceEffects::<A>::checked(SlotSelectorWork::BindingAdvance.work_units())?;
        self.source
            .local(work, 0, || next_slot_binding_has_definition(bindings))
            .await
    }

    async fn explicit_bases(&self, _class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Slots))
            .await
    }

    async fn source_python_version(&self, _scope: ScopeId<'db>) -> RunResult<PythonVersion> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Slots))
            .await
    }

    async fn slot_definition(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db SlotDefinition> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::SlotDefinition,
            ))
            .await
    }

    async fn instance_layout(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db InstanceLayout> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Slots))
            .await
    }

    async fn is_stub(&self, _class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Slots))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassSlotCheckEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn has_explicit_slots(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        own_class_binding_with(class, "__slots__", self).await
    }

    async fn dataclass_has_slots(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        let context = self.source.access.endpoint().field_request_context();
        let Some(parameters) = self
            .source
            .field(class.field_requests(context).dataclass_params())
            .await?
        else {
            return Ok(false);
        };
        let flags = self
            .source
            .field(parameters.field_requests(context).flags())
            .await?;
        self.source
            .local(3, 0, || flags.contains(DataclassFlags::SLOTS))
            .await
    }

    async fn report_dataclass_conflict(&self, _class: StaticClassLiteral<'db>) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::SlotDataclassConflict,
            ))
            .await
    }

    async fn in_stub(&self) -> RunResult<bool> {
        self.source
            .check_file_program(self.builder.program_file())
            .await?;
        let file = self.builder.context.file();
        self.source.file_is_stub(file).await
    }

    async fn slot_names(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<&'db [Name]>> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        slot_names_with(class, self).await
    }

    async fn check_namespace(
        &self,
        _class: StaticClassLiteral<'db>,
        _slot_names: &[Name],
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::SlotNamespace,
            ))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> GenericEnumEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn is_enum(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.source
            .is_enum_class_by_inheritance_source(class, self.builder.program_environment())
            .await
    }

    async fn has_generic_context(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        Ok(self
            .source
            .access
            .class_generic_context(class)
            .await?
            .is_some())
    }

    async fn report_generic_enum(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::StmtClassDef,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::GenericEnum,
            ))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StaticCodeGeneratorEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<DataclassParams<'db>>> {
        SlotSelectorEffects::dataclass_params(self, class).await
    }

    async fn known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        SlotSelectorEffects::known(self, class).await
    }

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        SlotSelectorEffects::has_explicit_bases(self, class).await
    }

    async fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .has_explicit_metaclass(),
            )
            .await
    }

    async fn checkpoint(&self, work: MemberSourceWork) -> RunResult<()> {
        self.source
            .work(match work {
                MemberSourceWork::ClassHeader => 4,
                _ => 1,
            })
            .await
    }

    async fn code_generator_query(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        // Class validation uses the same canonical code-generator query as member lookup.
        // Obtaining the receiver through source/access and binding it costs four operations;
        // evaluating and binding class costs three; the call and future forwarding add two.
        // The two tuples cover evaluated arguments and callee bindings; the helper separately
        // covers captures, the boxed future and fixed result transfers.
        let bytes = size_of::<[(&A, StaticClassLiteral<'db>); 2]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((9, bytes)), || {
                self.source.access.code_generator(class)
            })
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StaticClassDefinitionEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn check_inheritance_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        _class_node: &ast::StmtClassDef,
    ) -> RunResult<bool> {
        match inheritance_cycle_with(class, self).await? {
            None => Ok(false),
            Some(_) => {
                self.source
                    .unavailable(SourceOperation::ClassCheck(
                        ClassCheckOperation::InheritanceDiagnostic,
                    ))
                    .await
            }
        }
    }

    async fn check_slots(&self, class: StaticClassLiteral<'db>) -> RunResult<()> {
        check_class_slots_with(class, self).await
    }

    async fn check_generic_enum(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> RunResult<()> {
        check_generic_enum_with(class, class_node, self).await
    }

    async fn class_kind(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        static_code_generator_with(class, self).await
    }

    async fn check_named_tuple(&self, _class: StaticClassLiteral<'db>) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::NamedTuple))
            .await
    }

    async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        static_is_protocol_with(class, self.source).await
    }

    async fn check_disjoint_base_decorator(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        class_kind: Option<CodeGeneratorKind<'db>>,
        is_protocol: bool,
    ) -> RunResult<()> {
        check_disjoint_base_decorator_with(class, class_node, class_kind, is_protocol, self).await
    }

    async fn check_dataclass_application(
        &self,
        class: StaticClassLiteral<'db>,
        is_protocol: bool,
    ) -> RunResult<()> {
        check_dataclass_application_with(class, is_protocol, self).await
    }

    async fn check_explicit_bases<'node>(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &'node ast::StmtClassDef,
        class_kind: Option<CodeGeneratorKind<'db>>,
        is_protocol: bool,
    ) -> RunResult<StaticClassBaseChecks<'node, 'db>> {
        check_explicit_bases_with(
            class,
            class_node,
            class_kind,
            is_protocol,
            ExplicitBaseCheckFacts,
            self,
        )
        .await
    }

    async fn check_mro(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        bases: &mut StaticClassBaseChecks<'_, 'db>,
    ) -> RunResult<bool> {
        check_mro_with(class, class_node, bases, self).await
    }

    async fn check_total_ordering(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> RunResult<()> {
        check_total_ordering_with(class, class_node, self).await
    }

    async fn check_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> RunResult<()> {
        check_metaclass_with(class, class_node, self).await
    }

    async fn check_arguments(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        class_kind: Option<CodeGeneratorKind<'db>>,
    ) -> RunResult<()> {
        check_arguments_with(class, class_node, class_kind, ClassArgumentCheckFacts, self).await
    }

    async fn check_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
    ) -> RunResult<()> {
        check_generic_context_with(class, class_node, ClassGenericCheckFacts, self).await
    }

    async fn check_dataclass_fields(
        &self,
        _class: StaticClassLiteral<'db>,
        _class_node: &ast::StmtClassDef,
        _field_policy: CodeGeneratorKind<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DataclassFields,
            ))
            .await
    }

    async fn check_overrides(
        &self,
        class: StaticClassLiteral<'db>,
        inconsistent_generic_bases: bool,
    ) -> RunResult<()> {
        check_class_with(class, inconsistent_generic_bases, OverrideCheckFacts, self).await
    }

    async fn namespace_metaclass(&self, class: StaticClassLiteral<'db>) -> RunResult<Type<'db>> {
        let metaclass = static_inferred_metaclass_with(class, self.source).await?;
        self.source.work(1).await?;
        match metaclass {
            ClassMetaclass::Selected(ty) => Ok(ty),
            ClassMetaclass::ProtocolFallback => self.builtin_type().await,
        }
    }

    async fn builtin_type(&self) -> RunResult<Type<'db>> {
        let result = self
            .source
            .access
            .known_class_lookup(self.source.program, KnownClass::Type)
            .await?;
        self.source
            .local(2, 0, || match interpret_class_literal_lookup(result) {
                Some(class) => Type::ClassLiteral(ClassLiteral::Static(class)),
                None => Type::unknown(),
            })
            .await
    }

    async fn same_type(&self, left: Type<'db>, right: Type<'db>) -> RunResult<bool> {
        self.source.local(1, 0, || left == right).await
    }

    async fn check_namespace(
        &self,
        _class: StaticClassLiteral<'db>,
        _metaclass: Type<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Namespace))
            .await
    }

    async fn is_final(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        static_finality_with(class, self.source).await
    }

    async fn check_abstract_methods(
        &self,
        _class: StaticClassLiteral<'db>,
        _class_node: &ast::StmtClassDef,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::AbstractMethods,
            ))
            .await
    }

    async fn check_final_values(&self, class: StaticClassLiteral<'db>) -> RunResult<()> {
        check_class_final_without_value_with(class, ClassFinalValueFacts, self).await
    }

    async fn check_protocol_variance(&self, _class: StaticClassLiteral<'db>) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::ProtocolVariance,
            ))
            .await
    }

    async fn nominal_variance_enabled(&self) -> RunResult<bool> {
        self.source
            .is_lint_enabled_source(self.builder, &INVALID_GENERIC_CLASS)
            .await
    }

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.source.access.class_generic_context(class).await
    }

    async fn check_nominal_variance(
        &self,
        _class: StaticClassLiteral<'db>,
        _generic_context: GenericContext<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::ProtocolVariance,
            ))
            .await
    }

    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        static_is_typed_dict_with(class, self).await
    }

    async fn check_typed_dict(
        &self,
        _class: StaticClassLiteral<'db>,
        _class_node: &ast::StmtClassDef,
        _direct_typed_dict_bases: &[ClassType<'db>],
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::TypedDict))
            .await
    }

    async fn check_members(
        &self,
        _class: StaticClassLiteral<'db>,
        _field_policy: CodeGeneratorKind<'db>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Members))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> instance_storage_sealed::Sealed
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InstanceClassificationEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        SlotSelectorEffects::known(self, class).await
    }

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        SlotSelectorEffects::has_explicit_bases(self, class).await
    }

    async fn checkpoint(&self, _work: InstanceStorageWork) -> RunResult<()> {
        self.source.work(3).await
    }

    async fn instance_flags(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::TypedDict))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DisjointBaseDecoratorEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn next_decorator<'node>(
        &self,
        class_node: &'node ast::StmtClassDef,
        cursor: &mut usize,
    ) -> RunResult<Option<&'node ast::Decorator>> {
        self.source
            .local(1, 0, || next_class_decorator(class_node, cursor))
            .await
    }

    async fn expression_type(&self, _expression: &ast::Expr) -> RunResult<Type<'db>> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DisjointBaseDecorator,
            ))
            .await
    }

    async fn is_known_function(
        &self,
        function: FunctionType<'db>,
        known: KnownFunction,
    ) -> RunResult<bool> {
        let file = self.source.function_file(function).await?;
        self.source.check_file_program(file).await?;
        let context = self.source.access.endpoint().field_request_context();
        let literal = self
            .source
            .field(function.field_requests(context).literal())
            .await?;
        let function_known = self
            .source
            .field(literal.last_definition.field_requests(context).known())
            .await?;
        self.source
            .local(3, 0, || function_known == Some(known))
            .await
    }

    async fn report_typed_dict(
        &self,
        _class: StaticClassLiteral<'db>,
        _decorator: &ast::Decorator,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DisjointBaseDecorator,
            ))
            .await
    }

    async fn report_protocol(
        &self,
        _class: StaticClassLiteral<'db>,
        _decorator: &ast::Decorator,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DisjointBaseDecorator,
            ))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DataclassApplicationEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn has_dataclass_params(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        let params = self
            .source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .dataclass_params(),
            )
            .await?;
        self.source.local(1, 0, || params.is_some()).await
    }

    async fn has_named_tuple_class_in_mro(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<bool> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DataclassApplication,
            ))
            .await
    }

    async fn is_typed_dict(&self, _class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DataclassApplication,
            ))
            .await
    }

    async fn is_enum(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.source
            .is_enum_class_by_inheritance_source(class, self.builder.program_environment())
            .await
    }

    async fn report_named_tuple(&self, _class: StaticClassLiteral<'db>) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DataclassApplication,
            ))
            .await
    }

    async fn report_typed_dict(&self, _class: StaticClassLiteral<'db>) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DataclassApplication,
            ))
            .await
    }

    async fn report_enum(&self, _class: StaticClassLiteral<'db>) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DataclassApplication,
            ))
            .await
    }

    async fn report_protocol(&self, _class: StaticClassLiteral<'db>) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::DataclassApplication,
            ))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ProtocolStatusEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await?;
        let context = self.access.endpoint().field_request_context();
        self.field_with_profile(class.field_requests(context).known(), &FixedFieldCopy)
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

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        explicit_class_bases_with(class, self).await
    }

    async fn classify_bases(&self, bases: &[Type<'db>]) -> RunResult<bool> {
        self.local(4, size_of::<bool>(), || {
            StaticClassLiteral::protocol_explicit_bases(bases)
        })
        .await
    }
}
