//! Annotation-only declarations validate prior bindings before storing their declared type.

use std::convert::Infallible;

use ruff_python_ast::AnyNodeRef;
use ty_python_core::definition::Definition;
use ty_python_core::place::PlaceExprRef;
use ty_python_core::scope::FileScopeId;

use super::TypeInferenceBuilder;
use crate::place::{
    LookupError, LookupResult, Place, PlaceAndQualifiers, module_type_implicit_global_symbol,
    place_from_bindings_with_reachability_cache,
};
use crate::types::diagnostic::INVALID_DECLARATION;
use crate::types::signatures::effects::legacy_inline;
use crate::types::{Type, TypeAndQualifiers, TypeQualifiers};

pub(in crate::types::infer) trait AddDeclarationEffects<'db, 'ast> {
    type Error;

    async fn local<T>(
        &self,
        work: usize,
        bytes: usize,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn step<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.local(1, size_of::<T>(), action).await
    }

    async fn prior_bindings(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        declaration: Definition<'db>,
    ) -> Result<Place<'db>, Self::Error>;

    async fn lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        place: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Self::Error>;

    async fn fallback_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        declaration: Definition<'db>,
    ) -> Result<(FileScopeId, PlaceExprRef<'db>), Self::Error>;

    async fn implicit_global(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

    async fn merge_fallback(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        error: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Self::Error>;

    async fn assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        inferred: Type<'db>,
        declared: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn invalid_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        node: AnyNodeRef<'_>,
        inferred: Type<'db>,
        declared: Type<'db>,
    ) -> Result<(), Self::Error>;

    async fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        declaration: Definition<'db>,
        ty: TypeAndQualifiers<'db>,
    ) -> Result<(), Self::Error>;
}

pub(in crate::types::infer) async fn add_declaration_with<
    'db,
    'ast,
    E: AddDeclarationEffects<'db, 'ast>,
>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    node: AnyNodeRef<'_>,
    declaration: Definition<'db>,
    declared: TypeAndQualifiers<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let prior = effects.prior_bindings(builder, declaration).await?;
    let prior = effects
        .step(|| prior.with_qualifiers(TypeQualifiers::empty()))
        .await?;
    let lookup = match effects.lookup(builder, prior).await? {
        Ok(ty) => effects.step(|| Ok(ty)).await?,
        Err(error) => {
            // Fallback to bindings declared on `types.ModuleType` if it's a global symbol
            let (scope, place) = effects.fallback_place(builder, declaration).await?;
            let name = effects
                .step(|| match place {
                    PlaceExprRef::Symbol(symbol) if scope.is_global() => {
                        Some(symbol.name().as_str())
                    }
                    _ => None,
                })
                .await?;
            let fallback = if let Some(name) = name {
                effects.implicit_global(builder, name).await?
            } else {
                effects.step(|| Place::Undefined.into()).await?
            };
            effects.merge_fallback(builder, error, fallback).await?
        }
    };
    // An absent binding imposes no compatibility constraint, so Undefined becomes Never.
    let inferred = effects
        .step(|| match lookup {
            Ok(ty) | Err(LookupError::PossiblyUndefined(ty)) => ty.inner_type(),
            Err(LookupError::Undefined(_)) => Type::Never,
        })
        .await?;
    let inner = effects.step(|| declared.inner_type()).await?;
    let declared = if effects.assignable(builder, inferred, inner).await? {
        effects.step(|| declared).await?
    } else {
        effects
            .invalid_declaration(builder, node, inferred, inner)
            .await?;
        effects
            .step(|| TypeAndQualifiers::declared(Type::unknown()))
            .await?
    };
    effects.store(builder, declaration, declared).await
}

pub(super) fn add_declaration_sync<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    node: AnyNodeRef<'_>,
    declaration: Definition<'db>,
    declared: TypeAndQualifiers<'db>,
) {
    legacy_inline(add_declaration_with(
        builder,
        node,
        declaration,
        declared,
        &OrdinaryAddDeclaration,
    ))
}

struct OrdinaryAddDeclaration;

impl<'db, 'ast> AddDeclarationEffects<'db, 'ast> for OrdinaryAddDeclaration {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: usize,
        _bytes: usize,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn prior_bindings(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        declaration: Definition<'db>,
    ) -> Result<Place<'db>, Infallible> {
        let db = builder.db();
        debug_assert!(
            declaration
                .kind(db)
                .category(builder.context.in_stub(), builder.module())
                .is_declaration()
        );
        let use_def = builder.index.use_def_map(declaration.file_scope(db));
        Ok(place_from_bindings_with_reachability_cache(
            db,
            builder.program_environment(),
            use_def.bindings_at_definition(declaration),
            builder.reachability_cache(),
        )
        .place)
    }

    async fn lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        place: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Infallible> {
        Ok(place.into_lookup_result(builder.db(), builder.program_environment()))
    }

    async fn fallback_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        declaration: Definition<'db>,
    ) -> Result<(FileScopeId, PlaceExprRef<'db>), Infallible> {
        let scope = builder.scope().file_scope_id(builder.db());
        Ok((
            scope,
            builder
                .index
                .place_table(scope)
                .place(declaration.place(builder.db())),
        ))
    }

    async fn implicit_global(
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

    async fn merge_fallback(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        error: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Infallible> {
        Ok(error.or_fall_back_to(builder.db(), builder.program_environment(), fallback))
    }

    async fn assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        inferred: Type<'db>,
        declared: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(inferred.is_assignable_to(builder.db(), builder.program_environment(), declared))
    }

    async fn invalid_declaration(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        node: AnyNodeRef<'_>,
        inferred: Type<'db>,
        declared: Type<'db>,
    ) -> Result<(), Infallible> {
        let db = builder.db();
        let env = builder.program_environment();
        if let Some(builder) = builder.context.report_lint(&INVALID_DECLARATION, node) {
            builder.into_diagnostic(format_args!(
                "Cannot declare type `{}` for inferred type `{}`",
                declared.display(db, env),
                inferred.display(db, env)
            ));
        }
        Ok(())
    }

    async fn store(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        declaration: Definition<'db>,
        ty: TypeAndQualifiers<'db>,
    ) -> Result<(), Infallible> {
        builder.declarations.insert(declaration, ty);
        Ok(())
    }
}
