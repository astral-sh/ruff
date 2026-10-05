//! Implicit module-global and class-body selection shared by ordinary and suspended inference.

use std::convert::Infallible;

use ruff_python_ast::PythonVersion;
use ruff_python_ast::name::Name;
use ty_python_core::ProgramFile;

use super::implicit_globals::module_type_symbols;
use super::{DefinedPlace, Definedness, Place, PlaceAndQualifiers};
use crate::module_docstring;
use crate::types::{
    KnownClass, MemberLookupPolicy, Parameter, Parameters, Signature, Type, UnionType,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SpecialModuleGlobal {
    String,
    Bool,
    WarningRegistry,
    Annotate,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassBodySymbolEffects)]
    pub(crate) trait ClassBodySymbolEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, name: &str) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn known_instance(&self, env: &ProgramEnvironment<'db>, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn python_version_at_least(&self, env: &ProgramEnvironment<'db>, minimum: PythonVersion) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn union_two(&self, env: &ProgramEnvironment<'db>, first: Type<'db>, second: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(class_body_implicit_symbol_sync)]
    #[capabilities(effects = ClassBodySymbolEffects)]
    #[passive_values(KnownClass::Str, KnownClass::NoneType, KnownClass::Int, PythonVersion::PY313, Place::bound, Place::Undefined, PlaceAndQualifiers::from)]
    pub(crate) async fn class_body_implicit_symbol_with<'db, E: ClassBodySymbolEffects<'db>>(
        env: &ProgramEnvironment<'db>,
        name: &str,
        effects: &E,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        effects.checkpoint(name).await?;
        let ty = match name {
            "__qualname__" => effects.known_instance(env, KnownClass::Str).await?,
            "__module__" => effects.known_instance(env, KnownClass::Str).await?,
            // __doc__ is `str` if there's a docstring, `None` if there isn't
            "__doc__" => {
                let string = effects.known_instance(env, KnownClass::Str).await?;
                let none = effects.known_instance(env, KnownClass::NoneType).await?;
                effects.union_two(env, string, none).await?
            }
            // __firstlineno__ was added in Python 3.13
            "__firstlineno__" => {
                if !effects.python_version_at_least(env, PythonVersion::PY313).await? {
                    return Ok(PlaceAndQualifiers::from(Place::Undefined));
                }
                effects.known_instance(env, KnownClass::Int).await?
            }
            _ => return Ok(PlaceAndQualifiers::from(Place::Undefined)),
        };
        Ok(PlaceAndQualifiers::from(Place::bound(ty)))
    }

    #[synchronous(SynchronousModuleGlobalSymbolEffects)]
    pub(crate) trait ModuleGlobalSymbolEffects<'db> {
        type Error;

        #[operation(source)]
        async fn symbol_checkpoint(&self, db: &'db dyn Db, file: ProgramFile<'db>, name: &str) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn has_module_docstring(&self, db: &'db dyn Db, file: ProgramFile<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn python_version_at_least(&self, db: &'db dyn Db, file: ProgramFile<'db>, minimum: PythonVersion) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn special_type(&self, db: &'db dyn Db, file: ProgramFile<'db>, special: SpecialModuleGlobal) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn is_module_global(&self, db: &'db dyn Db, file: ProgramFile<'db>, name: &str) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn module_global_member(&self, db: &'db dyn Db, file: ProgramFile<'db>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    }

    #[synchronous(module_type_implicit_global_symbol_sync)]
    #[capabilities(effects = ModuleGlobalSymbolEffects)]
    #[passive_values(
        DefinedPlace::new,
        DefinedPlace::with_definedness,
        Definedness::PossiblyUndefined,
        Place::bound,
        Place::Defined,
        Place::Undefined,
        PlaceAndQualifiers::from,
        PythonVersion::PY314,
        SpecialModuleGlobal::String,
        SpecialModuleGlobal::Bool,
        SpecialModuleGlobal::WarningRegistry,
        SpecialModuleGlobal::Annotate,
        Type::any
    )]
    pub(crate) async fn module_type_implicit_global_symbol_with<'db, E: ModuleGlobalSymbolEffects<'db>>(
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        name: &str,
        effects: &E,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        effects.symbol_checkpoint(db, file, name).await?;
        match name {
            // We special-case `__file__` here because we know that for an internal implicit global
            // lookup in a Python module, it is always a string, even though typeshed says `str |
            // None`.
            "__file__" => {
                let ty = effects.special_type(db, file, SpecialModuleGlobal::String).await?;
                return Ok(PlaceAndQualifiers::from(Place::bound(ty)));
            }

            // We special-case `__doc__` because a module with a literal docstring has `__doc__`
            // set to that string at runtime. We only narrow when a docstring is present: `__doc__`
            // may be set dynamically, so we fall back to the typeshed's `str | None`.
            "__doc__" => {
                if effects.has_module_docstring(db, file).await? {
                    // Docstrings are stripped in `-OO` optimized mode, but here we assume that the
                    // existence of an actual docstring AND the usage of `__doc__` is reason enough to
                    // believe that it will exist at runtime.
                    let ty = effects.special_type(db, file, SpecialModuleGlobal::String).await?;
                    return Ok(PlaceAndQualifiers::from(Place::bound(ty)));
                }
            }

            "__builtins__" => return Ok(PlaceAndQualifiers::from(Place::bound(Type::any()))),

            "__debug__" => {
                let ty = effects.special_type(db, file, SpecialModuleGlobal::Bool).await?;
                return Ok(PlaceAndQualifiers::from(Place::bound(ty)));
            }

            // Created lazily by the warnings machinery; may be absent.
            // Model as possibly-unbound to avoid false negatives.
            "__warningregistry__" => {
                let ty = effects.special_type(db, file, SpecialModuleGlobal::WarningRegistry).await?;
                return Ok(PlaceAndQualifiers::from(Place::Defined(
                    DefinedPlace::with_definedness(DefinedPlace::new(ty), Definedness::PossiblyUndefined),
                )));
            }

            // Marked as possibly-unbound as it is only present in the module namespace
            // if at least one global symbol is annotated in the module.
            "__annotate__" => {
                if effects.python_version_at_least(db, file, PythonVersion::PY314).await? {
                    let ty = effects.special_type(db, file, SpecialModuleGlobal::Annotate).await?;
                    return Ok(PlaceAndQualifiers::from(Place::Defined(
                        DefinedPlace::with_definedness(DefinedPlace::new(ty), Definedness::PossiblyUndefined),
                    )));
                }
            }
            _ => {}
        }

        // In general we wouldn't check to see whether a symbol exists on a class before doing the
        // `.member()` call on the instance type -- we'd just do the `.member`() call on the instance
        // type, since it has the same end result. The reason to only call `.member()` on `ModuleType`
        // when absolutely necessary is that this function is used in a very hot path (name resolution
        // in `infer.rs`). We use less idiomatic (and much more verbose) code here as a micro-optimisation.
        if !effects.is_module_global(db, file, name).await? {
            return Ok(PlaceAndQualifiers::from(Place::Undefined));
        }
        effects.module_global_member(db, file, name).await
    }
}

pub(super) struct OrdinaryClassBodySymbolEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousClassBodySymbolEffects<'db> for OrdinaryClassBodySymbolEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _name: &str) -> Result<(), Infallible> {
        Ok(())
    }

    fn known_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> Result<Type<'db>, Infallible> {
        Ok(class.to_instance(self.db, env))
    }

    fn python_version_at_least(
        &self,
        env: &ProgramEnvironment<'db>,
        minimum: PythonVersion,
    ) -> Result<bool, Infallible> {
        Ok(env.python_version(self.db) >= minimum)
    }

    fn union_two(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(UnionType::from_two_elements(self.db, env, first, second))
    }
}

pub(super) struct OrdinaryModuleGlobalSymbolEffects;

impl<'db> SynchronousModuleGlobalSymbolEffects<'db> for OrdinaryModuleGlobalSymbolEffects {
    type Error = Infallible;

    fn symbol_checkpoint(
        &self,
        _db: &'db dyn Db,
        _file: ProgramFile<'db>,
        _name: &str,
    ) -> Result<(), Infallible> {
        Ok(())
    }

    fn has_module_docstring(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<bool, Infallible> {
        Ok(module_docstring(db, file.python_file(db)).is_some())
    }

    fn python_version_at_least(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        minimum: PythonVersion,
    ) -> Result<bool, Infallible> {
        Ok(ProgramEnvironment::from_file(file).python_version(db) >= minimum)
    }

    fn special_type(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        special: SpecialModuleGlobal,
    ) -> Result<Type<'db>, Infallible> {
        let env = ProgramEnvironment::from_file(file);
        Ok(match special {
            SpecialModuleGlobal::String => KnownClass::Str.to_instance(db, &env),
            SpecialModuleGlobal::Bool => KnownClass::Bool.to_instance(db, &env),
            SpecialModuleGlobal::WarningRegistry => KnownClass::Dict.to_specialized_instance(
                db,
                &env,
                &[Type::any(), KnownClass::Int.to_instance(db, &env)],
            ),
            SpecialModuleGlobal::Annotate => {
                let signature = Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "format",
                    )))
                    .with_annotated_type(KnownClass::Int.to_instance(db, &env))]),
                    KnownClass::Dict.to_specialized_instance(
                        db,
                        &env,
                        &[KnownClass::Str.to_instance(db, &env), Type::any()],
                    ),
                );
                Type::function_like_callable(db, signature)
            }
        })
    }

    fn is_module_global(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        name: &str,
    ) -> Result<bool, Infallible> {
        Ok(
            module_type_symbols(db, &ProgramEnvironment::from_file(file))
                .iter()
                .any(|module_type_member| &**module_type_member == name),
        )
    }

    fn module_global_member(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        let env = ProgramEnvironment::from_file(file);
        Ok(KnownClass::ModuleType
            .to_instance(db, &env)
            .member_lookup_with_policy(db, &env, name, MemberLookupPolicy::NO_GETATTR_LOOKUP))
    }
}
