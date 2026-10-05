use salsa::execution_probe::{RunError, RunResult};
use ty_module_resolver::{FileModule, Module};

use super::class_selection::FixedFieldBorrow;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::analysis::ClassCheckOperation;
use crate::types::class::metaclass_reconciliation::most_derived_metaclass_with;
use crate::types::class::metaclass_selection::MetaclassSelectionResult;
use crate::types::class::namespace::NamespaceLookupEffects;
use crate::types::class::static_literal::inner_metaclass::{
    InheritedTransformEffects, InnerMetaclassEffects, MetaclassBases, inherited_transform_with,
    inner_metaclass_with, known_type_metaclass, next_metaclass_base_with,
    next_selected_metaclass_with,
};
use crate::types::class::{ClassMetaclass, KnownClassInstanceEffects};
use crate::types::class_base::conversion::{ClassBaseConversion, resolve_class_base_with};
use crate::types::class_base::metaclass::{ClassBaseMetaclassEffects, class_base_metaclass_with};
use crate::types::definition_expression::definition_expression_type_with;
use crate::types::local_transfer::generated_field_quote;
use crate::types::mro::construction::StaticMroEffects;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::{
    ClassBase, ClassLiteral, ClassType, DataclassTransformerParams, GenericAlias, KnownClass,
    MetaclassCandidate, Specialization, StaticClassLiteral, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Validates the class's program before a canonical metaclass read, including memo hits.
    pub(in crate::types::infer) async fn check_metaclass_program(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<()> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await
    }

    /// Runs the shared metaclass reducer with an admitted future and the existing source children.
    pub(in crate::types::infer) async fn infer_inner_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<MetaclassSelectionResult<'db>> {
        self.check_metaclass_program(class).await?;
        self.allocate_future(|| inner_metaclass_with(class, self))
            .await?
            .await
    }

    /// Admits the finite result copy returned from a borrowed canonical metaclass memo.
    pub(in crate::types::infer) async fn clone_metaclass_result(
        &self,
        result: &MetaclassSelectionResult<'db>,
    ) -> RunResult<MetaclassSelectionResult<'db>> {
        self.local(16, size_of::<MetaclassSelectionResult<'db>>(), || {
            result.clone()
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InnerMetaclassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        let bytes = Self::checked(
            size_of::<MetaclassSelectionResult<'db>>()
                .checked_mul(4)
                .and_then(|bytes| bytes.checked_add(size_of::<MetaclassCandidate<'db>>() * 2)),
        )?;
        self.local(32, bytes, || ()).await
    }

    async fn bases(&self, class: StaticClassLiteral<'db>) -> RunResult<MetaclassBases<'db>> {
        let scope = self
            .field(class.field_requests(self.db()).body_scope())
            .await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let bases = self.access.explicit_bases(class).await?;
        let bases = self.local(8, size_of::<MetaclassBases<'db>>() * 2, || MetaclassBases {
            class,
            env: ProgramEnvironment::from_file(file),
            base_env: ProgramEnvironment::from_scope(scope),
            bases,
            next: 0,
            pending: None,
                has_protocol_fallback: false,
                #[cfg(test)]
                retirement_observer: crate::types::infer::source_runtime::tests::inner_metaclass::observe_bases_created(self.db(), class),
        })
        .await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::inner_metaclass::observe_bases_ready(
            self.db(),
            class,
        );
        Ok(bases)
    }

    async fn take_pending(
        &self,
        bases: &mut MetaclassBases<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        self.local(2, size_of::<Option<ClassBase<'db>>>() * 2, || {
            bases.pending.take()
        })
        .await
    }

    async fn save_pending(
        &self,
        bases: &mut MetaclassBases<'db>,
        base: Option<ClassBase<'db>>,
    ) -> RunResult<()> {
        self.local(1, size_of::<Option<ClassBase<'db>>>(), || {
            bases.pending = base
        })
        .await
    }

    async fn next_raw(&self, bases: &mut MetaclassBases<'db>) -> RunResult<Option<Type<'db>>> {
        self.local(
            4,
            size_of::<Option<Type<'db>>>() + size_of::<usize>(),
            || {
                let next = bases.bases.get(bases.next).copied();
                bases.next += usize::from(next.is_some());
                next
            },
        )
        .await
    }

    async fn convert_base(
        &self,
        bases: &MetaclassBases<'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        let conversion = self
            .local(4, size_of::<ClassBaseConversion<'db>>(), || {
                ClassBaseConversion::from_type(ty)
            })
            .await?;
        resolve_class_base_with(
            conversion,
            &bases.base_env,
            Some(ClassLiteral::Static(bases.class)),
            self,
        )
        .await
    }

    async fn next_base(
        &self,
        bases: &mut MetaclassBases<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        self.local(4, size_of::<Option<ClassBase<'db>>>() * 2, || ())
            .await?;
        next_metaclass_base_with(bases, self).await
    }

    async fn base_metaclass(
        &self,
        bases: &MetaclassBases<'db>,
        base: ClassBase<'db>,
    ) -> RunResult<ClassMetaclass<'db>> {
        self.boxed_future_with_fixed_transfers(
            Ok((1, size_of::<ClassLiteral<'db>>())),
            || class_base_metaclass_with(base, &bases.env, ClassLiteral::Static(bases.class), self),
        )
        .await?
        .await
    }

    async fn record_fallback(&self, bases: &mut MetaclassBases<'db>) -> RunResult<()> {
        self.local(1, size_of::<bool>(), || bases.has_protocol_fallback = true)
            .await
    }

    async fn next_selected(
        &self,
        bases: &mut MetaclassBases<'db>,
    ) -> RunResult<Option<(ClassBase<'db>, Type<'db>)>> {
        self.local(
            4,
            size_of::<Option<(ClassBase<'db>, Type<'db>)>>() * 2,
            || (),
        )
        .await?;
        next_selected_metaclass_with(bases, self).await
    }

    async fn inheritance_cycle(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let cycle = self.access.inheritance_cycle(class).await?;
        self.local(1, size_of::<bool>(), || cycle.is_some()).await
    }

    async fn mro_is_cycle(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        StaticMroEffects::static_mro_is_cycle(self, class, None).await
    }

    async fn explicit_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        if !self
            .field(class.field_requests(self.db()).has_explicit_metaclass())
            .await?
        {
            return self.initialize_value(|| None).await;
        }
        let scope = self
            .field(class.field_requests(self.db()).body_scope())
            .await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let prepared = self.access.prepare_existing(file).await?;
        if prepared.file != file {
            return Err(RunError::Contract("prepared metaclass file is foreign"));
        }
        let file_scope = self
            .field(scope.read_fields(self.db()).file_scope_id())
            .await?;
        let class_node = self
            .local(2, size_of::<&ruff_python_ast::StmtClassDef>(), || {
                prepared
                    .index
                    .scope(file_scope)
                    .node()
                    .expect_class()
                    .node(&prepared.module)
            })
            .await?;
        let Some(arguments) = self
            .local(1, size_of::<Option<&ruff_python_ast::Arguments>>(), || {
                class_node.arguments.as_ref()
            })
            .await?
        else {
            return self.initialize_value(|| None).await;
        };
        let mut cursor = self.initialize_value(|| arguments.keywords.iter()).await?;
        while let Some(keyword) = self
            .local(2, size_of::<Option<&ruff_python_ast::Keyword>>(), || {
                cursor.next()
            })
            .await?
        {
            let length = self
                .local(2, size_of::<usize>(), || {
                    keyword.arg.as_ref().map_or(0, |name| name.len())
                })
                .await?;
            let selected = self
                .local(
                    Self::checked(length.checked_add(2))?,
                    size_of::<bool>(),
                    || keyword.arg.as_ref().is_some_and(|name| name == "metaclass"),
                )
                .await?;
            if selected {
                let definition = self
                    .local(
                        3,
                        size_of::<ty_python_core::definition::Definition<'db>>(),
                        || prepared.index.expect_single_definition(class_node),
                    )
                    .await?;
                let ty = definition_expression_type_with(definition, &keyword.value, self).await?;
                return self.initialize_value(|| Some(ty)).await;
            }
        }
        self.initialize_value(|| None).await
    }

    async fn specialization_has_typevars(
        &self,
        _env: &ProgramEnvironment<'db>,
        _alias: GenericAlias<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Metaclass))
            .await
    }

    async fn known_type(&self, env: &ProgramEnvironment<'db>) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        KnownClassInstanceEffects::class_literal(self, KnownClass::Type).await
    }

    async fn class_type(&self, ty: Type<'db>) -> RunResult<Option<ClassType<'db>>> {
        self.local(2, size_of::<Option<ClassType<'db>>>(), || ())
            .await?;
        KnownClassInstanceEffects::to_class_type(self, ty).await
    }

    async fn is_unknown_metaclass(&self, ty: Type<'db>) -> RunResult<bool> {
        self.local(8, size_of::<bool>(), || {
            ty == crate::types::SubclassOfType::subclass_of_unknown()
        })
        .await
    }

    async fn same_class(&self, left: ClassType<'db>, right: ClassType<'db>) -> RunResult<bool> {
        self.local(4, size_of::<bool>(), || left == right).await
    }

    async fn call_metaclass(
        &self,
        _bases: &MetaclassBases<'db>,
        _metaclass: Type<'db>,
    ) -> RunResult<MetaclassSelectionResult<'db>> {
        self.unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Metaclass))
            .await
    }

    async fn most_derived(
        &self,
        env: &ProgramEnvironment<'db>,
        candidate: ClassType<'db>,
        other: ClassType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.allocate_future(|| most_derived_metaclass_with(env, candidate, other, self))
            .await?
            .await
    }

    async fn transform_params(
        &self,
        metaclass: ClassType<'db>,
    ) -> RunResult<Option<DataclassTransformerParams<'db>>> {
        let Some((class, specialization)) = self.static_class_identity(metaclass).await? else {
            return self.initialize_value(|| None).await;
        };
        inherited_transform_with(class, specialization, self).await
    }

    async fn finish(
        &self,
        bases: &MetaclassBases<'db>,
        metaclass: ClassType<'db>,
    ) -> RunResult<ClassMetaclass<'db>> {
        let mut fallback = bases.has_protocol_fallback;
        if fallback {
            let known = self
                .field(bases.class.field_requests(self.db()).known())
                .await?;
            if let Some(known) = known {
                let program = self.environment_program(&bases.env).await?;
                let fields = self.access.endpoint().field_request_context();
                let environment = self
                    .field(program.field_requests(fields).resolver_environment())
                    .await?;
                let version = self
                    .field(environment.read_fields(fields).python_version())
                    .await?;
                fallback = self
                    .local(4, size_of::<bool>(), || {
                        !known_type_metaclass(known, version)
                    })
                    .await?;
            }
        }
        if fallback && let Some((class, _)) = self.static_class_identity(metaclass).await? {
            let known = self.field(class.field_requests(self.db()).known()).await?;
            if known == Some(KnownClass::Type) {
                return self
                    .initialize_value(|| ClassMetaclass::ProtocolFallback)
                    .await;
            }
        }
        self.initialize_value(|| ClassMetaclass::Selected(metaclass.into()))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InheritedTransformEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn own_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<DataclassTransformerParams<'db>>> {
        self.field(
            class
                .field_requests(self.db())
                .dataclass_transformer_params(),
        )
        .await
    }

    async fn mro(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<MroCursor<'db>> {
        self.local(3, size_of::<MroCursor<'db>>() * 2, || {
            MroCursor::new(class.into(), specialization)
        })
        .await
    }

    async fn next_mro(&self, cursor: &mut MroCursor<'db>) -> RunResult<Option<ClassBase<'db>>> {
        mro_next_with(
            MroFieldReads::new(self.db()),
            cursor,
            MroDirection::Forward,
            self,
        )
        .await
    }

    async fn static_literal(
        &self,
        base: ClassBase<'db>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let class = self
            .local(2, size_of::<Option<ClassType<'db>>>(), || base.into_class())
            .await?;
        let Some(class) = class else {
            return self.initialize_value(|| None).await;
        };
        let identity = self.static_class_identity(class).await?;
        self.local(2, size_of::<Option<StaticClassLiteral<'db>>>(), || {
            identity.map(|(class, _)| class)
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassBaseMetaclassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // One invocation has one variant dispatch, at most two predicate branches, and
        // fixed payload/result construction and transfers. The child operations admit reads.
        self.local_with_fixed_transfers(
            12,
            2 * size_of::<ClassBase<'db>>()
                + 2 * size_of::<ClassLiteral<'db>>()
                + 3 * size_of::<Type<'db>>()
                + 3 * size_of::<ClassMetaclass<'db>>()
                + 2 * size_of::<bool>(),
            || (),
        )
        .await
    }

    async fn class_metaclass(&self, class: ClassType<'db>) -> RunResult<ClassMetaclass<'db>> {
        self.type_parameter_future(|| NamespaceLookupEffects::inferred_metaclass(self, class))
            .await?
            .await
    }

    async fn subclass_is_stub(&self, subclass: ClassLiteral<'db>) -> RunResult<bool> {
        let file = self.type_parameter_future(|| self.class_file(subclass)).await?.await?;
        let physical = self.type_parameter_future(|| self.physical_file(file)).await?.await?;
        self.type_parameter_future(|| self.file_is_stub(physical)).await?.await
    }

    async fn subclass_is_standard_library(&self, subclass: ClassLiteral<'db>) -> RunResult<bool> {
        let file = self.type_parameter_future(|| self.class_file(subclass)).await?.await?;
        self.type_parameter_future(|| self.check_file_program(file)).await?.await?;
        let module = self.type_parameter_future(|| self.access.file_module(file)).await?.await?;
        self.local_with_fixed_transfers(3, size_of::<Option<Module<'db>>>(), || ())
            .await?;
        let Some(module) = module else {
            return Ok(false);
        };
        match module {
            Module::File(module) => {
                let quote = generated_field_quote(
                    |module: FileModule<'db>, context| module.field_requests(context),
                    |module: FileModule<'db>, context| module.search_path_request(context),
                );
                let path = self
                    .boxed_future_with_fixed_transfers(quote, || {
                        let endpoint = self.access.endpoint();
                        endpoint.read_field(
                            module.search_path_request(endpoint.field_request_context()),
                            &FixedFieldBorrow,
                        )
                    })
                    .await?
                    .await;
                self.local_with_fixed_transfers(2, size_of::<bool>(), || path.is_standard_library())
                    .await
            }
            Module::Namespace(_) => Ok(false),
        }
    }

    async fn known_class_literal(
        &self,
        env: &ProgramEnvironment<'db>,
        known: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.type_parameter_future(|| self.environment_program(env)).await?.await?;
        self.type_parameter_future(|| KnownClassInstanceEffects::class_literal(self, known))
            .await?
            .await
    }

    async fn known_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        known: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.type_parameter_future(|| self.environment_program(env)).await?.await?;
        self.type_parameter_future(|| self.infer_known_class_instance(known)).await?.await
    }
}
