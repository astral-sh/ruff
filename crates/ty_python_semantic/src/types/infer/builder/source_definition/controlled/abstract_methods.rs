//! Abstract-method discovery leaves results unpublished until every required child query completes.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::BindingWithConstraintsIterator;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::symbol::ScopedSymbolId;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::analysis::ClassCheckOperation;
use crate::place::{PlaceWithDefinition, RequiresExplicitReExport, place_from_declarations_with};
use crate::types::abstract_methods::{AbstractMethod, AbstractMethods};
use crate::types::abstract_methods::discovery::{
    AbstractMethodMap, AbstractScope, Accessor, CandidateEffects, DiscoveryEffects, Retention,
    abstract_methods_with, might_be_explicitly_abstract_with, type_as_abstract_method_with,
};
use crate::types::class::member_source::MemberSourceEffects;
use crate::types::class::own_member::OwnMemberLookupRequest;
use crate::types::class::protocol_status::static_is_protocol_with;
use crate::types::class::slots::own_slot_descriptor_with;
use crate::types::class::synthesized_member::own_synthesized_member_with;
use crate::types::function::{AbstractMethodKind, FunctionDecorators};
use crate::types::instance::effects::InstanceEffects;
use crate::types::mro::base::class_mro_start_with;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::property_provenance::PropertyProvenanceEffects;
use crate::types::{
    BoundMethodType, ClassBase, ClassLiteral, ClassType, FunctionType, PropertyInstanceType,
    StaticClassLiteral, Type,
};
use crate::{ProgramEnvironment, TypeQualifiers};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn class_abstract_methods(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<AbstractMethods<'db>> {
        let methods = self.access.abstract_methods(class).await?;
        self.local(1, 0, || AbstractMethods::from_methods(class, methods))
            .await
    }

    pub(in crate::types::infer) async fn infer_abstract_methods(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<AbstractMethodMap<'db>> {
        abstract_methods_with(class, self).await
    }

    pub(in crate::types::infer) async fn infer_might_be_explicitly_abstract(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        might_be_explicitly_abstract_with(definition, self).await
    }

    async fn require_unallocated_abstract_map(
        &self,
        methods: &AbstractMethodMap<'db>,
    ) -> RunResult<()> {
        if self
            .local(2, 0, || methods.is_empty() && methods.capacity() == 0)
            .await?
        {
            Ok(())
        } else {
            self.unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::AbstractMethods,
            ))
            .await
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> CandidateEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn kind(&self, definition: Definition<'db>) -> RunResult<&'db DefinitionKind<'db>> {
        self.check_file_program(self.definition_file(definition).await?)
            .await?;
        self.field(definition.read_fields(self.db()).kind()).await
    }

    async fn decorators(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<(FunctionDecorators, bool)> {
        let inference = self.access.function_known_decorators(definition).await?;
        self.local(3, 0, || {
            (
                inference.known_decorators(),
                inference.has_unknown_decorators(),
            )
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DiscoveryEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Mro = MroCursor<'db>;

    async fn empty(&self) -> RunResult<AbstractMethodMap<'db>> {
        self.local(
            size_of::<AbstractMethodMap<'db>>() * 2 + 1,
            0,
            AbstractMethodMap::default,
        )
        .await
    }

    async fn environment(&self, class: ClassType<'db>) -> RunResult<ProgramEnvironment<'db>> {
        let literal = DiscoveryEffects::literal(self, class).await?;
        let file = self.class_file(literal).await?;
        self.check_file_program(file).await?;
        self.local(2, 0, || ProgramEnvironment::from_file(file))
            .await
    }

    async fn mro(&self, class: ClassType<'db>) -> RunResult<Self::Mro> {
        let start = class_mro_start_with(MroFieldReads::new(self.db()), class, None, self).await?;
        self.local(1, size_of::<MroCursor<'db>>() * 2, || {
            MroCursor::new(start.class, start.specialization)
        })
        .await
    }

    async fn next_base(&self, mro: &mut Self::Mro) -> RunResult<Option<ClassBase<'db>>> {
        mro_next_with(
            MroFieldReads::new(self.db()),
            mro,
            MroDirection::Reverse,
            self,
        )
        .await
    }

    async fn literal(&self, class: ClassType<'db>) -> RunResult<ClassLiteral<'db>> {
        let (literal, _) =
            InstanceEffects::class_literal_and_specialization(self, self.db(), class).await?;
        Ok(literal)
    }

    async fn scope(&self, literal: StaticClassLiteral<'db>) -> RunResult<AbstractScope<'db>> {
        let scope = self
            .field(literal.field_requests(self.db()).body_scope())
            .await?;
        let places = MemberSourceEffects::place_table(self, scope).await?;
        let uses = MemberSourceEffects::use_def_map(self, scope).await?;
        let file = self.static_class_file(literal).await?;
        let implicit = !self.file_is_stub(self.physical_file(file).await?).await?
            && static_is_protocol_with(literal, self).await?;
        self.local(size_of::<AbstractScope<'db>>() * 2 + 1, 0, || {
            AbstractScope {
                literal,
                places,
                uses,
                implicit,
            }
        })
        .await
    }

    async fn retain(
        &self,
        methods: &mut AbstractMethodMap<'db>,
        _env: &ProgramEnvironment<'db>,
        _retention: Retention<'_, 'db>,
    ) -> RunResult<()> {
        self.require_unallocated_abstract_map(methods).await
    }

    async fn remove(&self, methods: &mut AbstractMethodMap<'db>, _name: &str) -> RunResult<()> {
        self.require_unallocated_abstract_map(methods).await
    }

    async fn dynamic_member_is_undefined(
        &self,
        class: ClassType<'db>,
        _env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> RunResult<bool> {
        let member = self.source_own_class_member(class, name, None).await?;
        self.local(1, 0, || member.is_undefined()).await
    }

    async fn synthesized(
        &self,
        class: StaticClassLiteral<'db>,
        _env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> RunResult<bool> {
        let member = self
            .allocate_future(|| {
                own_synthesized_member_with(
                    OwnMemberLookupRequest {
                        class,
                        name,
                        inherited_generic_context: None,
                        specialization: None,
                    },
                    self,
                )
            })
            .await?
            .await?;
        self.local(1, 0, || member.is_some()).await
    }

    async fn class_var(
        &self,
        scope: &AbstractScope<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> RunResult<bool> {
        let Some(symbol) = MemberSourceEffects::symbol_id(self, scope.places, name).await? else {
            return Ok(false);
        };
        let declarations = self
            .local(2, 0, || scope.uses.end_of_scope_symbol_declarations(symbol))
            .await?;
        let place = self
            .allocate_future(|| {
                place_from_declarations_with(
                    env,
                    self,
                    declarations,
                    RequiresExplicitReExport::No,
                    None,
                )
            })
            .await?
            .await?;
        self.local(2, 0, || {
            place
                .ignore_conflicting_declarations()
                .qualifiers
                .contains(TypeQualifiers::CLASS_VAR)
        })
        .await
    }

    async fn next_symbol(
        &self,
        scope: &AbstractScope<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<ScopedSymbolId>> {
        self.local(3, 0, || {
            let symbol = scope.uses.end_of_scope_symbol_at(*cursor);
            *cursor += usize::from(symbol.is_some());
            symbol
        })
        .await
    }

    async fn name(
        &self,
        scope: &AbstractScope<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<&'db Name> {
        self.local(1, 0, || scope.places.symbol(symbol).name())
            .await
    }

    async fn contains(&self, methods: &AbstractMethodMap<'db>, _name: &str) -> RunResult<bool> {
        self.require_unallocated_abstract_map(methods).await?;
        Ok(false)
    }

    async fn reachable(
        &self,
        scope: &AbstractScope<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<BindingWithConstraintsIterator<'db, 'db>> {
        self.local(4, 0, || scope.uses.reachable_symbol_bindings(symbol))
            .await
    }

    async fn next_definition(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'db, 'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        while let Some(binding) = self.local(4, 0, || bindings.next()).await? {
            if let Some(definition) = binding.binding.definition() {
                return Ok(Some(definition));
            }
        }
        Ok(None)
    }

    async fn candidate(&self, definition: Definition<'db>) -> RunResult<bool> {
        self.access.might_be_explicitly_abstract(definition).await
    }

    async fn binding(
        &self,
        scope: &AbstractScope<'db>,
        env: &ProgramEnvironment<'db>,
        symbol: ScopedSymbolId,
    ) -> RunResult<PlaceWithDefinition<'db>> {
        let bindings = self
            .local(2, 0, || scope.uses.end_of_scope_symbol_bindings(symbol))
            .await?;
        MemberSourceEffects::binding_place(self, env, bindings).await
    }

    async fn abstract_kind(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
    ) -> RunResult<Option<AbstractMethodKind>> {
        self.allocate_future(|| type_as_abstract_method_with(ty, class, self))
            .await?
            .await
    }

    async fn function_kind(
        &self,
        _function: FunctionType<'db>,
        _class: ClassType<'db>,
    ) -> RunResult<Option<AbstractMethodKind>> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::AbstractMethods,
        ))
        .await
    }

    async fn bound_function(&self, method: BoundMethodType<'db>) -> RunResult<Type<'db>> {
        PropertyProvenanceEffects::bound_callable(self, method).await
    }

    async fn accessor(
        &self,
        property: PropertyInstanceType<'db>,
        accessor: Accessor,
    ) -> RunResult<Option<Type<'db>>> {
        match accessor {
            Accessor::Getter => PropertyProvenanceEffects::getter(self, property).await,
            Accessor::Setter => PropertyProvenanceEffects::setter(self, property).await,
            Accessor::Deleter => PropertyProvenanceEffects::deleter(self, property).await,
        }
    }

    async fn insert(
        &self,
        _methods: &mut AbstractMethodMap<'db>,
        _name: &Name,
        _method: AbstractMethod<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::AbstractMethods,
        ))
        .await
    }

    async fn slot(&self, class: StaticClassLiteral<'db>, name: &str) -> RunResult<bool> {
        own_slot_descriptor_with(class, name, self).await
    }

    async fn finish(&self, methods: &mut AbstractMethodMap<'db>) -> RunResult<()> {
        self.require_unallocated_abstract_map(methods).await?;
        self.local(2, 0, || methods.shrink_to_fit()).await
    }
}
