//! Declaration and binding writes validate different types before selecting and storing a binding.

use std::convert::Infallible;

use ruff_python_ast::AnyNodeRef;
use ty_python_core::definition::Definition;
use ty_python_core::place::PlaceExprRef;
use ty_python_core::scope::FileScopeId;

use super::{DeclaredAndInferredType, TypeInferenceBuilder};
use crate::place::{PlaceAndQualifiers, module_type_implicit_global_symbol};
use crate::types::diagnostic::INVALID_DECLARATION;
use crate::types::{DynamicType, KnownInstanceType, Type, TypeAndQualifiers};

/// Classifies fixed type and place variants without resolving semantic dependencies.
#[derive(Clone, Copy, Debug)]
pub(super) struct DeclarationBindingFacts;

/// Supplies ordinary declaration diagnostics, compatibility checks, and final map writes.
#[derive(Clone, Copy, Debug)]
pub(super) struct OrdinaryDeclarationBindingEffects;

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies declaration compatibility, value validation, and the final writes to both maps.
    #[synchronous(SynchronousDeclarationBindingEffects)]
    pub(super) trait DeclarationBindingEffects<'db, 'ast> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn file_scope(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<FileScopeId, Self::Error>;
        #[operation(source)]
        async fn definition_place(&self, builder: &TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, scope: FileScopeId) -> Result<PlaceExprRef<'db>, Self::Error>;
        #[operation(child)]
        async fn implicit_global(&self, builder: &TypeInferenceBuilder<'db, 'ast>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn assignable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, source: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn invalid_implicit_global(&self, builder: &TypeInferenceBuilder<'db, 'ast>, node: AnyNodeRef<'_>, place: PlaceExprRef<'db>, declared: Type<'db>, implicit: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn validate_assignment(&self, builder: &TypeInferenceBuilder<'db, 'ast>, node: AnyNodeRef<'_>, definition: Definition<'db>, declared: Type<'db>, inferred: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn discard_dict_key_assignments(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn store(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, definition: Definition<'db>, declared: TypeAndQualifiers<'db>, inferred: Type<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl DeclarationBindingFacts {
        fn is_global(&self, scope: FileScopeId) -> bool {
            scope.is_global()
        }

        fn symbol_name<'db>(&self, place: PlaceExprRef<'db>) -> Option<&'db str> {
            match place {
                PlaceExprRef::Symbol(symbol) => Some(symbol.name().as_str()),
                PlaceExprRef::Member(_) => None,
            }
        }

        fn implicit_type<'db>(&self, place: PlaceAndQualifiers<'db>) -> Option<Type<'db>> {
            place.place.ignore_possibly_undefined()
        }

        fn declared_type<'db>(&self, declared: TypeAndQualifiers<'db>) -> Type<'db> {
            declared.inner_type()
        }

        const fn preserve_inferred_binding(&self, ty: Type<'_>) -> bool {
            // Dataclass field specifiers carry metadata in the inferred RHS type; replacing it with the
            // declared field type would lose settings like `init=False`.
            matches!(ty, Type::KnownInstance(KnownInstanceType::Field(_)))
        }

        const fn is_unknown(&self, ty: Type<'_>) -> bool {
            matches!(ty, Type::Dynamic(DynamicType::Unknown))
        }
    }

    /// Checks declaration and value compatibility, then stores the qualified declaration and
    /// selected binding. An invalid value uses the declared type and discards dictionary-key facts.
    #[synchronous(add_declaration_binding_sync)]
    #[capabilities(effects = DeclarationBindingEffects, facts = DeclarationBindingFacts)]
    #[passive_values()]
    pub(super) async fn add_declaration_binding_with<'db, 'ast, E: DeclarationBindingEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        node: AnyNodeRef<'_>,
        definition: Definition<'db>,
        types: DeclaredAndInferredType<'db>,
        facts: DeclarationBindingFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.checkpoint().await?;
        let (declared, inferred) = match types {
            DeclaredAndInferredType::AreTheSame(declared) => (declared, facts.declared_type(declared)),
            DeclaredAndInferredType::MightBeDifferent { declared_ty, inferred_ty } => {
                let scope = effects.file_scope(builder).await?;
                if facts.is_global(scope) {
                    let place = effects.definition_place(builder, definition, scope).await?;
                    if let Some(name) = facts.symbol_name(place) {
                        let implicit = effects.implicit_global(builder, name).await?;
                        if let Some(implicit) = facts.implicit_type(implicit) {
                            let declared_type = facts.declared_type(declared_ty);
                            if !effects.assignable(builder, declared_type, implicit).await? {
                                effects.invalid_implicit_global(builder, node, place, declared_type, implicit).await?;
                            }
                        }
                    }
                }
                let declared_type = facts.declared_type(declared_ty);
                let inferred = if effects.validate_assignment(builder, node, definition, declared_type, inferred_ty).await? {
                    // TODO We currently can't distinguish here between "no declared type" and
                    // "declared types is `Unknown` (e.g. due to a bad annotation, missing
                    // import, etc.)". Ideally we would still prefer `Unknown` declared type,
                    // but use inferred type if there is no declared type.
                    if !facts.preserve_inferred_binding(inferred_ty)
                        && !facts.is_unknown(declared_type)
                        && effects.assignable(builder, declared_type, inferred_ty).await?
                    {
                        declared_type
                    } else {
                        inferred_ty
                    }
                } else {
                    effects.discard_dict_key_assignments(builder, definition).await?;

                    // if the assignment is invalid, fall back to assuming the annotation is correct
                    declared_type
                };
                (declared_ty, inferred)
            }
        };
        effects.store(builder, definition, declared, inferred).await
    }
}

impl<'db, 'ast> SynchronousDeclarationBindingEffects<'db, 'ast>
    for OrdinaryDeclarationBindingEffects
{
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn file_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<FileScopeId, Infallible> {
        Ok(builder.scope().file_scope_id(builder.db()))
    }

    fn definition_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        scope: FileScopeId,
    ) -> Result<PlaceExprRef<'db>, Infallible> {
        Ok(builder
            .index
            .place_table(scope)
            .place(definition.place(builder.db())))
    }

    fn implicit_global(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(module_type_implicit_global_symbol(
            builder.db(),
            builder.program_file(),
            name,
        ))
    }

    fn assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(source.is_assignable_to(builder.db(), builder.program_environment(), target))
    }

    fn invalid_implicit_global(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        node: AnyNodeRef<'_>,
        place: PlaceExprRef<'db>,
        declared: Type<'db>,
        implicit: Type<'db>,
    ) -> Result<(), Infallible> {
        let db = builder.db();
        let env = builder.program_environment();
        if let Some(builder) = builder.context.report_lint(&INVALID_DECLARATION, node) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Cannot shadow implicit global attribute `{place}` \
                                    with declaration of type `{}`",
                declared.display(db, env)
            ));
            diagnostic.info(format_args!(
                "The global symbol `{}` \
                                    must always have a type assignable to `{}`",
                place,
                implicit.display(db, env)
            ));
        }
        Ok(())
    }

    fn validate_assignment(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        node: AnyNodeRef<'_>,
        definition: Definition<'db>,
        declared: Type<'db>,
        inferred: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(builder.validate_assignment_type(node, definition, None, declared, inferred))
    }

    fn discard_dict_key_assignments(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<(), Infallible> {
        builder.discard_dict_key_assignments_for(definition);
        Ok(())
    }

    fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        declared: TypeAndQualifiers<'db>,
        inferred: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.declarations.insert(definition, declared);
        builder.bindings.insert(definition, inferred);
        Ok(())
    }
}
