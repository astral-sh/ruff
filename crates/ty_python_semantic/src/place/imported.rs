use std::convert::Infallible;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::definition::Definition;
use ty_python_core::{ProgramFile, place_table};

use super::{LookupError, LookupResult, Place, PlaceAndQualifiers};
use crate::dunder_all::dunder_all_names;
use crate::types::{KnownClass, MemberLookupPolicy, Type};
use crate::{Db, ProgramEnvironment};

pub(crate) struct ImportedFallbackFacts;

pub(super) struct InlineImportedEffects<'db> {
    pub(super) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousReExportEffects)]
    pub(crate) trait ReExportEffects<'db> {
        type Error;

        #[operation(source)]
        async fn redundant_alias(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;

        #[operation(child)]
        async fn export_names(
            &self,
            definition: Definition<'db>,
        ) -> Result<Option<&'db FxHashSet<Name>>, Self::Error>;

        #[operation(source)]
        async fn definition_name(&self, definition: Definition<'db>) -> Result<&'db Name, Self::Error>;

        #[operation(local)]
        async fn contains_name(&self, names: &FxHashSet<Name>, name: &Name) -> Result<bool, Self::Error>;
    }

    // Returns `true` if the `definition` is re-exported.
    //
    // This will first check if the definition is using the "redundant alias" pattern like `import foo
    // as foo` or `from foo import bar as bar`. If it's not, it will check whether the symbol is being
    // exported via `__all__`.
    #[synchronous(is_reexported_sync)]
    #[capabilities(effects = ReExportEffects)]
    #[passive_values()]
    pub(crate) async fn is_reexported_with<'db, E: ReExportEffects<'db>>(
        definition: Definition<'db>,
        effects: &E,
    ) -> Result<bool, E::Error> {
        // This information is computed by the semantic index builder.
        if effects.redundant_alias(definition).await? {
            return Ok(true);
        }
        // At this point, the definition should either be an `import` or `from ... import` statement.
        // This is because the default value of `is_reexported` is `true` for any other kind of
        // definition.
        let Some(all_names) = effects.export_names(definition).await? else {
            return Ok(false);
        };
        let symbol_name = effects.definition_name(definition).await?;
        effects.contains_name(all_names, symbol_name).await
    }

    #[synchronous(SynchronousImportedFallbackEffects)]
    pub(crate) trait ImportedFallbackEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;

        #[operation(child)]
        async fn lookup_result(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            prior: PlaceAndQualifiers<'db>,
        ) -> Result<LookupResult<'db>, Self::Error>;

        #[operation(child)]
        async fn known_class_instance(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            class: KnownClass,
        ) -> Result<Type<'db>, Self::Error>;

        #[operation(child)]
        async fn member_lookup(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            name: &str,
            policy: MemberLookupPolicy,
        ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;

        #[operation(child)]
        async fn combine_fallback(
            &self,
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            prior: LookupError<'db>,
            fallback: PlaceAndQualifiers<'db>,
        ) -> Result<LookupResult<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ImportedFallbackFacts {
        fn place<'db>(&self, result: LookupResult<'db>) -> PlaceAndQualifiers<'db> {
            result.into()
        }

        fn bound<'db>(&self, ty: Type<'db>) -> PlaceAndQualifiers<'db> {
            Place::bound(ty).into()
        }

        fn undefined<'db>(&self) -> PlaceAndQualifiers<'db> {
            Place::Undefined.into()
        }

        fn any<'db>(&self) -> PlaceAndQualifiers<'db> {
            Place::bound(Type::any()).into()
        }
    }

    #[synchronous(imported_fallback_sync)]
    #[capabilities(effects = ImportedFallbackEffects, facts = ImportedFallbackFacts)]
    #[passive_values(KnownClass::Str, KnownClass::NoneType, KnownClass::ModuleType, MemberLookupPolicy::NO_GETATTR_LOOKUP)]
    pub(crate) async fn imported_fallback_with<'db, E: ImportedFallbackEffects<'db>>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: PlaceAndQualifiers<'db>,
        file: Option<ProgramFile<'db>>,
        name: &str,
        facts: ImportedFallbackFacts,
        effects: &E,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        effects.checkpoint().await?;
        let prior = match effects.lookup_result(db, env, prior).await? {
            Ok(found) => return Ok(facts.place(Ok(found))),
            Err(prior) => prior,
        };
        let fallback = match name {
            "__file__" => {
                // We special-case `__file__` here because we know that for a successfully imported
                // non-namespace-package Python module, that hasn't been explicitly overridden it
                // is always a string, even though typeshed says `str | None`. For a namespace package,
                // meanwhile, it will always be `None`.
                //
                // Note that C-extension modules (stdlib examples include `sys`, `itertools`, etc.)
                //  may not have a `__file__` attribute at runtime at all, but that doesn't really
                // affect the *type* of the attribute, just the *boundness*. There's no way for us
                // to know right now whether a stub represents a C extension or not, so for now we
                // do not attempt to detect this; we just infer `str` still. This matches the
                // behaviour of other major type checkers.
                let class = match file {
                    Some(_) => KnownClass::Str,
                    None => KnownClass::NoneType,
                };
                facts.bound(effects.known_class_instance(db, env, class).await?)
            }
            "__getattr__" => facts.undefined(),
            "__builtins__" => facts.any(),
            _ => {
                let module = effects.known_class_instance(db, env, KnownClass::ModuleType).await?;
                effects.member_lookup(db, env, module, name, MemberLookupPolicy::NO_GETATTR_LOOKUP).await?
            }
        };
        Ok(facts.place(effects.combine_fallback(db, env, prior, fallback).await?))
    }
}

impl<'db> SynchronousReExportEffects<'db> for InlineImportedEffects<'db> {
    type Error = Infallible;

    fn redundant_alias(&self, definition: Definition<'db>) -> Result<bool, Infallible> {
        Ok(definition.is_reexported(self.db))
    }

    fn export_names(
        &self,
        definition: Definition<'db>,
    ) -> Result<Option<&'db FxHashSet<Name>>, Infallible> {
        Ok(dunder_all_names(self.db, definition.program_file(self.db)))
    }

    fn definition_name(&self, definition: Definition<'db>) -> Result<&'db Name, Infallible> {
        let table = place_table(self.db, definition.scope(self.db));
        Ok(table
            .symbol(definition.place(self.db).expect_symbol())
            .name())
    }

    fn contains_name(&self, names: &FxHashSet<Name>, name: &Name) -> Result<bool, Infallible> {
        Ok(names.contains(name))
    }
}

impl<'db> SynchronousImportedFallbackEffects<'db> for InlineImportedEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn lookup_result(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Infallible> {
        Ok(prior.into_lookup_result(db, env))
    }

    fn known_class_instance(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Infallible> {
        Ok(class.to_instance(db, env))
    }

    fn member_lookup(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(ty.member_lookup_with_policy(db, env, name, policy))
    }

    fn combine_fallback(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        prior: LookupError<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Infallible> {
        Ok(prior.or_fall_back_to(db, env, fallback))
    }
}
