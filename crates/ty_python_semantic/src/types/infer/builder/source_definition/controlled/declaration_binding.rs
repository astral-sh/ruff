//! Controlled declaration and binding selection shares the ordinary compatibility checks.

use ruff_python_ast::AnyNodeRef;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;
#[cfg(debug_assertions)]
use ty_python_core::definition::{DefinitionCategory, DefinitionKind};
use ty_python_core::place::PlaceExprRef;
use ty_python_core::scope::FileScopeId;

use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::place::PlaceAndQualifiers;
use crate::place::implicit_symbol::module_type_implicit_global_symbol_with;
use crate::types::infer::builder::declaration_binding::{
    DeclarationBindingEffects, DeclarationBindingFacts, add_declaration_binding_with,
};
use crate::types::infer::builder::{DeclaredAndInferredType, TypeInferenceBuilder};
use crate::types::relation::source::assignability_condition;
use crate::types::{Type, TypeAndQualifiers};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Validates and stores declaration and binding types through [`add_declaration_binding_with`].
    /// Checks the definition category in debug builds and admits the shared computation before storage.
    pub(in crate::types::infer::builder) async fn add_source_declaration_with_binding(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        node: AnyNodeRef<'_>,
        definition: Definition<'db>,
        types: &DeclaredAndInferredType<'db>,
    ) -> RunResult<()> {
        #[cfg(debug_assertions)]
        {
            self.local(1, size_of::<&DefinitionKind<'db>>(), || ())
                .await?;
            let fields = self.access.endpoint().field_request_context();
            let kind = self.field(definition.read_fields(fields).kind()).await?;
            let in_stub = self.file_is_stub(builder.file()).await?;
            self.local(
                4,
                size_of::<DefinitionCategory>() + size_of::<bool>() * 2,
                || {
                    let category = kind.category(in_stub, builder.module());
                    debug_assert!(category.is_binding());
                    debug_assert!(category.is_declaration());
                },
            )
            .await?;
        }
        let types = self
            .local(1, size_of::<DeclaredAndInferredType<'db>>(), || {
                types.clone()
            })
            .await?;
        self.allocate_future(|| {
            add_declaration_binding_with(
                builder,
                node,
                definition,
                types,
                DeclarationBindingFacts,
                self,
            )
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> DeclarationBindingEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // Prepay fixed selections and logical transfers before either shared branch runs.
        self.local(
            16,
            size_of::<(TypeAndQualifiers<'db>, Type<'db>)>() * 2
                + size_of::<Type<'db>>() * 3
                + size_of::<Option<Type<'db>>>() * 2
                + size_of::<Option<&str>>()
                + size_of::<bool>() * 4,
            || (),
        )
        .await
    }

    async fn file_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<FileScopeId> {
        let scope = self.initialize_value(|| builder.scope()).await?;
        let fields = self.access.endpoint().field_request_context();
        self.field_with_profile(scope.read_fields(fields).file_scope_id(), &FixedFieldCopy)
            .await
    }

    async fn definition_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        scope: FileScopeId,
    ) -> RunResult<PlaceExprRef<'db>> {
        let fields = self.access.endpoint().field_request_context();
        let place = self
            .field_with_profile(definition.read_fields(fields).place_info(), &FixedFieldCopy)
            .await?;
        self.local(3, size_of::<PlaceExprRef<'db>>(), || {
            builder.index.place_table(scope).place(place.place())
        })
        .await
    }

    async fn implicit_global(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.allocate_future(|| {
            module_type_implicit_global_symbol_with(self.db(), builder.program_file(), name, self)
        })
        .await?
        .await
    }

    async fn assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<bool> {
        self.allocate_future(|| {
            assignability_condition(
                self.db(),
                builder.program_environment(),
                source,
                target,
                self,
            )
        })
        .await?
        .await
    }

    async fn invalid_implicit_global(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _node: AnyNodeRef<'_>,
        _place: PlaceExprRef<'db>,
        _declared: Type<'db>,
        _implicit: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::BindingDiagnostic).await
    }

    async fn validate_assignment(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        node: AnyNodeRef<'_>,
        definition: Definition<'db>,
        declared: Type<'db>,
        inferred: Type<'db>,
    ) -> RunResult<bool> {
        self.allocate_future(|| {
            self.validate_assignment_source(builder, node, definition, None, declared, inferred)
        })
        .await?
        .await
    }

    async fn discard_dict_key_assignments(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.local(2, size_of::<bool>(), || {
            builder.discard_dict_key_assignments_for(definition);
        })
        .await
    }

    async fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
        inferred: Type<'db>,
    ) -> RunResult<()> {
        self.store_source_declaration_and_binding(builder, definition, declared, inferred)
            .await
    }
}
